-- Dependencies
local Broker = require("@contracts/Broker")
local Signal = require("@pkgs/signal")
local Trove = require("@pkgs/trove")
local captureValues = require("@engine/captureValues")
local sourceInfo = require("@lib/sourceInfo")

-- Constants
--- The methods a call can leave the client through, and the classes each is legal on.
local OUTGOING_METHODS: { [string]: { [string]: boolean } } = {
    FireServer = { RemoteEvent = true, UnreliableRemoteEvent = true },
    fireServer = { RemoteEvent = true, UnreliableRemoteEvent = true },
    InvokeServer = { RemoteFunction = true },
    invokeServer = { RemoteFunction = true },
    Fire = { BindableEvent = true },
    fire = { BindableEvent = true },
    Invoke = { BindableFunction = true },
    invoke = { BindableFunction = true },
}
--- The methods that answer their caller, so the reply is worth capturing too.
local INVOKE_METHODS: { [string]: boolean } = {
    InvokeServer = true,
    invokeServer = true,
    Invoke = true,
    invoke = true,
}
--- The signal each class delivers a server-driven call on.
local INCOMING_SIGNALS: { [string]: string } = {
    RemoteEvent = "OnClientEvent",
    UnreliableRemoteEvent = "OnClientEvent",
    BindableEvent = "Event",
}
--- The callback property each class delivers a server-driven invoke on.
local INCOMING_CALLBACKS: { [string]: string } = {
    RemoteFunction = "OnClientInvoke",
    BindableFunction = "OnInvoke",
}
--- The same properties as a set, for rejecting a write on its name alone.
local CALLBACK_PROPERTIES: { [string]: boolean } = {}
for _, property in INCOMING_CALLBACKS do
    CALLBACK_PROPERTIES[property] = true
end
local NETWORKED_CLASSES: { [string]: boolean } = {
    RemoteEvent = true,
    UnreliableRemoteEvent = true,
    RemoteFunction = true,
}
local BINDABLE_CLASSES: { [string]: boolean } = {
    BindableEvent = true,
    BindableFunction = true,
}
--- The classes we read the shared method closures off, in hooking order.
local OUTGOING_CLASSES = { "RemoteEvent", "UnreliableRemoteEvent", "RemoteFunction" }
local OUTGOING_BINDABLE_CLASSES = { "BindableEvent", "BindableFunction" }

-- Types
type CallSource = Broker.CallSource
type InstanceRef = Broker.InstanceRef
type LuaValue = Broker.LuaValue
type RemoteCall = Broker.RemoteCall
type RemoteDirection = Broker.RemoteDirection
type SpyConfig = Broker.SpyConfig

local NO_SOURCE: CallSource = {
    script = nil,
    functionName = nil,
    chunk = nil,
    line = nil,
    isExecutor = false,
    actor = nil,
}

--- Every callback wrapper installed in this VM, shared by all hooks in it: an
--- executor without `restorefunction` leaves the previous arm's wrappers assigned,
--- and the next hook has to recognise them rather than wrap them a second time.
local WRAPPERS: { [any]: boolean } = setmetatable({}, { __mode = "k" }) :: any

--- Whether this executor's `newcclosure` can be yielded through, decided once.
local canDisguise: boolean? = nil

--- Test the wrapper before trusting the game's calls to it: an `InvokeServer`
--- yields inside our hook, and a `newcclosure` that cannot be yielded through
--- would break the call. The probe yields too, so it runs at install time only.
local function probeDisguise(): boolean
    if canDisguise ~= nil then
        return canDisguise
    end
    if not newcclosure then
        canDisguise = false
        return false
    end

    local built, wrapped = pcall(newcclosure, function()
        task.wait()
        return true
    end)
    if not built then
        canDisguise = false
        return false
    end

    local survived, answer = pcall(wrapped)
    local yields = survived and answer == true
    canDisguise = yields
    return yields
end

--- Present our hooks to the game as C functions, so an `iscclosure` or
--- `debug.info` check on a metamethod answers the way the original did.
local function disguise<A..., R...>(hook: (A...) -> R...): (A...) -> R...
    if not canDisguise then
        return hook
    end

    local ok, disguised = pcall(newcclosure, hook)
    if ok then
        return disguised
    end
    return hook
end

