-- Dependencies
local Messages = require("@lib/Messages")

--- Validate property values without mutating state, so a bad value can't error
--- after instances are already marked mirrored.
local function validateProperties(properties: RawProperties?)
    if not properties then
        return
    end

    for key, value in properties do
        if typeof(value) == "Instance" then
            assert(value ~= game, `property "{key}" cannot reference game; it is the session's root`)
        elseif typeof(value) == "Content" and value.SourceType == Enum.ContentSourceType.Object then
            local target = value.Object
            assert(target ~= nil and target ~= game, `property "{key}" Content must reference a non-game instance`)
        elseif type(value) == "table" then
            local domValue = value :: { type: string, content: any }
            assert(Messages.knownTag(domValue.type), `property "{key}" is not a DomValue`)
            assert(
                Messages.contentMatchesTag(domValue.type, domValue.content),
                `property "{key}" content is not a valid {domValue.type}`
            )
        else
            assert(Messages.wrappable(value), `property "{key}" has unsupported type {typeof(value)}`)
        end
    end
end

--- Validate attribute values without mutating any state.
local function validateAttributes(attributes: RawAttributes?)
    if not attributes then
        return
    end

    for key, domValue in attributes do
        if type(domValue) == "table" then
            assert(Messages.knownTag(domValue.type), `attribute "{key}" is not a DomValue`)
            assert(Messages.attributeSafeTag(domValue.type), `attribute "{key}" is not a legal Roblox attribute type`)
            assert(
                Messages.contentMatchesTag(domValue.type, domValue.content),
                `attribute "{key}" content is not a valid {domValue.type}`
            )
        else
            local tag = Messages.tagOf(domValue)
            assert(
                tag ~= nil and Messages.attributeSafeTag(tag),
                `attribute "{key}" has unsupported type {typeof(domValue)}`
            )
        end
    end
end

--- Ensure the values are an array (contiguous) and not a table/dict.
local function validateStringArray(values: { string }?, label: string)
    if not values then
        return
    end

    local count = #values
    local seen = 0
    for index, value in values do
        assert(type(index) == "number" and index >= 1 and index <= count, `{label}s must be a plain array`)
        assert(type(value) == "string", `{label} #{index} is not a string`)
        seen += 1
    end

    assert(seen == count, `{label}s must be a plain array without gaps`)
end

--- Lazily mirrors instances into the broker's per-session DOM.
local DomSync = {}
DomSync.__index = DomSync

-- Types
export type DomSyncData = {
    _ids: { [Instance]: string },
    _instanceById: { [string]: Instance },
    _parents: { [Instance]: Instance },
    _nextId: number,
}
export type DomSync = setmetatable<DomSyncData, typeof(DomSync)>

export type RawProperties = { [string]: boolean | number | string | Instance | Messages.DomValue }
export type RawAttributes = { [string]: boolean | number | string | Messages.DomValue }

--- Everything [`DomSync.Update`] can change on a mirrored instance.
export type UpdateChanges = {
    properties: RawProperties?,
    removeProperties: { string }?,
    attributes: RawAttributes?,
    removeAttributes: { string }?,
    --- Replace the stored tag set in full (`{}` clears it), mutually exclusive with `addTags`/`removeTags`.
    tags: { string }?,
    addTags: { string }?,
    removeTags: { string }?,
}

--- Create a new [`DomSync`] object.
function DomSync.new(): DomSync
    local self: DomSyncData = {
        _ids = {},
        _instanceById = {},
        _parents = {},
        _nextId = 1,
    }
    return setmetatable(self, DomSync)
end

--- The id an instance was mirrored under, if it has been.
function DomSync.IdOf(self: DomSync, instance: Instance): string?
    return self._ids[instance]
end

--- The mirrored instance an id belongs to, if it is still mirrored.
function DomSync.InstanceOf(self: DomSync, id: string): Instance?
    return self._instanceById[id]
end

--- Allocate an id for an unseen instance.
function DomSync._allocateId(self: DomSync, instance: Instance): string
    if gethiddenproperty then
        local ok, uniqueId = pcall(gethiddenproperty, instance, "UniqueId")
        -- Services (and freshly-created instances) report an all-zero UniqueId;
        -- those would all collide, so only trust a genuinely unique one.
        if ok and uniqueId ~= nil then
            local id = tostring(uniqueId)
            if not id:match("^0*$") then
                return id
            end
        end
    end

    local id = tostring(self._nextId)
    self._nextId += 1
    return id
end

