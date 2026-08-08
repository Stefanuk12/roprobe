-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local primitives = require("@lib/Messages/primitives")
local serdeArray = require("@lib/serdeArray")
local squashEnum = require("@lib/squashEnum")
local values = require("@lib/Messages/values")

-- Primitives
local boolean = primitives.boolean
local f64 = primitives.f64
local rawBuffer = primitives.rawBuffer
local str = primitives.str
local u16 = primitives.u16
local u32 = primitives.u32
local u8 = primitives.u8

-- Values
local domValue = values.domValue

-- Types
type CallSource = Broker.CallSource
type FunctionRef = Broker.FunctionRef
type InstanceRef = Broker.InstanceRef
type LuaEntry = Broker.LuaEntry
type LuaTable = Broker.LuaTable
type LuaValue = Broker.LuaValue
type RemoteCall = Broker.RemoteCall
type RemoteDirection = Broker.RemoteDirection
type SpyConfig = Broker.SpyConfig

local optStr = Squash.opt(str)
local optU32 = Squash.opt(u32)

--- Mirrors `InstanceRef`, a plain struct: `ser` runs forward and `des` in reverse.
local instanceRef: Squash.SerDes<InstanceRef> = {
    ser = function(cursor, value)
        str.ser(cursor, value.id)
        str.ser(cursor, value.class)
        str.ser(cursor, value.name)
        str.ser(cursor, value.path)
    end,

    des = function(cursor)
        local path = str.des(cursor)
        local name = str.des(cursor)
        local class = str.des(cursor)
        local id = str.des(cursor)
        return { id = id, class = class, name = name, path = path }
    end,
}

--- Mirrors `CallSource`, a plain struct.
local callSource: Squash.SerDes<CallSource> = {
    ser = function(cursor, value)
        optStr.ser(cursor, value.script)
        optStr.ser(cursor, value.functionName)
        optStr.ser(cursor, value.chunk)
        optU32.ser(cursor, value.line)
        boolean.ser(cursor, value.isExecutor)
        optStr.ser(cursor, value.actor)
    end,

    des = function(cursor)
        local actor = optStr.des(cursor)
        local isExecutor = boolean.des(cursor)
        local line = optU32.des(cursor)
        local chunk = optStr.des(cursor)
        local functionName = optStr.des(cursor)
        local script = optStr.des(cursor)
        return {
            script = script,
            functionName = functionName,
            chunk = chunk,
            line = line,
            isExecutor = isExecutor,
            actor = actor,
        }
    end,
}

--- Mirrors `FunctionRef`, a plain struct.
local functionRef: Squash.SerDes<FunctionRef> = {
    ser = function(cursor, value)
        optStr.ser(cursor, value.name)
        optStr.ser(cursor, value.chunk)
        optU32.ser(cursor, value.line)
    end,

    des = function(cursor)
        local line = optU32.des(cursor)
        local chunk = optStr.des(cursor)
        local name = optStr.des(cursor)
        return { name = name, chunk = chunk, line = line }
    end,
}

--- Mirrors `RemoteDirection`.
local remoteDirection: Squash.SerDes<RemoteDirection> = Squash.literal("Outgoing", "Incoming")

--- Raw bytes, so a remote's packed binary payload survives the trip intact.
local bytes: Squash.SerDes<buffer | string> = {
    ser = function(cursor, value)
        rawBuffer.ser(cursor, if type(value) == "string" then buffer.fromstring(value) else value)
    end,

    des = function(cursor)
        return rawBuffer.des(cursor)
    end,
}

-- `LuaValue` nests itself through tables, so the codec is declared before the
-- shapes that close over it and filled in below.
local luaValue: Squash.SerDes<LuaValue> = {} :: any
local luaValues = serdeArray(luaValue)

--- Mirrors `LuaEntry`, a plain struct.
local luaEntry: Squash.SerDes<LuaEntry> = {
    ser = function(cursor, entry)
        luaValue.ser(cursor, entry.key)
        luaValue.ser(cursor, entry.value)
    end,

    des = function(cursor)
        local value = luaValue.des(cursor)
        local key = luaValue.des(cursor)
        return { key = key, value = value }
    end,
}

local luaEntries = serdeArray(luaEntry)