--- Where a function was defined, for a call site we hold the closure of.
local function describeClosure(handler: (...any) -> ...any): CallSource
    local info = sourceInfo(handler)
    local okExecutor, isExecutor = pcall(isexecutorclosure, handler)
    return {
        script = nil,
        functionName = info.name,
        chunk = info.chunk,
        line = info.line,
        isExecutor = okExecutor and isExecutor == true,
        actor = nil,
    }
end

--- The chunk this file compiled into, read off a closure defined in it. The
--- client is one bundled file, so every frame reporting it is ours and skippable
--- — a fixed stack level cannot be, since an executor may add a trampoline.
local OWN_CHUNK = sourceInfo(function() end).chunk

--- Describe the call site that reached our hook.
local function describeStack(): CallSource
    local script: string? = nil
    local okScript, calling = pcall(getcallingscript)
    if okScript and calling then
        local okPath, path = pcall(function()
            return (calling :: Instance):GetFullName()
        end)
        script = if okPath then path :: string else (calling :: Instance).Name
    end

    local functionName: string? = nil
    local chunk: string? = nil
    local line: number? = nil

    -- Walked inline: `debug.info` counts levels from whichever function calls it,
    -- so a helper in between would renumber every level here.
    pcall(function()
        -- 1 is this closure and 2 is `describeStack`, both of them ours anyway.
        for level = 3, 32 do
            local fn: any, source: string?, name: string?, current: number? = debug.info(level, "fsnl")
            if fn == nil then
                break
            end

            if source ~= "[C]" and source ~= OWN_CHUNK then
                chunk = source
                functionName = if name ~= nil and name ~= "" then name else nil
                line = current
                break
            end
        end
    end)

    local okCaller, isExecutor = pcall(checkcaller)
    return {
        script = script,
        functionName = functionName,
        chunk = chunk,
        line = line,
        isExecutor = okCaller and isExecutor == true,
        actor = nil,
    }
end

--- Which Luau VM a hook is installed in, and what that makes it responsible for.
export type VmScope = {
    --- The `Actor` this VM belongs to, stamped onto every call it reports.
    actor: string?,
    --- Whether this VM watches the signals and callbacks every VM shares, which
    --- would otherwise report each incoming call once per VM. Outgoing calls and
    --- callback *assignments* are VM-local and are watched everywhere.
    ownsSharedState: boolean,
}

--- Watches every remote call the client makes or receives, reporting each one as
--- a [`RemoteCall`]; nothing is kept.
local RemoteHook = {}
RemoteHook.__index = RemoteHook

export type RemoteHookData = {
    _trove: Trove.Trove,
    _config: SpyConfig,
    _resolveId: (Instance) -> string,
    _vm: VmScope,
    _isInstalled: boolean,
    --- Re-checked by [`RemoteHook.Initialise`], which yields before it installs.
    _isTornDown: boolean,
    --- The threads inside our own outgoing hook, so the namecall hook and the
    --- closure hook cannot both log the one call that went through both.
    _dispatching: { [thread]: boolean },
    --- The callbacks we wrapped, so our own assignment is not wrapped again.
    _wrappers: { [any]: boolean },
    --- Instances the spy owns, which must never be reported: the comm channel is
    --- itself a bindable, and logging it would feed the spy its own traffic.
    _ignored: { [Instance]: boolean },
    --- Each remote's description, otherwise three property reads and a
    --- `GetFullName` walk per call.
    _describedBy: { [Instance]: InstanceRef },
    --- Each signal's attributed handler, refreshed only when its connections move.
    _handlersBy: { [RBXScriptSignal]: { count: number, handler: any, source: CallSource } },
    --- The instances already adopted, so a re-parented remote does not gain a
    --- second listener.
    _watched: { [Instance]: boolean },
    --- Each adopted remote's signal connection, so teardown can take them back.
    --- Weak, so a remote the game drops takes its entry with it.
    _connectionBy: { [Instance]: RBXScriptConnection },
    _hookedNamecall: boolean,
    _hookedNewIndex: boolean,
    _hookedMethods: { any },

    onCall: Signal.Signal<RemoteCall>,
}
export type RemoteHook = setmetatable<RemoteHookData, typeof(RemoteHook)>

