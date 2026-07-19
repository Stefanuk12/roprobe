-- Dependencies
local DomSync = require("@features/broker/DomSync")
local Messages = require("@lib/Messages")
local Signal = require("@pkgs/signal")
local Trove = require("@pkgs/trove")

--- Keeps a [`DomSync`] mirror live for one subtree, emitting a
--- [`Messages.DomPatch`] through `onPatch` as its structure changes.
local DomWatch = {}
DomWatch.__index = DomWatch

-- Types
export type DomWatchData = {
    _trove: Trove.Trove,
    _sync: DomSync.DomSync,
    _root: Instance,
    _isLazy: boolean,
    _started: boolean,

    onPatch: Signal.Signal<Messages.DomPatch>,
}
export type DomWatch = setmetatable<DomWatchData, typeof(DomWatch)>

--- Create a new [`DomWatch`] over `root` (defaults to the whole DataModel).
function DomWatch.new(sync: DomSync.DomSync, root: Instance?, isLazy: boolean?): DomWatch
    local trove = Trove.new()
    local self: DomWatchData = {
        _trove = trove,
        _sync = sync,
        _root = root or game,
        _isLazy = isLazy or false,
        _started = false,

        onPatch = trove:Add(Signal.new()),
    }
    return setmetatable(self, DomWatch)
end

--- Destroy this object, and clean up.
function DomWatch.Destroy(self: DomWatch)
    self._trove:Destroy()
end

--- Follow structural teardown: prune the mirror as instances leave the subtree.
function DomWatch._followRemovals(self: DomWatch)
    local sync = self._sync
    self._trove:Add(self._root.DescendantRemoving:Connect(function(descendant)
        local patch = sync:Remove(descendant)
        if patch then
            self.onPatch:Fire(patch)
        end
    end))
end

--- Start following changes.
function DomWatch.Start(self: DomWatch)
    if self._started then
        return
    end
    self._started = true

    self:_followRemovals()
    if self._isLazy then
        -- Seed the roots so the tree has something to show; deeper levels load on demand.
        self.onPatch:Fire(self._sync:Children(self._root))
        return
    end

    local sync = self._sync
    local root = self._root
    self._trove:Add(root.DescendantAdded:Connect(function(descendant)
        self.onPatch:Fire(sync:Upsert(descendant))
    end))

    self.onPatch:Fire(sync:Snapshot(root))
end

--- Mirror `instance` and its immediate children on demand.
function DomWatch.SyncChildren(self: DomWatch, instance: Instance?)
    self.onPatch:Fire(self._sync:Children(instance or self._root))
end

--- Mirror a single `instance`.
function DomWatch.SyncNode(self: DomWatch, instance: Instance)
    self.onPatch:Fire(self._sync:Upsert(instance))
end

--- Snapshot `instance`'s subtree on demand (the whole watch scope when `nil`).
function DomWatch.SyncSnapshot(self: DomWatch, instance: Instance?)
    self.onPatch:Fire(self._sync:Snapshot(instance or self._root))
end

--- Mirror `from`'s descendants whose name matches `query`, on demand.
function DomWatch.SyncSearch(self: DomWatch, from: Instance, query: string)
    self.onPatch:Fire(self._sync:Search(from, query))
end

--- Mirror each of `instances` (structure only) in one patch, on demand.
function DomWatch.SyncNodes(self: DomWatch, instances: { Instance })
    self.onPatch:Fire(self._sync:Nodes(instances))
end

return DomWatch