--- Mirror `instance` (creating any unmirrored ancestors) into `upserts`, returning its id.
function DomSync._mirror(
    self: DomSync,
    instance: Instance,
    upserts: { Messages.DomInstance },
    refresh: boolean?
): string
    local id = self._ids[instance]
    if id and not refresh then
        return id
    end

    if not id then
        id = self:_allocateId(instance)
        self._ids[instance] = id
        self._instanceById[id] = instance
    end

    local parent = instance.Parent
    local mirroredParent = if parent ~= nil and parent ~= game then parent else nil
    self._parents[instance] = mirroredParent

    local record: Messages.DomInstance = {
        id = id,
        parent = if mirroredParent then self:_mirror(mirroredParent, upserts) else nil,
        class = instance.ClassName,
        name = instance.Name,
        hasChildren = #instance:GetChildren() > 0,
        properties = {},
        attributes = {},
        tags = nil,
    }
    table.insert(upserts, record)
    return id
end

--- Whether `instance`'s *mirrored* ancestor chain passes through `root`.
function DomSync._underMirrored(self: DomSync, instance: Instance, root: Instance): boolean
    local current = self._parents[instance]
    local seen: { [Instance]: true } = {}

    while current ~= nil do
        if current == root then
            return true
        elseif seen[current] then
            return false
        end

        seen[current] = true
        current = self._parents[current]
    end

    return false
end

--- Wrap validated raw property values into `out`.
function DomSync._wrapProperties(
    self: DomSync,
    out: { [string]: Messages.DomValue },
    properties: RawProperties?,
    upserts: { Messages.DomInstance }
)
    if not properties then
        return
    end

    for key, value in properties do
        if typeof(value) == "Instance" then
            out[key] = { type = "Ref" :: "Ref", content = self:_mirror(value, upserts) }
        elseif typeof(value) == "Content" and value.SourceType == Enum.ContentSourceType.Object then
            out[key] = {
                type = "Content" :: "Content",
                -- `value.Object` is validated before this call, so the cast is safe.
                content = { type = "Object" :: "Object", content = self:_mirror(value.Object :: Instance, upserts) },
            }
        elseif type(value) == "table" then
            out[key] = value
        else
            out[key] = Messages.value(value)
        end
    end
end

--- Wrap validated raw attribute values into `out`.
local function wrapAttributes(out: { [string]: Messages.DomValue }, attributes: RawAttributes?)
    if not attributes then
        return
    end

    for key, value in attributes do
        out[key] = if type(value) == "table" then value else Messages.value(value)
    end
end