--- Create a new [`RemoteHook`]; nothing is hooked until [`RemoteHook.Initialise`].
function RemoteHook.new(config: SpyConfig, resolveId: (Instance) -> string, vm: VmScope): RemoteHook
    local trove = Trove.new()
    -- Weak keys throughout: holding a dropped remote alive is both a leak and a
    -- difference the game can see.
    local weak = { __mode = "k" }
    local self: RemoteHookData = {
        _trove = trove,
        _config = config,
        _resolveId = resolveId,
        _vm = vm,
        _isInstalled = false,
        _isTornDown = false,
        _dispatching = setmetatable({}, weak) :: any,
        _wrappers = WRAPPERS,
        _ignored = setmetatable({}, weak) :: any,
        _describedBy = setmetatable({}, weak) :: any,
        _handlersBy = setmetatable({}, weak) :: any,
        _watched = setmetatable({}, weak) :: any,
        _connectionBy = setmetatable({}, weak) :: any,
        _hookedNamecall = false,
        _hookedNewIndex = false,
        _hookedMethods = {},

        onCall = trove:Add(Signal.new()),
    }

    local hook = setmetatable(self, RemoteHook)
    trove:Add(function()
        hook._isTornDown = true
        hook:Revert()
    end)
    return hook
end

--- Revert every hook and clean up.
function RemoteHook.Destroy(self: RemoteHook)
    self._trove:Destroy()
end

--- Never report calls on this instance, for the spy's own comm channel.
function RemoteHook.Ignore(self: RemoteHook, instance: Instance)
    self._ignored[instance] = true
end

--- Whether this class is one the current config asks us to watch.
function RemoteHook._watches(self: RemoteHook, class: string): boolean
    if NETWORKED_CLASSES[class] then
        return true
    end
    return self._config.bindables and BINDABLE_CLASSES[class] == true
end

--- Describe a remote, reusing the description from the last call on it. Keyed on
--- the reference handed in: `cloneref` answers a fresh userdata every time, so a
--- clone would never hit the cache.
function RemoteHook._describe(self: RemoteHook, remote: Instance): InstanceRef
    local cached = self._describedBy[remote]
    if cached then
        return cached
    end

    -- The reads go through a clone, so we never touch the reference the game holds.
    local target = cloneref(remote)
    local okClass, class = pcall(function()
        return target.ClassName
    end)
    local okName, name = pcall(function()
        return target.Name
    end)
    local okPath, path = pcall(function()
        return target:GetFullName()
    end)
    local resolvedName = if okName then name else "?"
    -- Resolved off the original: the mirror keys its ids on the instance the DOM
    -- walked, which a clone never matches.
    local okId, resolvedId = pcall(self._resolveId, remote)

    local described: InstanceRef = {
        id = if okId then resolvedId else "",
        class = if okClass then class else "?",
        name = resolvedName,
        path = if okPath then path else resolvedName,
    }
    self._describedBy[remote] = described
    return described
end

--- Emit one finished call to the listeners. `id` is left at zero: only the spy,
--- which owns this hook and each actor's, can number them meaningfully.
function RemoteHook._report(
    self: RemoteHook,
    direction: RemoteDirection,
    remote: Instance,
    method: string,
    arguments: { LuaValue },
    returns: { LuaValue }?,
    source: CallSource
)
    -- Copied: `NO_SOURCE` and the handler cache are shared, so stamping the actor
    -- in place would name it on every call after.
    local reported = table.clone(source)
    reported.actor = self._vm.actor

    self.onCall:Fire({
        id = 0,
        direction = direction,
        remote = self:_describe(remote),
        method = method,
        arguments = arguments,
        returns = returns,
        source = reported,
        timestamp = os.clock(),
    })
end

--- Capture the first `count` values of `packed` under the current caps.
function RemoteHook._capture(self: RemoteHook, packed: { any }, count: number): { LuaValue }
    local config = self._config
    return captureValues(packed, count, {
        maxDepth = config.maxDepth,
        maxEntries = config.maxEntries,
        maxBytes = config.maxBytes,
        resolveId = self._resolveId,
    })
end

