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
local u32 = primitives.u32

-- Values
local domValue = values.domValue

-- Types
type EnumEntry = Broker.EnumEntry
type EnumFamily = Broker.EnumFamily
type NamedValue = Broker.NamedValue
type OpResult = Broker.OpResult
type Operation = Broker.Operation
type Reads = Broker.Reads

-- Squash's cursor consumes from the tail, so enum variants land reversed while plain structs land in declaration order (proven by the Rust wire-layout pins).

-- Operation variant payloads (struct variants -> fields reversed).
--- A single-string payload, keyed by `key` (e.g. `{ node }` / `{ source }`).
local function singleString(key: string): Squash.SerDes<any>
    return {
        ser = function(cursor, value)
            str.ser(cursor, value[key])
        end,

        des = function(cursor)
            return { [key] = str.des(cursor) }
        end,
    }
end
local nodeOnly = singleString("node")

--- A `{ node, [key] }` payload of two strings.
local function nodeAnd(key: string): Squash.SerDes<any>
    return {
        ser = function(cursor, value)
            str.ser(cursor, value[key])
            str.ser(cursor, value.node)
        end,

        des = function(cursor)
            local node = str.des(cursor)
            return { node = node, [key] = str.des(cursor) }
        end,
    }
end

local moveContent: Squash.SerDes<{ node: string, parent: string? }> = {
    ser = function(cursor, value)
        optStr.ser(cursor, value.parent)
        str.ser(cursor, value.node)
    end,

    des = function(cursor)
        local node = str.des(cursor)
        local parent = optStr.des(cursor)
        return { node = node, parent = parent }
    end,
}
local createContent: Squash.SerDes<{ parent: string, class: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.class)
        str.ser(cursor, value.parent)
    end,

    des = function(cursor)
        local parent = str.des(cursor)
        local class = str.des(cursor)
        return { parent = parent, class = class }
    end,
}
--- Mirrors `SetProperty`/`SetAttribute`'s `{ node, name, value }`.
local setValueContent: Squash.SerDes<{ node: string, name: string, value: any }> = {
    ser = function(cursor, value)
        domValue.ser(cursor, value.value)
        str.ser(cursor, value.name)
        str.ser(cursor, value.node)
    end,

    des = function(cursor)
        local node = str.des(cursor)
        local name = str.des(cursor)
        return { node = node, name = name, value = domValue.des(cursor) }
    end,
}
--- Mirrors `GetProperties { node, properties }` — the broker fills `properties`.
local getPropertiesContent: Squash.SerDes<{ node: string, properties: { string } }> = {
    ser = function(cursor, value)
        strArray.ser(cursor, value.properties)
        str.ser(cursor, value.node)
    end,

    des = function(cursor)
        local node = str.des(cursor)
        local properties = strArray.des(cursor)
        return { node = node, properties = properties }
    end,
}
local renameAttributeContent: Squash.SerDes<{ node: string, old: string, new: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.new)
        str.ser(cursor, value.old)
        str.ser(cursor, value.node)
    end,

    des = function(cursor)
        local node = str.des(cursor)
        local old = str.des(cursor)
        return { node = node, old = old, new = str.des(cursor) }
    end,
}

--- Mirrors `Operation`.
local operation: Squash.SerDes<Operation> = squashEnum({
    { type = "Rename" :: "Rename", content = Squash.T(nodeAnd("name")) } :: any,
    { type = "Delete" :: "Delete", content = Squash.T(nodeOnly) },
    { type = "Move" :: "Move", content = Squash.T(moveContent) },
    { type = "Create" :: "Create", content = Squash.T(createContent) },
    { type = "AddTag" :: "AddTag", content = Squash.T(nodeAnd("tag")) },
    { type = "RemoveTag" :: "RemoveTag", content = Squash.T(nodeAnd("tag")) },
    { type = "SetProperty" :: "SetProperty", content = Squash.T(setValueContent) },
    { type = "SetAttribute" :: "SetAttribute", content = Squash.T(setValueContent) },
    { type = "RemoveAttribute" :: "RemoveAttribute", content = Squash.T(nodeAnd("name")) },
    { type = "RenameAttribute" :: "RenameAttribute", content = Squash.T(renameAttributeContent) },
    { type = "GetProperties" :: "GetProperties", content = Squash.T(getPropertiesContent) },
    { type = "RunCode" :: "RunCode", content = Squash.T(singleString("source")) },
}) :: any

-- Result structures (plain structs: `ser` forward, `des` reversed).
local namedValue: Squash.SerDes<NamedValue> = {
    ser = function(cursor, pair)
        str.ser(cursor, pair.name)
        domValue.ser(cursor, pair.value)
    end,

    des = function(cursor)
        local value = domValue.des(cursor)
        local name = str.des(cursor)
        return { name = name, value = value }
    end,
}
local namedValues = serdeArray(namedValue)
local reads: Squash.SerDes<Reads> = {
    ser = function(cursor, value)
        namedValues.ser(cursor, value.properties)
        strArray.ser(cursor, value.tags)
        namedValues.ser(cursor, value.attributes)
    end,

    des = function(cursor)
        local attributes = namedValues.des(cursor)
        local tags = strArray.des(cursor)
        local properties = namedValues.des(cursor)
        return { properties = properties, tags = tags, attributes = attributes }
    end,
}

--- Mirrors `OpResult`.
local opResult: Squash.SerDes<OpResult> = squashEnum({
    { type = "Ok" :: "Ok" } :: any,
    { type = "Reads" :: "Reads", content = Squash.T(reads) },
    { type = "Err" :: "Err", content = Squash.T(str) },
    { type = "Output" :: "Output", content = Squash.T(str) },
}) :: any

-- Enum catalog (plain structs).
local enumEntry: Squash.SerDes<EnumEntry> = {
    ser = function(cursor, entry)
        str.ser(cursor, entry.name)
        u32.ser(cursor, entry.value)
    end,

    des = function(cursor)
        local value = u32.des(cursor)
        local name = str.des(cursor)
        return { name = name, value = value }
    end,
}
local enumEntries = serdeArray(enumEntry)
local enumFamily: Squash.SerDes<EnumFamily> = {
    ser = function(cursor, family)
        str.ser(cursor, family.name)
        enumEntries.ser(cursor, family.items)
    end,

    des = function(cursor)
        local items = enumEntries.des(cursor)
        local name = str.des(cursor)
        return { name = name, items = items }
    end,
}

--- Mirrors `Vec<EnumFamily>` (the `EnumFamilies` message payload).
local enumFamilies = serdeArray(enumFamily)

return {
    operation = operation,
    opResult = opResult,
    enumFamilies = enumFamilies,
}