--- Build a patch that mirrors `instance` (and any unseen ancestors) with the given properties, attributes and tags.
function DomSync.Upsert(
    self: DomSync,
    instance: Instance,
    properties: RawProperties?,
    attributes: RawAttributes?,
    tags: { string }?
): Messages.DomPatch
    assert(instance ~= game, "cannot mirror game itself; it is the session's root")

    validateProperties(properties)
    validateAttributes(attributes)
    validateStringArray(tags, "tag")

    local upserts: { Messages.DomInstance } = {}
    self:_mirror(instance, upserts, true)
    local record = upserts[#upserts]

    self:_wrapProperties(record.properties, properties, upserts)
    wrapAttributes(record.attributes, attributes)
    if tags then
        record.tags = table.clone(tags)
    end

    return { upserts = upserts, removals = {}, updates = {} }
end

--- Apply the changes onto the instance and upserting it if unseen.
function DomSync.Update(self: DomSync, instance: Instance, changes: UpdateChanges): Messages.DomPatch
    local id = self._ids[instance]
    if not id then
        return self:Upsert(instance, changes.properties, changes.attributes, changes.tags or changes.addTags)
    end

    assert(
        not (changes.tags and (changes.addTags or changes.removeTags)),
        "tags (replace) and addTags/removeTags (delta) are mutually exclusive"
    )
    validateProperties(changes.properties)
    validateStringArray(changes.removeProperties, "removed property")
    validateAttributes(changes.attributes)
    validateStringArray(changes.removeAttributes, "removed attribute")
    validateStringArray(changes.tags, "tag")
    validateStringArray(changes.addTags, "added tag")
    validateStringArray(changes.removeTags, "removed tag")

    local tags: Messages.TagChange? = nil
    if changes.tags then
        tags = { type = "Replace" :: "Replace", content = table.clone(changes.tags) }
    elseif changes.addTags or changes.removeTags then
        tags = {
            type = "Delta" :: "Delta",
            content = {
                add = if changes.addTags then table.clone(changes.addTags) else {},
                remove = if changes.removeTags then table.clone(changes.removeTags) else {},
            },
        }
    end

    -- Ref/Content property targets still ride along as upserts when unmirrored.
    local upserts: { Messages.DomInstance } = {}
    local update: Messages.DomUpdate = {
        id = id,
        properties = {},
        removedProperties = if changes.removeProperties then table.clone(changes.removeProperties) else {},
        attributes = {},
        removedAttributes = if changes.removeAttributes then table.clone(changes.removeAttributes) else {},
        tags = tags,
    }
    self:_wrapProperties(update.properties, changes.properties, upserts)
    wrapAttributes(update.attributes, changes.attributes)

    return { upserts = upserts, removals = {}, updates = { update } }
end

--- Capture an instance's live attributes as patch-ready values, skipping unsupported types.
function DomSync.AttributesOf(instance: Instance): { [string]: Messages.DomValue }
    local out: { [string]: Messages.DomValue } = {}

    for name, value in instance:GetAttributes() do
        local tag = Messages.tagOf(value)
        if tag ~= nil and Messages.attributeSafeTag(tag) then
            out[name] = Messages.value(value)
        end
    end

    return out
end

--- Capture an instance's live [`CollectionService`] tags.
function DomSync.TagsOf(instance: Instance): { string }
    return instance:GetTags()
end

--- Build a patch that snapshots a full subtree, pruning mirrored instances the live tree no longer has.
function DomSync.Snapshot(self: DomSync, root: Instance?): Messages.DomPatch
    local upserts: { Messages.DomInstance } = {}
    local visited: { [Instance]: boolean } = {}
    local unscanned: { Instance } = {}

    local function mirrorTree(top: Instance)
        visited[top] = true
        self:_mirror(top, upserts, true)

        local ok, descendants = pcall(function()
            return top:GetDescendants()
        end)
        if not ok then
            table.insert(unscanned, top)
            return
        end

        for _, descendant in descendants :: { Instance } do
            visited[descendant] = true
            self:_mirror(descendant, upserts, true)
        end
    end

    local scope = root or game
    if scope == game then
        for _, child in game:GetChildren() do
            mirrorTree(child)
        end
    else
        mirrorTree(scope)
    end

    -- Remove any nodes that don't exist anymore
    local staleRoots: { Instance } = {}
    for tracked in self._ids do
        if visited[tracked] or (scope ~= game and not self:_underMirrored(tracked, scope)) then
            continue
        end

        local inUnscanned = false
        for _, top in unscanned do
            if tracked == top or self:_underMirrored(tracked, top) then
                inUnscanned = true
                break
            end
        end

        if not inUnscanned then
            table.insert(staleRoots, tracked)
        end
    end

    -- Remove all stale instances
    local removals: { string } = {}
    for _, stale in staleRoots do
        local removed = self:Remove(stale)
        if removed then
            table.insert(removals, removed.removals[1])
        end
    end

    return { upserts = upserts, removals = removals, updates = {} }
end

--- Build a patch that mirrors `instance` and its immediate children.
function DomSync.Children(self: DomSync, instance: Instance): Messages.DomPatch
    local upserts: { Messages.DomInstance } = {}

    -- Mirror the instance itself
    if instance ~= game then
        self:_mirror(instance, upserts, true)
    end

    -- Then all the children
    for _, child in instance:GetChildren() do
        self:_mirror(child, upserts, true)
    end

    return { upserts = upserts, removals = {}, updates = {} }
end

--- Build a patch that queries the `from`'s descendants for instances matching the `query` selector.
function DomSync.Search(self: DomSync, from: Instance, query: string): Messages.DomPatch
    local ok, descendants = pcall(function()
        return from:QueryDescendants(query)
    end)
    if not ok then
        return { upserts = {}, removals = {}, updates = {} }
    end

    return self:Nodes(descendants)
end

--- Build a patch that mirrors each of `instances` providers, including ancestors.
function DomSync.Nodes(self: DomSync, instances: { Instance }): Messages.DomPatch
    local upserts: { Messages.DomInstance } = {}
    for _, instance in instances do
        self:_mirror(instance, upserts, true)
    end
    return { upserts = upserts, removals = {}, updates = {} }
end

--- Build a patch that removes an instance's mirrored subtree, or `nil` if it was never mirrored.
function DomSync.Remove(self: DomSync, instance: Instance): Messages.DomPatch?
    local id = self._ids[instance]
    if not id then
        return nil
    end

    local dropped: { Instance } = {}
    for tracked in self._ids do
        if tracked ~= instance and self:_underMirrored(tracked, instance) then
            table.insert(dropped, tracked)
        end
    end

    self._ids[instance] = nil
    self._instanceById[id] = nil
    self._parents[instance] = nil

    for _, tracked in dropped do
        local trackedId = self._ids[tracked] :: string
        self._ids[tracked] = nil
        self._instanceById[trackedId] = nil
        self._parents[tracked] = nil
    end

    return { upserts = {}, removals = { id }, updates = {} }
end

--- Forget everything mirrored so far.
function DomSync.Reset(self: DomSync)
    table.clear(self._ids)
    table.clear(self._instanceById)
    table.clear(self._parents)
    self._nextId = 1
end

return DomSync