--- Handle one outgoing call: capture it, report it, then let it through untouched.
--- An invoke reports twice, the request as it leaves and the answer as an
--- `Incoming`, so a call the server never answers is still visible.
function RemoteHook._outgoing(
    self: RemoteHook,
    remote: Instance,
    method: string,
    original: (...any) -> ...any,
    ...: any
): ...any
    local thread = coroutine.running()
    if not self._isInstalled or self._dispatching[thread] then
        return original(...)
    end

    -- The receiver rides in as the first argument; the payload starts after it.
    local packed = table.pack(select(2, ...))
    local arguments = self:_capture(packed, packed.n)
    self:_report("Outgoing", remote, method, arguments, nil, describeStack())

    if not INVOKE_METHODS[method] then
        self._dispatching[thread] = true
        local ok, err = pcall(original, ...)
        self._dispatching[thread] = nil

        if not ok then
            error(err, 0)
        end
        return
    end

    self._dispatching[thread] = true
    local answered = table.pack(pcall(original, ...))
    self._dispatching[thread] = nil

    if not answered[1] then
        error(answered[2], 0)
    end

    local count = answered.n - 1
    local results: { any } = table.move(answered, 2, answered.n, 1, table.create(count))
    self:_report("Incoming", remote, method, arguments, self:_capture(results, count), NO_SOURCE)
    return table.unpack(results, 1, count)
end

--- Whether this namecall is one worth capturing.
function RemoteHook._isWatchedCall(self: RemoteHook, receiver: any, method: string): boolean
    if typeof(receiver) ~= "Instance" or self._ignored[receiver :: Instance] then
        return false
    end

    local legal = OUTGOING_METHODS[method]
    if not legal then
        return false
    end

    local ok, class = pcall(function()
        return (receiver :: Instance).ClassName
    end)
    return ok and legal[class] == true and self:_watches(class)
end

--- Intercept `remote:FireServer(...)` and friends at the metamethod.
function RemoteHook._hookNamecall(self: RemoteHook)
    if not (hookmetamethod and getnamecallmethod) then
        return
    end

    local original: (...any) -> ...any
    local ok = pcall(function()
        original = hookmetamethod(
            game,
            "__namecall",
            disguise(function(...)
                local receiver = ...
                local okMethod, method = pcall(getnamecallmethod)
                if not okMethod or not self:_isWatchedCall(receiver, method) then
                    return original(...)
                end

                -- Forwarded through the method value, not `original`: describing
                -- a call runs namecalls of our own, and each overwrites the
                -- pending atom `original` would dispatch on. A plain call needs
                -- no atom, and the fallbacks run before anything is described.
                local okValue, forward = pcall(function()
                    return (receiver :: any)[method]
                end)
                if not okValue or type(forward) ~= "function" then
                    return original(...)
                end

                return self:_outgoing(receiver :: Instance, method, forward, ...)
            end)
        )
    end)
    self._hookedNamecall = ok
end

--- Intercept the same calls made through the method value itself
--- (`local fire = remote.FireServer; fire(remote, ...)`), which never reaches
--- `__namecall`. One closure is shared per class, so any instance will do.
function RemoteHook._hookMethods(self: RemoteHook)
    if not hookfunction then
        return
    end

    -- Sampled only once there is something to hook: the search walks the whole
    -- DataModel and mints any class the place lacks, so it is not run for nothing.
    for class, sample in self:_samples() do
        for method, legal in OUTGOING_METHODS do
            if not legal[class] then
                continue
            end

            local okMethod, target = pcall(function()
                return (sample :: any)[method]
            end)
            if not okMethod or type(target) ~= "function" then
                continue
            end

            local original: (...any) -> ...any
            local ok = pcall(function()
                original = hookfunction(
                    target,
                    disguise(function(...)
                        local receiver = ...
                        if not self:_isWatchedCall(receiver, method) then
                            return original(...)
                        end

                        return self:_outgoing(receiver :: Instance, method, original, ...)
                    end)
                )
            end)
            if ok then
                table.insert(self._hookedMethods, target)
            end
        end
    end
end

--- The first connection on a signal that is not the listener we connected, with
--- how many there were. Reads `ForeignState` first: an executor is free to raise
--- on a foreign connection's `Function` rather than answer nil.
local function findHandler(connections: { any }, ours: any): (number, any)
    for _, connection in connections do
        if connection.ForeignState then
            continue
        end

        local handler = connection.Function
        if handler ~= nil and handler ~= ours then
            return #connections, handler
        end
    end

    return #connections, nil
end

