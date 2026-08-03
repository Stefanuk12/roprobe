-- Dependencies
local broker = require("./lib/broker")
local buildSandbox = require("./lib/sandbox")
local task = require("@lune/task")

-- Constants
--- Longer than `ConsoleRelay`'s flush interval, so one batch is one frame.
local SETTLE = 0.5

--- The client's console output, end to end: `LogService` and the executor's own
--- globals through [`ConsoleRelay`], over a real socket to a real broker, out
--- to a control connection standing in for the extension.
return {
    name = "console relay",

    run = function(check: (boolean, string) -> ())
        local binary = broker.binary() :: string
        local client = buildSandbox()

        -- Capture what the hook passes through, rather than letting it hit stdout.
        local printed: { string } = {}
        client.env.print = function(...)
            local parts = {}
            for index = 1, select("#", ...) do
                parts[index] = tostring((select(index, ...)))
            end
            table.insert(printed, table.concat(parts, " "))
        end
        local originalPrint = client.env.print

        local Messages = client.load("src/lib/Messages")
        local ConsoleRelay = client.load("src/features/consoleRelay")
        local WebSocketClient = client.load("src/engine/WebSocketClient")

        local running = broker.start(binary)
        local control = broker.control(client, running)

        -- The client end, wired the way `@features/broker` wires it.
        local socket = WebSocketClient.new(running.url)
        socket:Initialise()

        local relay = ConsoleRelay.new()
        relay.onBatch:Connect(function(entries)
            socket:Send(Messages.encode({ type = "Log", content = entries }))
        end)
        relay:Initialise()
        task.wait(0.2)

        -- One batch: the game's output, the executor's, and a throw.
        client.logService.MessageOut:Fire("from the game", client.env.Enum.MessageType.MessageWarning)
        client.env.print("from the executor", 42)
        local threw, thrown = pcall(function()
            client.env.error("boom")
        end)

        local index, batch = control.waitFor("SessionLog", 0)
        local entries: { any } = if batch then batch.entries else {}
        local contentByLevel: { [string]: string } = {}
        for _, entry in entries do
            contentByLevel[entry.level] = entry.content
        end
        local errorContent = contentByLevel.Error or ""

        check(index ~= nil, "the broker relays the client's console output")
        check(#entries == 3, `the flush interval batches the lines into one frame (got {#entries})`)
        check(contentByLevel.Warn == "from the game", "a LogService line arrives at its own level")
        check(contentByLevel.Print == "from the executor 42", "the hooked print arrives, joined as print joins")
        check(printed[1] == "from the executor 42", "the hooked print still reaches the executor's console")
        check(not threw, "the hooked error still throws")
        -- The relayed line has to read exactly as the throw the caller saw, or
        -- the hook's `blame` arithmetic is pointing at the wrong frame.
        local blamed = string.split(errorContent, "\n")[1]
        local blameLabel = "the hooked error is blamed at the throw site"
        if blamed ~= tostring(thrown) then
            blameLabel ..= ` (relayed {blamed}, threw {thrown})`
        end
        check(blamed == tostring(thrown), blameLabel)
        check(string.find(errorContent, "\nStack Begin\n") ~= nil, "the hooked error carries a traceback")

        -- Tearing the relay down has to put the globals back and stop the output.
        relay:Destroy()
        check(client.env.print == originalPrint, "destroying the relay reverts the hooked globals")

        client.env.print("after teardown")
        client.logService.MessageOut:Fire("after teardown", client.env.Enum.MessageType.MessageOutput)
        task.wait(SETTLE)
        local later = control.waitFor("SessionLog", index or 0, 0.1)
        check(later == nil, "no output is relayed once the relay is destroyed")

        socket:Destroy()
        control.close()
        running.stop()
    end,
}
