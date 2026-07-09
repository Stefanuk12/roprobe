-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local primitives = require("@lib/Messages/primitives")
local serdeArray = require("@lib/serdeArray")
local squashEnum = require("@lib/squashEnum")
local values = require("@lib/Messages/values")

-- Primitives
local optStr = primitives.optStr
local str = primitives.str
local strArray = primitives.strArray
local vlq = primitives.vlq

-- Values
local domValue = values.domValue

-- Types
type DomInstance = Broker.DomInstance
type DomPatch = Broker.DomPatch
type DomUpdate = Broker.DomUpdate
type DomValue = Broker.DomValue
type TagChange = Broker.TagChange

local properties = Squash.map(str, domValue)
local optTags = Squash.opt(serdeArray(str))
local domInstance: Squash.SerDes<DomInstance> = {
    ser = function(cursor, instance)
        str.ser(cursor, instance.id)
        optStr.ser(cursor, instance.parent)
        str.ser(cursor, instance.class)
        str.ser(cursor, instance.name)
        properties.ser(cursor, instance.properties)
        properties.ser(cursor, instance.attributes)
        optTags.ser(cursor, instance.tags)
    end,

    des = function(cursor)
        local tags = optTags.des(cursor)
        local attributes = properties.des(cursor)
        local props = properties.des(cursor)
        local name = str.des(cursor)
        local class = str.des(cursor)
        local parent = optStr.des(cursor)
        local id = str.des(cursor)
        return {
            id = id,
            parent = parent,
            class = class,
            name = name,
            properties = props,
            attributes = attributes,
            tags = tags,
        }
    end,
}
local optDomValue = Squash.opt(domValue)

--- Write `present` plus a `None` per `removed` key as a serde map.
local function serChangeMap(cursor: Squash.Cursor, present: { [string]: DomValue }, removed: { string })
    local count = 0

    for key, value in present do
        optDomValue.ser(cursor, value)
        str.ser(cursor, key)
        count += 1
    end

    for _, key in removed do
        optDomValue.ser(cursor, nil)
        str.ser(cursor, key)
        count += 1
    end

    vlq.ser(cursor, count)
end

--- Read a change map back, splitting `None`s into a removed-keys list (Luau can't hold nil values).
local function desChangeMap(cursor: Squash.Cursor): ({ [string]: DomValue }, { string })
    local present: { [string]: DomValue } = {}
    local removed: { string } = {}

    for _ = 1, vlq.des(cursor) do
        local key = str.des(cursor)
        local value = optDomValue.des(cursor)

        if value == nil then
            table.insert(removed, key)
        else
            present[key] = value
        end
    end

    return present, removed
end

--- Mirrors `TagChange::Delta`
local tagDelta: Squash.SerDes<{ add: { string }, remove: { string } }> = {
    ser = function(cursor, delta: { add: { string }, remove: { string } })
        strArray.ser(cursor, delta.remove)
        strArray.ser(cursor, delta.add)
    end,

    des = function(cursor)
        local add = strArray.des(cursor)
        local remove = strArray.des(cursor)
        return { add = add, remove = remove }
    end,
}

--- Mirrors `TagChange`.
local tagChange: Squash.SerDes<TagChange> = squashEnum({
    { type = "None" :: "None" } :: any,
    { type = "Replace" :: "Replace", content = Squash.T(strArray) },
    { type = "Delta" :: "Delta", content = Squash.T(tagDelta) },
}) :: any

local TAGS_UNCHANGED: TagChange = { type = "None" :: "None", content = nil }

--- Mirrors `DomUpdate`.
local domUpdate: Squash.SerDes<DomUpdate> = {
    ser = function(cursor, update: DomUpdate)
        str.ser(cursor, update.id)
        serChangeMap(cursor, update.properties, update.removedProperties)
        serChangeMap(cursor, update.attributes, update.removedAttributes)
        tagChange.ser(cursor, update.tags or TAGS_UNCHANGED)
    end,

    des = function(cursor)
        local tags = tagChange.des(cursor)
        local attributes, removedAttributes = desChangeMap(cursor)
        local props, removedProperties = desChangeMap(cursor)
        local id = str.des(cursor)
        return {
            id = id,
            properties = props,
            removedProperties = removedProperties,
            attributes = attributes,
            removedAttributes = removedAttributes,
            tags = tags,
        }
    end,
}

local upserts = serdeArray(domInstance)
local removals = strArray
local updates = serdeArray(domUpdate)

--- Mirrors `DomPatch`.
local domPatch: Squash.SerDes<DomPatch> = {
    ser = function(cursor, patch)
        upserts.ser(cursor, patch.upserts)
        removals.ser(cursor, patch.removals)
        updates.ser(cursor, patch.updates)
    end,

    des = function(cursor)
        local updated = updates.des(cursor)
        local removed = removals.des(cursor)
        local upserted = upserts.des(cursor)
        return { upserts = upserted, removals = removed, updates = updated }
    end,
}

return {
    domInstance = domInstance,
    tagChange = tagChange,
    domUpdate = domUpdate,
    domPatch = domPatch,
}