--- The game's own handler on a signal, ignoring the listener we connected, so a
--- remote is attributed to the script listening on it. The walk stops at the first
--- handler; only describing it, which is the measurable part, is cached.
function RemoteHook._handlersOf(self: RemoteHook, signal: RBXScriptSignal, ours: any): CallSource
    if not getconnections then
        return NO_SOURCE
    end

    local ok, connections = pcall(getconnections, signal)
    if not ok then
        return NO_SOURCE
    end

    local walked, count, handler = pcall(findHandler, connections, ours)
    if not walked then
        return NO_SOURCE
    end

    -- Keyed on the handler as well as the count: a script that swaps one handler
    -- for another leaves the count exactly where it was.
    local cached = self._handlersBy[signal]
    if cached and cached.count == count and cached.handler == handler then
        return cached.source
    end

    local source = NO_SOURCE
    if handler ~= nil then
        source = describeClosure(handler)
    end

    self._handlersBy[signal] = { count = count :: number, handler = handler, source = source }
    return source
end

--- Listen on a remote's own signal, so a server-driven fire is seen even with no
--- handler connected.
function RemoteHook._watchSignal(self: RemoteHook, remote: Instance, class: string)
    local signalName = INCOMING_SIGNALS[class]
    if not signalName then
        return
    end

    local ok, signal = pcall(function()
        return (remote :: any)[signalName] :: RBXScriptSignal
    end)
    if not ok or typeof(signal) ~= "RBXScriptSignal" then
        return
    end

    local listener
    listener = function(...)
        if not self._isInstalled then
            return
        end

        local packed = table.pack(...)
        self:_report(
            "Incoming",
            remote,
            signalName,
            self:_capture(packed, packed.n),
            nil,
            self:_handlersOf(signal, listener)
        )
    end

    local okConnect, connection = pcall(function()
        return signal:Connect(listener)
    end)
    if okConnect then
        self._trove:Add(connection)
    end
end

--- Wrap a callback the game assigned so the invoke and its answer are both seen,
--- returning the wrapper to assign in its place.
function RemoteHook._wrapCallback(self: RemoteHook, remote: Instance, property: string, callback: any): any
    if type(callback) ~= "function" or self._wrappers[callback] then
        return callback
    end

    local handler = callback :: (...any) -> ...any
    local source = describeClosure(handler)
    local wrapper = disguise(function(...)
        if not self._isInstalled then
            return handler(...)
        end

        local packed = table.pack(...)
        local arguments = self:_capture(packed, packed.n)

        local answered = table.pack(pcall(handler, ...))
        if not answered[1] then
            self:_report("Incoming", remote, property, arguments, nil, source)
            error(answered[2], 0)
        end

        local count = answered.n - 1
        local results: { any } = table.move(answered, 2, answered.n, 1, table.create(count))
        self:_report("Incoming", remote, property, arguments, self:_capture(results, count), source)
        return table.unpack(results, 1, count)
    end)

    self._wrappers[wrapper] = true
    return wrapper
end

--- Intercept `remote.OnClientInvoke = handler`, which is a property write rather
--- than a signal, so the assignment itself is the only place to see it.
function RemoteHook._hookCallbacks(self: RemoteHook)
    if not hookmetamethod then
        return
    end

    local original: (...any) -> ...any
    local ok = pcall(function()
        original = hookmetamethod(
            game,
            "__newindex",
            disguise(function(...)
                local receiver, property, value = ...
                -- The name first: this metamethod sees every property write in
                -- the game, and all but these two are rejected without reading
                -- the receiver's class.
                if type(property) ~= "string" or not CALLBACK_PROPERTIES[property] then
                    return original(...)
                end
                if typeof(receiver) ~= "Instance" or self._ignored[receiver :: Instance] then
                    return original(...)
                end

                local okClass, class = pcall(function()
                    return (receiver :: Instance).ClassName
                end)
                if not okClass or INCOMING_CALLBACKS[class] ~= property or not self:_watches(class) then
                    return original(...)
                end

                local remote = cloneref(receiver :: Instance)
                return original(receiver, property, self:_wrapCallback(remote, property, value))
            end)
        )
    end)
    self._hookedNewIndex = ok
end

