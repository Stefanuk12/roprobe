-- Dependencies
local Broker = require("@contracts/Broker")
local Executor = require("@engine/executor")
local Logger = require("@engine/Logger")
local RemoteHook = require("@engine/hooks/RemoteHook")
local Signal = require("@pkgs/signal")
local Trove = require("@pkgs/trove")
local actorSource = require("@generated/actorSource")
local uniqueId = require("@engine/uniqueId")

-- Services
local HttpService = cloneref(game:GetService("HttpService"))

-- Constants
local FLUSH_INTERVAL = 0.1
local MAX_PENDING = 512
--- The event name an actor VM relays a captured call under.
local RELAY_EVENT = "roprobe.call"
--- The event that tells every injected VM to unhook itself.
local UNLOAD_EVENT = "roprobe.unload"
--- An actor rejects injection while its VM is still coming up.
local INJECT_ATTEMPTS = 10
local INJECT_RETRY = 0.25

-- Types
type RemoteCall = Broker.RemoteCall
type SpyConfig = Broker.SpyConfig

--- Batches the remote calls the hooks capture, so a game firing hundreds a second
--- costs one frame per interval rather than one per call. Nothing is retained
--- past a flush; the broker holds the history.
---
--- Calls arrive from this VM's own hook and from the copy inside each actor, and
--- are numbered here so one order covers both.
local RemoteSpy = {}
RemoteSpy.__index = RemoteSpy

export type RemoteSpyData = {
    _trove: Trove.Trove,
    _hookTrove: Trove.Trove,
    _config: SpyConfig,
    _resolveId: (Instance) -> string,
    _hook: RemoteHook.RemoteHook?,
    _channel: Executor.CommChannel?,
    --- The actors already injected, weakly held so a destroyed one is forgotten.
    _injected: { [Instance]: boolean },
    _pending: { RemoteCall },
    _dropped: number,
    _nextCallId: number,
    _flushThread: thread?,

    onBatch: Signal.Signal<{ RemoteCall }>,
}
export type RemoteSpy = setmetatable<RemoteSpyData, typeof(RemoteSpy)>

--- The id an instance is mirrored under, falling back to its `UniqueId` so a
--- remote the DOM has never seen still gets a stable name.
local function resolver(idOf: (Instance) -> string?): (Instance) -> string
    return function(instance: Instance): string
        return idOf(instance) or uniqueId(instance) or ""
    end
end

--- Whether two configs ask for exactly the same capture.
local function isSameConfig(a: SpyConfig, b: SpyConfig): boolean
    return a.enabled == b.enabled
        and a.outgoing == b.outgoing
        and a.incoming == b.incoming
        and a.bindables == b.bindables
        and a.maxDepth == b.maxDepth
        and a.maxEntries == b.maxEntries
        and a.maxBytes == b.maxBytes
end

--- Create a new [`RemoteSpy`]; nothing is hooked until [`RemoteSpy.Configure`]
--- receives a config with `enabled` set.
function RemoteSpy.new(config: SpyConfig, idOf: (Instance) -> string?): RemoteSpy
    local trove = Trove.new()
    local self: RemoteSpyData = {
        _trove = trove,
        _hookTrove = trove:Extend(),
        _config = config,
        _resolveId = resolver(idOf),
        _hook = nil,
        _channel = nil,
        _injected = setmetatable({}, { __mode = "k" }) :: any,
        _pending = {},
        _dropped = 0,
        _nextCallId = 0,
        _flushThread = nil,

        onBatch = trove:Add(Signal.new()),
    }

    local spy = setmetatable(self, RemoteSpy)
    trove:Add(function()
        -- No trove reaches into an actor's VM, so its teardown goes over the
        -- channel before this one goes away.
        spy:_unloadActors()

        local thread = spy._flushThread
        if thread then
            task.cancel(thread)
            spy._flushThread = nil
        end
    end)
    return spy
end

--- Unhook everything and clean up.
function RemoteSpy.Destroy(self: RemoteSpy)
    self._trove:Destroy()
end

--- Adopt a new config, re-installing the hooks it changed. The hook reads its
--- config once at install, so a change tears the old one down rather than
--- mutating it underneath a live metamethod.
function RemoteSpy.Configure(self: RemoteSpy, config: SpyConfig)
    -- Most pushes restate what is already installed, and re-installing is not
    -- free: an executor without `restorefunction` cannot unhook, so the old
    -- metamethods stay and the new ones stack on top.
    local isInstalled = self._hook ~= nil
    if isInstalled == config.enabled and isSameConfig(self._config, config) then
        return
    end

    self:_unloadActors()
    self._config = config
    self._hookTrove:Clean()
    self._hook = nil
    self._channel = nil
    table.clear(self._injected)

    if not config.enabled then
        Logger:Info("spy", "remote spy disabled")
        return
    end

    -- The main VM owns the shared state, so an incoming call is reported once no
    -- matter how many actors are injected.
    local hook = self._hookTrove:Add(RemoteHook.new(config, self._resolveId, {
        actor = nil,
        ownsSharedState = true,
    }))
    self._hook = hook
    self._hookTrove:Add(
        hook.onCall:Connect(function(call: RemoteCall)
            self:_queue(call)
        end),
        "Disconnect"
    )

    -- Opened first so the hook can be told to skip it, rather than the spy
    -- briefly capturing its own relay traffic.
    local channelId = self:_openChannel(hook)
    hook:Initialise()

    -- Injected only after this VM has adopted the callbacks already in the tree,
    -- or the main hook would wrap an actor's wrapper and report every invoke twice.
    if channelId then
        local settings = HttpService:JSONEncode(config)
        self:_injectAll(channelId, settings)
        self:_watchForActors(channelId, settings)
    end

    local scope = if config.bindables then "remotes and bindables" else "remotes"
    Logger:Info("spy", `watching {scope}`)
