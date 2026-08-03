-- Dependencies
local fs = require("@lune/fs")
local net = require("@lune/net")
local process = require("@lune/process")
local sandbox = require("./sandbox")
local serde = require("@lune/serde")
local task = require("@lune/task")

-- Constants
--- A debug build first: these tests exist to check the broker in this repo, and
--- the staged copy under `extension/bin` is only refreshed on a bundle.
local BINARIES = {
    "../broker/target/debug/broker",
    "../broker/target/release/broker",
}
--- Upstreams stay off: the tests drive the client end, not verde or luau-lsp.
local ARGUMENTS = { "run", "--no-verde", "--no-luau-lsp", "--handshake=stdout" }
local POLL_INTERVAL = 0.05

-- Types
type Sandbox = sandbox.Sandbox
export type Running = {
    port: number,
    token: string,
    --- The client URL; a control connection appends `&control=1`.
    url: string,
    stop: () -> (),
}
export type Control = {
    --- Every control frame received so far, in order.
    received: { any },
    --- Wait for the next frame of `type` past `from`, returning its index and payload.
    waitFor: (type: string, from: number, timeout: number?) -> (number?, any),
    close: () -> (),
}

local broker = {}

--- Locate a built broker, if there is one.
function broker.binary(): string?
    for _, candidate in BINARIES do
        if fs.isFile(candidate) then
            return candidate
        end
    end
    return nil
end

--- Start a broker on an OS-assigned port with a fresh token, waiting for its handshake.
function broker.start(binary: string): Running
    local child = process.create(binary, ARGUMENTS)

    local buffered = ""
    local handshake: { port: number, token: string }?
    for _ = 1, 100 do
        buffered ..= child.stdout:read() or ""
        local line = string.match(buffered, "^([^\n]*)\n")
        if line then
            handshake = serde.decode("json", line) :: any
            break
        end
    end

    local resolved = handshake
    if not resolved then
        child:kill()
        error(`broker printed no handshake: {buffered}`)
    end

    return {
        port = resolved.port,
        token = resolved.token,
        url = `ws://127.0.0.1:{resolved.port}/?token={net.urlEncode(resolved.token)}`,
        stop = function()
            child:kill()
        end,
    }
end

--- The control-only tail of `ServerMessage`. The client's codec stops at
--- `Operation` on purpose, so this end of the socket — the extension's end —
--- has to mirror the rest, exactly as `extension/src/broker/message` does.
local function buildControlCodec(client: Sandbox)
    local Squash = client.load("roblox_packages/.pesde/data-oriented-house_squash@5.0.0/squash")
    local primitives = client.load("src/lib/Messages/primitives")
    local serdeArray = client.load("src/lib/serdeArray")
    local squashEnum = client.load("src/lib/squashEnum")

    local logLevel = Squash.literal("Print", "Info", "Warn", "Error")
    local logEntries = serdeArray({
        ser = function(cursor, entry)
            logLevel.ser(cursor, entry.level)
            primitives.str.ser(cursor, entry.content)
        end,

        des = function(cursor)
            local content = primitives.str.des(cursor)
            return { level = logLevel.des(cursor), content = content }
        end,
    })

    -- A struct variant, so `entries` lands before `id`.
    local sessionLog = {
        ser = function(cursor, value)
            logEntries.ser(cursor, value.entries)
            primitives.u32.ser(cursor, value.id)
        end,

        des = function(cursor)
            local id = primitives.u32.des(cursor)
            return { id = id, entries = logEntries.des(cursor) }
        end,
    }

    -- Padded up to the control variants, since the tag is the position.
    return squashEnum({
        { type = "Hello" },
        { type = "UpstreamChanged" },
        { type = "RequestChildren" },
        { type = "RequestNode" },
        { type = "RequestSnapshot" },
        { type = "Search" },
        { type = "RequestNodes" },
        { type = "Operation" },
        { type = "Sessions" },
        { type = "NewSession", content = Squash.T(primitives.u32) },
        { type = "RemoveSession", content = Squash.T(primitives.u32) },
        { type = "SessionLog", content = Squash.T(sessionLog) },
    })
end

--- Open a control connection, standing in for the extension, decoding as it reads.
function broker.control(client: Sandbox, running: Running): Control
    local codec = buildControlCodec(client)
    local Squash = client.load("roblox_packages/.pesde/data-oriented-house_squash@5.0.0/squash")
    local socket = net.socket(`{running.url}&control=1`)

    local received: { any } = {}
    task.spawn(function()
        while true do
            -- A reset while the test tears the broker down is not a failure.
            local ok, frame = pcall(function()
                return socket:next()
            end)
            if not ok or frame == nil then
                break
            end

            local bytes = buffer.fromstring(client.env.crypt.base64decode(frame))
            table.insert(received, codec.des(Squash.frombuffer(bytes)))
        end
    end)

    return {
        received = received,

        waitFor = function(type: string, from: number, timeout: number?): (number?, any)
            local deadline = (timeout or 5) / POLL_INTERVAL
            for _ = 1, deadline do
                for index = from + 1, #received do
                    if received[index].type == type then
                        return index, received[index].content
                    end
                end
                task.wait(POLL_INTERVAL)
            end
            return nil, nil
        end,

        close = function()
            socket:close()
        end,
    }
end

return broker