--- Mirrors `LuaTable`, a plain struct.
local luaTable: Squash.SerDes<LuaTable> = {
    ser = function(cursor, value)
        u32.ser(cursor, value.id)
        luaValues.ser(cursor, value.array)
        luaEntries.ser(cursor, value.entries)
        boolean.ser(cursor, value.truncated)
        boolean.ser(cursor, value.metatable)
    end,

    des = function(cursor)
        local metatable = boolean.des(cursor)
        local truncated = boolean.des(cursor)
        local entries = luaEntries.des(cursor)
        local array = luaValues.des(cursor)
        local id = u32.des(cursor)
        return { id = id, array = array, entries = entries, truncated = truncated, metatable = metatable }
    end,
}

local luaValueCodec: Squash.SerDes<LuaValue> = squashEnum({
    { type = "Nil" :: "Nil" } :: any,
    { type = "Bool" :: "Bool", content = Squash.T(boolean) },
    { type = "Number" :: "Number", content = Squash.T(f64) },
    { type = "String" :: "String", content = Squash.T(bytes) },
    { type = "Table" :: "Table", content = Squash.T(luaTable) },
    { type = "Instance" :: "Instance", content = Squash.T(instanceRef) },
    { type = "Roblox" :: "Roblox", content = Squash.T(domValue) },
    { type = "Buffer" :: "Buffer", content = Squash.T(bytes) },
    { type = "Function" :: "Function", content = Squash.T(functionRef) },
    { type = "Cycle" :: "Cycle", content = Squash.T(u32) },
    { type = "Opaque" :: "Opaque", content = Squash.T(str) },
})
luaValue.ser = luaValueCodec.ser
luaValue.des = luaValueCodec.des

local optLuaValues = Squash.opt(luaValues)

--- Mirrors `RemoteCall`, a plain struct.
local remoteCall: Squash.SerDes<RemoteCall> = {
    ser = function(cursor, call)
        u32.ser(cursor, call.id)
        remoteDirection.ser(cursor, call.direction)
        instanceRef.ser(cursor, call.remote)
        str.ser(cursor, call.method)
        luaValues.ser(cursor, call.arguments)
        optLuaValues.ser(cursor, call.returns)
        callSource.ser(cursor, call.source)
        f64.ser(cursor, call.timestamp)
    end,

    des = function(cursor)
        local timestamp = f64.des(cursor)
        local source = callSource.des(cursor)
        local returns = optLuaValues.des(cursor)
        local arguments = luaValues.des(cursor)
        local method = str.des(cursor)
        local remote = instanceRef.des(cursor)
        local direction = remoteDirection.des(cursor)
        local id = u32.des(cursor)
        return {
            id = id,
            direction = direction,
            remote = remote,
            method = method,
            arguments = arguments,
            returns = returns,
            source = source,
            timestamp = timestamp,
        }
    end,
}

local remoteCalls = serdeArray(remoteCall)

--- Mirrors `SpyConfig`, a plain struct.
local spyConfig: Squash.SerDes<SpyConfig> = {
    ser = function(cursor, config)
        boolean.ser(cursor, config.enabled)
        boolean.ser(cursor, config.outgoing)
        boolean.ser(cursor, config.incoming)
        boolean.ser(cursor, config.bindables)
        u8.ser(cursor, config.maxDepth)
        u16.ser(cursor, config.maxEntries)
        u32.ser(cursor, config.maxBytes)
    end,

    des = function(cursor)
        local maxBytes = u32.des(cursor)
        local maxEntries = u16.des(cursor)
        local maxDepth = u8.des(cursor)
        local bindables = boolean.des(cursor)
        local incoming = boolean.des(cursor)
        local outgoing = boolean.des(cursor)
        local enabled = boolean.des(cursor)
        return {
            enabled = enabled,
            outgoing = outgoing,
            incoming = incoming,
            bindables = bindables,
            maxDepth = maxDepth,
            maxEntries = maxEntries,
            maxBytes = maxBytes,
        }
    end,
}

return {
    instanceRef = instanceRef,
    callSource = callSource,
    functionRef = functionRef,
    luaValue = luaValue,
    luaValues = luaValues,
    luaTable = luaTable,
    remoteCall = remoteCall,
    remoteCalls = remoteCalls,
    spyConfig = spyConfig,
}
