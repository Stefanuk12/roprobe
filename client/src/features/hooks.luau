-- Dependencies
local Broker = require("@contracts/Broker")
local Logger = require("@engine/Logger")
local Resolve = require("@engine/Resolve")
local captureValues = require("@engine/captureValues")

-- Constants
local MODE_BLOCK = 1
local CAPTURE = { maxDepth = 6, maxEntries = 128, maxBytes = 4096 }
--- The method each remote class sends through.
local METHODS: { [string]: string } = {
    RemoteEvent = "FireServer",
    UnreliableRemoteEvent = "FireServer",
    RemoteFunction = "InvokeServer",
}

-- Types
type CallSource = Broker.CallSource
type ClientMessage = Broker.ClientMessage
type InstanceRef = Broker.InstanceRef
type LuaValue = Broker.LuaValue
type Closure = (...any) -> ...any

--- One hooked method closure, shared by every remote of its class.
type Armed = {
    target: Closure,
    original: Closure,
    --- The remotes it reports, each with its mode.
    modes: { [Instance]: number },
}

--- Hooks remotes and traces functions, streaming each call to the broker.
local Hooks = {}
Hooks.__index = Hooks

export type HooksData = {
    _emit: (ClientMessage) -> (),
    _resolveId: (Instance) -> string,
    _isDead: boolean,
    _armed: { [Closure]: Armed },
    _remotes: { [string]: { remote: Instance, armed: Armed } },
    _traces: { [string]: { target: Closure, original: Closure } },
    _described: { [Instance]: InstanceRef },
}
export type Hooks = setmetatable<HooksData, typeof(Hooks)>

--- Make a hook look native where the executor lets it.
local function disguise(hook: Closure): Closure
    if not newcclosure then
        return hook
    end

    local ok, wrapped = pcall(newcclosure, hook)
    return if ok then wrapped else hook
end

--- Put a hooked function back, however the executor likes it.
local function restore(target: Closure, original: Closure)
    if restorefunction and pcall(restorefunction, target) then
        return
    end
    pcall(hookfunction, target, original)
end

--- Create a new [`Hooks`]; `emit` ships a message to the broker.
function Hooks.new(emit: (ClientMessage) -> (), resolveId: (Instance) -> string): Hooks
    local self: HooksData = {
        _emit = emit,
        _resolveId = resolveId,
        _isDead = false,
        _armed = {},
        _remotes = {},
        _traces = {},
        _described = setmetatable({}, { __mode = "k" }) :: any,
    }
    return setmetatable(self, Hooks)
end

--- Unhook everything; wrappers the executor cannot remove go inert.
function Hooks.Destroy(self: Hooks)
    self._isDead = true
    for _, armed in self._armed do
        restore(armed.target, armed.original)
    end
    for _, trace in self._traces do
        restore(trace.target, trace.original)
    end
    table.clear(self._armed)
    table.clear(self._remotes)
    table.clear(self._traces)
end

--- Capture `count` values of `packed` under the hook caps.
function Hooks._capture(self: Hooks, packed: { any }, count: number): { LuaValue }
    return captureValues(packed, count, {
        maxDepth = CAPTURE.maxDepth,
        maxEntries = CAPTURE.maxEntries,
        maxBytes = CAPTURE.maxBytes,
        resolveId = self._resolveId,
    })
end

--- Name a remote once, then reuse it.
function Hooks._describe(self: Hooks, remote: Instance): InstanceRef
    local cached = self._described[remote]
    if cached then
        return cached
    end

    local ref = self:_capture({ remote }, 1)[1].content :: InstanceRef
    self._described[remote] = ref
    return ref
end

--- Who made this call.
local function callerOf(): CallSource
    local script: string? = nil
    local ok, calling = pcall(getcallingscript)
    if ok and calling then
        local named, path = pcall(function()
            return (calling :: Instance):GetFullName()
        end)
        script = if named then path else nil
    end

    local checked, isExecutor = pcall(checkcaller)
    return {
        script = script,
        functionName = nil,
        chunk = nil,
        line = nil,
        isExecutor = checked and isExecutor == true,
        actor = nil,
    }
end