--- Wrap the callback a remote already had, for the ones assigned before we hooked.
function RemoteHook._adoptCallback(self: RemoteHook, remote: Instance, class: string)
    local property = INCOMING_CALLBACKS[class]
    if not property or not getcallbackvalue then
        return
    end

    local ok, callback = pcall(getcallbackvalue, remote, property)
    if not ok or type(callback) ~= "function" or self._wrappers[callback] then
        return
    end

    local wrapper = self:_wrapCallback(remote, property, callback)
    pcall(function()
        (remote :: any)[property] = wrapper
    end)
end

--- Start watching one instance, if it is a remote the config covers.
function RemoteHook._adopt(self: RemoteHook, instance: Instance)
    if self._ignored[instance] then
        return
    end

    local ok, class = pcall(function()
        return instance.ClassName
    end)
    if not ok or not self:_watches(class) then
        return
    end

    -- `DescendantAdded` fires again for a re-parented remote, and a second
    -- listener would report every call twice.
    if self._watched[instance] then
        return
    end
    self._watched[instance] = true

    local remote = cloneref(instance)
    self:_watchSignal(remote, class)
    self:_adoptCallback(remote, class)
end

--- One instance of each class we need the method closures of. Found rather than
--- minted wherever the game has one: `Instance.new` is a call the game can hook.
--- `FindFirstChildWhichIsA` recurses engine-side, so the search is cheap.
function RemoteHook._samples(self: RemoteHook): { [string]: Instance }
    local wanted = table.clone(OUTGOING_CLASSES)
    if self._config.bindables then
        table.move(OUTGOING_BINDABLE_CLASSES, 1, #OUTGOING_BINDABLE_CLASSES, #wanted + 1, wanted)
    end

    local sampleByClass: { [string]: Instance } = {}
    for _, class in wanted do
        local okFound, found = pcall(function()
            return game:FindFirstChildWhichIsA(class :: any, true)
        end)
        if okFound and found then
            sampleByClass[class] = cloneref(found)
            continue
        end

        -- Nothing to find, so the class has to be minted to be hooked at all. It
        -- is never parented, so nothing in the tree observes it.
        local okNew, sample = pcall(Instance.new, class :: any)
        if okNew then
            sampleByClass[class] = sample
            self._trove:Add(sample)
        end
    end

    return sampleByClass
end

--- Adopt every remote already in the tree. Only the VM that owns the shared
--- state does this; the rest would each re-listen and re-wrap the same signals.
function RemoteHook._adoptExisting(self: RemoteHook)
    local ok, descendants = pcall(function()
        return game:GetDescendants()
    end)
    if not ok then
        return
    end

    for _, descendant in descendants :: { Instance } do
        self:_adopt(descendant)
    end
end

--- Install every hook the config asks for and the executor supports.
function RemoteHook.Initialise(self: RemoteHook)
    if self._isInstalled or self._isTornDown or not self._config.enabled then
        return
    end

    -- Settled before the first hook goes in, so every one is wrapped the same way.
    probeDisguise()

    -- The probe yields, so a teardown can have landed while it ran. Installing
    -- past one would leave metamethods `Revert` has already declined to take back.
    if self._isTornDown then
        return
    end
    self._isInstalled = true

    local config = self._config
    if config.outgoing then
        self:_hookNamecall()
        self:_hookMethods()
    end

    if not config.incoming then
        return
    end

    -- Assignments are VM-local, so every VM watches for its own.
    self:_hookCallbacks()

    if not self._vm.ownsSharedState then
        return
    end

    self:_adoptExisting()
    self._trove:Connect(game.DescendantAdded, function(descendant: Instance)
        self:_adopt(descendant)
    end)
end

--- Put every hooked function back, leaving the game as we found it. An executor
--- without `restorefunction` cannot unhook, so the wrappers stay and go inert
--- instead — every one of them checks `_isInstalled` first.
function RemoteHook.Revert(self: RemoteHook)
    if not self._isInstalled then
        return
    end
    self._isInstalled = false

    if not restorefunction then
        table.clear(self._hookedMethods)
        return
    end

    for _, target in self._hookedMethods do
        pcall(restorefunction, target)
    end
    table.clear(self._hookedMethods)

    local metatable = if getrawmetatable then getrawmetatable(game) else nil
    if metatable then
        if self._hookedNamecall then
            pcall(restorefunction, metatable.__namecall)
        end
        if self._hookedNewIndex then
            pcall(restorefunction, metatable.__newindex)
        end
    end
    self._hookedNamecall = false
    self._hookedNewIndex = false
end

return RemoteHook