end

--- Open the channel actor VMs report over, answering its id. A game whose
--- networking sits behind `Actor`s runs it in a VM none of our hooks reach, so
--- the same hook runs there and relays back.
function RemoteSpy._openChannel(self: RemoteSpy, hook: RemoteHook.RemoteHook): number?
    if not Executor.CanReachActors() then
        Logger:Debug("spy", "executor cannot reach actor VMs, watching this one only")
        return nil
    end

    local create = Executor.createCommChannel :: () -> (number, Executor.CommChannel)
    local ok, channelId, channel = pcall(create)
    if not ok or not channel then
        Logger:Warn("spy", `could not open an actor channel: {channelId}`)
        return nil
    end

    self._channel = channel
    -- The channel is a bindable of our own, so the spy would capture its own relays.
    hook:Ignore(channel :: any)

    self._hookTrove:Add(
        channel.Event:Connect(function(event: string, call: RemoteCall)
            if event == RELAY_EVENT and type(call) == "table" then
                self:_queue(call)
            end
        end),
        "Disconnect"
    )

    return channelId
end

--- Inject into every actor VM that exists now.
function RemoteSpy._injectAll(self: RemoteSpy, channelId: number, settings: string)
    -- Luau states reach every VM in one go, including the ones whose actor has
    -- already been destroyed.
    local states = Executor.getActorStates
    local runOnThread = Executor.runOnThread
    if states and runOnThread then
        local ok, found = pcall(states)
        if ok then
            local count = 0
            for _, state in found do
                if pcall(runOnThread, state, actorSource, channelId, settings) then
                    count += 1
                end
            end

            if count > 0 then
                Logger:Info("spy", `injected into {count} actor VM(s)`)
                return
            end
        end
    end

    local getActors = Executor.getActors
    if not getActors then
        return
    end

    local sources = { getActors }
    if Executor.getDeletedActors then
        table.insert(sources, Executor.getDeletedActors :: () -> { Actor })
    end

    for _, list in sources do
        local ok, actors = pcall(list)
        if ok then
            for _, actor in actors do
                self:_inject(actor, channelId, settings)
            end
        end
    end
end

--- Inject into one actor, retrying while its VM is still coming up.
function RemoteSpy._inject(self: RemoteSpy, actor: Instance, channelId: number, settings: string)
    local runOnActor = Executor.runOnActor
    if not runOnActor or self._injected[actor] then
        return
    end
    self._injected[actor] = true

    local target = cloneref(actor)
    self._hookTrove:Add(task.spawn(function()
        for _ = 1, INJECT_ATTEMPTS do
            if pcall(runOnActor, target :: Actor, actorSource, channelId, settings, target) then
                Logger:Debug("spy", `injected into {target:GetFullName()}`)
                return
            end
            task.wait(INJECT_RETRY)
        end

        Logger:Debug("spy", `could not inject into {target:GetFullName()}`)
    end))
end

--- Keep injecting as the game creates more actors.
function RemoteSpy._watchForActors(self: RemoteSpy, channelId: number, settings: string)
    -- The state signal beats watching the tree: an actor created and used within
    -- one frame would fire `DescendantAdded` only after its first call.
    local onCreated = Executor.onActorStateCreated
    if onCreated then
        self._hookTrove:Add(
            onCreated:Connect(function(instance: Instance)
                self:_inject(instance, channelId, settings)
            end),
            "Disconnect"
        )
        return
    end

    -- The tree fallback can only inject through `run_on_actor`; without it, the
    -- VMs reached by Luau state up front are all there will ever be.
    if not Executor.runOnActor then
        Logger:Debug("spy", "executor cannot inject into actors made after arming, watching the current VMs only")
        return
    end

    self._hookTrove:Connect(game.DescendantAdded, function(descendant: Instance)
        local ok, isActor = pcall(function()
            return descendant:IsA("Actor")
        end)
        if ok and isActor then
            self:_inject(descendant, channelId, settings)
        end
    end)
end

--- Tell every injected VM to unhook itself, since our trove cannot reach them.
function RemoteSpy._unloadActors(self: RemoteSpy)
    local channel = self._channel
    if not channel then
        return
    end

    pcall(function()
        channel:Fire(UNLOAD_EVENT)
    end)
end

--- Queue one captured call, numbering it and scheduling the next flush.
function RemoteSpy._queue(self: RemoteSpy, call: RemoteCall)
    local pending = self._pending
    if #pending >= MAX_PENDING then
        self._dropped += 1
        return
    end

    -- Numbered here, not by the hook that caught it: each actor's hook counts
    -- from zero in its own VM.
    call.id = self._nextCallId
    self._nextCallId = (self._nextCallId + 1) % 0x100000000
    table.insert(pending, call)

    if not self._flushThread then
        self._flushThread = task.delay(FLUSH_INTERVAL, function()
            self._flushThread = nil
            self:_flush()
        end)
    end
end

--- Hand the queued calls to the listeners, emptying the queue.
function RemoteSpy._flush(self: RemoteSpy)
    local pending = self._pending
    if #pending == 0 then
        return
    end
    self._pending = {}

    local count = #pending
    local dropped = self._dropped
    self._dropped = 0
    Logger:Debug("spy", `flushing {count} call(s), {dropped} dropped`)

    self.onBatch:Fire(pending)
end

return RemoteSpy