--- Report one remote call.
function Hooks._report(self: Hooks, remote: Instance, args: { LuaValue }, returns: { LuaValue }?, caller: CallSource)
    self._emit({
        type = "HookedCall" :: "HookedCall",
        content = {
            remote = self:_describe(remote),
            args = args,
            returns = returns,
            caller = caller,
            timestamp = os.clock(),
        },
    })
end

--- Hook `FireServer`/`InvokeServer` for this class, once.
function Hooks._arm(self: Hooks, target: Closure, isInvoke: boolean): Armed?
    local existing = self._armed[target]
    if existing then
        return existing
    end

    local armed = { target = target, original = (nil :: any) :: Closure, modes = {} } :: Armed
    local function hook(receiver: any, ...: any): ...any
        local mode = armed.modes[receiver]
        if self._isDead or mode == nil then
            return armed.original(receiver, ...)
        end

        local packed = table.pack(...)
        local args = self:_capture(packed, packed.n)
        local caller = callerOf()

        if mode == MODE_BLOCK then
            self:_report(receiver, args, nil, caller)
            return
        end

        if not isInvoke then
            self:_report(receiver, args, nil, caller)
            return armed.original(receiver, ...)
        end

        local answered = table.pack(pcall(armed.original, receiver, ...))
        local count = answered.n - 1
        local results = table.move(answered, 2, answered.n, 1, table.create(count))
        self:_report(receiver, args, if answered[1] then self:_capture(results, count) else nil, caller)
        if not answered[1] then
            error(answered[2], 0)
        end
        return table.unpack(results, 1, count)
    end

    -- An invoke yields, which a cclosure wrapper may not survive.
    local ok, original = pcall(hookfunction, target, if isInvoke then hook else disguise(hook))
    if not ok then
        Logger:Warn("hooks", `hookfunction failed: {original}`)
        return nil
    end

    armed.original = original
    self._armed[target] = armed
    return armed
end

--- Log (`0`) or block (`1`) calls on the remote at `path`.
function Hooks.Hook(self: Hooks, path: string, mode: number)
    local remote = Resolve.Instance(path)
    if not remote then
        Logger:Warn("hooks", `remote not found: {path}`)
        return
    end

    local method = METHODS[remote.ClassName]
    if not method then
        Logger:Warn("hooks", `not a remote: {path}`)
        return
    end

    local ok, target = pcall(function()
        return (remote :: any)[method]
    end)
    if not ok or type(target) ~= "function" then
        Logger:Warn("hooks", `no {method} on {path}`)
        return
    end

    local armed = self:_arm(target, method == "InvokeServer")
    if not armed then
        return
    end

    armed.modes[remote] = mode
    self._remotes[path] = { remote = remote, armed = armed }
    Logger:Info("hooks", `hooked {path}`)
end

--- Stop reporting the remote at `path`; the method hook leaves with its last remote.
function Hooks.Unhook(self: Hooks, path: string)
    local entry = self._remotes[path]
    if not entry then
        return
    end

    self._remotes[path] = nil
    local armed = entry.armed
    armed.modes[entry.remote] = nil
    if next(armed.modes) == nil then
        restore(armed.target, armed.original)
        self._armed[armed.target] = nil
    end
end

--- Start or stop reporting every call to the function at `path`.
function Hooks.Trace(self: Hooks, path: string, enabled: boolean)
    local existing = self._traces[path]
    if existing then
        restore(existing.target, existing.original)
        self._traces[path] = nil
    end
    if not enabled then
        return
    end

    local target = Resolve.Closure(path)
    if not target then
        Logger:Warn("hooks", `trace target not found: {path}`)
        return
    end

    local original: Closure
    local function trace(...: any): ...any
        if self._isDead then
            return original(...)
        end

        local packed = table.pack(...)
        local args = self:_capture(packed, packed.n)
        local results = table.pack(original(...))
        self._emit({
            type = "TraceEvent" :: "TraceEvent",
            content = {
                target = path,
                args = args,
                returns = self:_capture(results, results.n),
                timestamp = os.clock(),
            },
        })
        return table.unpack(results, 1, results.n)
    end

    local wrapper = if iscclosure(target) then disguise(trace) else trace
    local ok, previous = pcall(hookfunction, target, wrapper)
    if not ok then
        Logger:Warn("hooks", `trace failed: {previous}`)
        return
    end

    original = previous
    self._traces[path] = { target = target, original = previous }
    Logger:Info("hooks", `tracing {path}`)
end

return Hooks
