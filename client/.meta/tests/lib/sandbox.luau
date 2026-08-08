-- Dependencies
local fs = require("@lune/fs")
local luau = require("@lune/luau")
local net = require("@lune/net")
local roblox = require("@lune/roblox")
local signal = require("./signal")
local task = require("@lune/task")

-- Constants
--- `.luaurc`'s aliases; `@pkgs` is special-cased against pesde's link files.
local ALIASES = {
    contracts = "src/contracts",
    lib = "src/lib",
    engine = "src/engine",
    features = "src/features",
    src = "src",
    generated = ".meta/generated",
}
local BASE64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
--- pesde's roblox links are instance requires lune can't follow, so read the path out of them.
local LINK_PATTERN = 'FindFirstChild%("%.pesde"%):FindFirstChild%("([^"]+)"%):FindFirstChild%("([^"]+)"%)'

-- Types
export type Sandbox = {
    --- The shared global table every sandboxed module resolves against, and what `getgenv()` returns.
    env: { [string]: any },
    --- Require a client module by repo-relative path, e.g. `src/lib/Messages`.
    load: (path: string) -> any,
    --- The fake `LogService`; fire `MessageOut` to stand in for the game's output.
    logService: { MessageOut: signal.Signal },
}

--- Encode the way the executor's `crypt` does; lune has no base64.
local function base64encode(input: string): string
    local out = {}
    for i = 1, #input, 3 do
        local a, b, c = string.byte(input, i, i + 2)
        local n = a * 65536 + (b or 0) * 256 + (c or 0)
        local one = bit32.rshift(n, 18) + 1
        local two = bit32.band(bit32.rshift(n, 12), 63) + 1
        local three = bit32.band(bit32.rshift(n, 6), 63) + 1
        local four = bit32.band(n, 63) + 1

        table.insert(out, string.sub(BASE64_ALPHABET, one, one))
        table.insert(out, string.sub(BASE64_ALPHABET, two, two))
        table.insert(out, if b then string.sub(BASE64_ALPHABET, three, three) else "=")
        table.insert(out, if c then string.sub(BASE64_ALPHABET, four, four) else "=")
    end

    return table.concat(out)
end

--- The inverse of [`base64encode`], tolerating whitespace the way the executor does.
local function base64decode(input: string): string
    local clean = (input:gsub("[^%w%+/=]", ""))
    local out = {}
    for i = 1, #clean, 4 do
        local n, pad = 0, 0
        for j = 0, 3 do
            local char = string.sub(clean, i + j, i + j)
            local index = string.find(BASE64_ALPHABET, char, 1, true)
            if char == "=" or not index then
                index = 1
                pad += 1
            end
            n = n * 64 + (index - 1)
        end

        local bytes = { bit32.rshift(n, 16), bit32.band(bit32.rshift(n, 8), 255), bit32.band(n, 255) }
        for k = 1, 3 - pad do
            table.insert(out, string.char(bytes[k]))
        end
    end

    return table.concat(out)
end

--- Stand in for `WebSocket.connect`, over lune's client socket.
local function connectWebSocket(url: string)
    local socket = net.socket(url)
    local connection = {
        OnMessage = signal.new(),
        OnClose = signal.new(),

        Send = function(_, message: string)
            socket:send(message)
        end,

        Close = function()
            socket:close()
        end,
    }

    task.spawn(function()
        while true do
            -- A reset while the test is tearing the broker down is not a failure.
            local ok, message = pcall(function()
                return socket:next()
            end)
            if not ok or message == nil then
                connection.OnClose:Fire()
                break
            end
            connection.OnMessage:Fire(message)
        end
    end)

    return connection
end

--- Roblox interns its `EnumItem`s; lune mints a fresh userdata per access, so
--- without this an item can never be a table key (`==` still holds, hashing
--- does not) and lookups like `LEVEL_BY_MESSAGE_TYPE[messageType]` silently miss.
local function buildEnum()
    local families: { [string]: any } = {}
    return setmetatable({}, {
        __index = function(_, familyName: string)
            local family = families[familyName]
            if family then
                return family
            end

            local real = (roblox :: any).Enum[familyName]
            -- `Enum:GetEnums()` and friends are methods, not families.
            if type(real) == "function" then
                family = function(_, ...)
                    return real((roblox :: any).Enum, ...)
                end
                families[familyName] = family
                return family
            end

            local items: { [string]: any } = {}
            family = setmetatable({}, {
                __index = function(_, key: string)
                    local item = items[key]
                    if item == nil then
                        item = real[key]
                        if type(item) == "function" then
                            local method = item
                            item = function(_, ...)
                                return method(real, ...)
                            end
                        end
                        items[key] = item
                    end
                    return item
                end,
            })
            families[familyName] = family
            return family
        end,
    })
end

--- Build a sandbox the client's source can run in: an executor-shaped global
--- table plus a module loader, since Luau has no `setfenv` and lune's `_G` is
--- not the global table, leaving `luau.load`'s `environment` the only way to
--- put `getgenv`/`cloneref`/`WebSocket` in front of a required module.
local function buildSandbox(): Sandbox
    local logService = { MessageOut = signal.new() }
    local runService = {
        BindToRenderStep = function() end,
        UnbindFromRenderStep = function() end,
    }

    local env: { [string]: any } = {}
    for name, value in pairs(roblox :: any) do
        env[name] = value
    end
    env.Enum = buildEnum()

    -- The hooked globals have to start out present, or `OutputHook` captures nil originals.
    env.print = print
    env.warn = warn
    env.error = error
    env.task = task
    env.getgenv = function()
        return env
    end
    env.game = {
        GetService = function(_, name: string)
            if name == "LogService" then
                return logService
            elseif name == "RunService" then
                return runService
            end
            error(`unstubbed service {name}`)
        end,
    }
    env.cloneref = function(value)
        return value
    end
    env.crypt = { base64encode = base64encode, base64decode = base64decode }
    env.isfile = function()
        return false
    end
    env.WebSocket = { connect = connectWebSocket }

    local cache: { [string]: any } = {}
    local loading: { string } = { "src/init.luau" }

    local function resolve(request: string): string
        if string.sub(request, 1, 1) ~= "@" then
            local dir = string.match(loading[#loading], "^(.*)/[^/]*$") :: string
            return `{dir}/{(string.gsub(request, "^%./", ""))}`
        end

        local alias, rest = string.match(request, "^@([^/]+)/?(.*)$")
        if alias == "pkgs" then
            local folder, module = string.match(fs.readFile(`roblox_packages/{rest}.luau`), LINK_PATTERN)
            return `roblox_packages/.pesde/{folder}/{module}`
        end

        local base = ALIASES[alias :: string]
        assert(base, `unknown alias {alias}`)
        return base .. (if rest ~= "" then `/{rest}` else "")
    end

    local function load(path: string): any
        local file = `{path}.luau`
        if not fs.isFile(file) then
            file = `{path}/init.luau`
        end
        assert(fs.isFile(file), `no module at {path}`)

        local cached = cache[file]
        if cached then
            return cached.value
        end

        table.insert(loading, file)
        local chunk = luau.load(fs.readFile(file), { environment = env, debugName = file, injectGlobals = true })
        local value = chunk()
        table.remove(loading)

        cache[file] = { value = value }
        return value
    end

    env.require = function(request: string)
        return load(resolve(request))
    end

    return { env = env, load = load, logService = logService }
end

return buildSandbox
