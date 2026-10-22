-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local primitives = require("@lib/Messages/primitives")
local remote = require("@lib/Messages/remote")

-- Primitives
local boolean = primitives.boolean
local f64 = primitives.f64
local rawBuffer = primitives.rawBuffer
local str = primitives.str
local strArray = primitives.strArray
local u32 = primitives.u32
local u8 = primitives.u8

-- Remote spy
local callSource = remote.callSource
local instanceRef = remote.instanceRef
local luaValues = remote.luaValues

-- Types
type BytecodeData = Broker.BytecodeData
type ClosureInfo = Broker.ClosureInfo
type HookedCall = Broker.HookedCall
type TraceEvent = Broker.TraceEvent

local optLuaValues = Squash.opt(luaValues)

-- fields go backwards, like my life

-- Vec<u8> is a seq, not bytes
local byteVec: Squash.SerDes<string> = {
    ser = function(cursor, value)
        rawBuffer.ser(cursor, buffer.fromstring(string.reverse(value)))
    end,

    des = function(cursor)
        return string.reverse(buffer.tostring(rawBuffer.des(cursor)))
    end,
}

--- Mirrors `ClientMessage::BytecodeData`.
local bytecodeData: Squash.SerDes<BytecodeData> = {
    ser = function(cursor, value)
        byteVec.ser(cursor, value.bytes)
        str.ser(cursor, value.scriptPath)
        u32.ser(cursor, value.request)
    end,

    des = function(cursor)
        local request = u32.des(cursor)
        local scriptPath = str.des(cursor)
        local bytes = byteVec.des(cursor)
        return { request = request, scriptPath = scriptPath, bytes = bytes }
    end,
}

--- Mirrors `ClientMessage::ClosureInfo`.
local closureInfo: Squash.SerDes<ClosureInfo> = {
    ser = function(cursor, value)
        u8.ser(cursor, value.nparams)
        str.ser(cursor, value.source)
        strArray.ser(cursor, value.protos)
        luaValues.ser(cursor, value.upvalues)
        luaValues.ser(cursor, value.constants)
        str.ser(cursor, value.name)
        u32.ser(cursor, value.request)
    end,

    des = function(cursor)
        local request = u32.des(cursor)
        local name = str.des(cursor)
        local constants = luaValues.des(cursor)
        local upvalues = luaValues.des(cursor)
        local protos = strArray.des(cursor)
        local source = str.des(cursor)
        local nparams = u8.des(cursor)
        return {
            request = request,
            name = name,
            constants = constants,
            upvalues = upvalues,
            protos = protos,
            source = source,
            nparams = nparams,
        }
    end,
}

--- Mirrors `ClientMessage::HookedCall`.
local hookedCall: Squash.SerDes<HookedCall> = {
    ser = function(cursor, value)
        f64.ser(cursor, value.timestamp)
        callSource.ser(cursor, value.caller)
        optLuaValues.ser(cursor, value.returns)
        luaValues.ser(cursor, value.args)
        instanceRef.ser(cursor, value.remote)
    end,

    des = function(cursor)
        local remoteRef = instanceRef.des(cursor)
        local args = luaValues.des(cursor)
        local returns = optLuaValues.des(cursor)
        local caller = callSource.des(cursor)
        local timestamp = f64.des(cursor)
        return { remote = remoteRef, args = args, returns = returns, caller = caller, timestamp = timestamp }
    end,
}

--- Mirrors `ClientMessage::TraceEvent`.
local traceEvent: Squash.SerDes<TraceEvent> = {
    ser = function(cursor, value)
        f64.ser(cursor, value.timestamp)
        luaValues.ser(cursor, value.returns)
        luaValues.ser(cursor, value.args)
        str.ser(cursor, value.target)
    end,

    des = function(cursor)
        local target = str.des(cursor)
        local args = luaValues.des(cursor)
        local returns = luaValues.des(cursor)
        local timestamp = f64.des(cursor)
        return { target = target, args = args, returns = returns, timestamp = timestamp }
    end,
}

--- Mirrors `ServerMessage::GetBytecode`'s `{ request, script_path }`.
local getBytecode: Squash.SerDes<{ request: number, scriptPath: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.scriptPath)
        u32.ser(cursor, value.request)
    end,

    des = function(cursor)
        local request = u32.des(cursor)
        local scriptPath = str.des(cursor)
        return { request = request, scriptPath = scriptPath }
    end,
}

--- Mirrors `ServerMessage::InspectClosure`'s `{ request, path }`.
local inspectClosure: Squash.SerDes<{ request: number, path: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.path)
        u32.ser(cursor, value.request)
    end,

    des = function(cursor)
        local request = u32.des(cursor)
        local path = str.des(cursor)
        return { request = request, path = path }
    end,
}

--- Mirrors `ServerMessage::HookRemote`'s `{ remote_path, mode }`.
local hookRemote: Squash.SerDes<{ remotePath: string, mode: number }> = {
    ser = function(cursor, value)
        u8.ser(cursor, value.mode)
        str.ser(cursor, value.remotePath)
    end,

    des = function(cursor)
        local remotePath = str.des(cursor)
        local mode = u8.des(cursor)
        return { remotePath = remotePath, mode = mode }
    end,
}

--- Mirrors `ServerMessage::UnhookRemote`'s `{ remote_path }`.
local unhookRemote: Squash.SerDes<{ remotePath: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.remotePath)
    end,

    des = function(cursor)
        return { remotePath = str.des(cursor) }
    end,
}

--- Mirrors `ServerMessage::Trace`'s `{ target_path, enabled }`.
local trace: Squash.SerDes<{ targetPath: string, enabled: boolean }> = {
    ser = function(cursor, value)
        boolean.ser(cursor, value.enabled)
        str.ser(cursor, value.targetPath)
    end,

    des = function(cursor)
        local targetPath = str.des(cursor)
        local enabled = boolean.des(cursor)
        return { targetPath = targetPath, enabled = enabled }
    end,
}

--- Mirrors `ServerMessage::ScanConstants`'s `{ request, pattern }`.
local scanConstants: Squash.SerDes<{ request: number, pattern: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.pattern)
        u32.ser(cursor, value.request)
    end,

    des = function(cursor)
        local request = u32.des(cursor)
        local pattern = str.des(cursor)
        return { request = request, pattern = pattern }
    end,
}

return {
    byteVec = byteVec,
    bytecodeData = bytecodeData,
    closureInfo = closureInfo,
    hookedCall = hookedCall,
    traceEvent = traceEvent,
    getBytecode = getBytecode,
    inspectClosure = inspectClosure,
    hookRemote = hookRemote,
    unhookRemote = unhookRemote,
    trace = trace,
    scanConstants = scanConstants,
}
