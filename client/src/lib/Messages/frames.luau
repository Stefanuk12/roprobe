-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local dom = require("@lib/Messages/dom")
local operations = require("@lib/Messages/operations")
local primitives = require("@lib/Messages/primitives")
local remote = require("@lib/Messages/remote")
local serdeArray = require("@lib/serdeArray")
local squashEnum = require("@lib/squashEnum")

-- Primitives
local boolean = primitives.boolean
local optStr = primitives.optStr
local str = primitives.str
local strArray = primitives.strArray
local u32 = primitives.u32

-- Dom
local domPatch = dom.domPatch

-- Operations
local enumFamilies = operations.enumFamilies
local operation = operations.operation
local opResult = operations.opResult

-- Remote spy
local remoteCalls = remote.remoteCalls
local spyConfig = remote.spyConfig

-- Types
type ClientMessage = Broker.ClientMessage
type LogEntry = Broker.LogEntry
type LogLevel = Broker.LogLevel
type OperationRequest = Broker.OperationRequest
type OperationResponse = Broker.OperationResponse
type ServerMessage = Broker.ServerMessage
type Upstream = Broker.Upstream
type UpstreamState = Broker.UpstreamState

--- Mirrors `Upstream`.
local upstream: Squash.SerDes<Upstream> = Squash.literal("Verde", "LuauLsp")

local upstreamState: Squash.SerDes<UpstreamState> = {
    ser = function(cursor, state)
        boolean.ser(cursor, state.enabled)
        upstream.ser(cursor, state.upstream)
    end,

    des = function(cursor)
        local up = upstream.des(cursor)
        local enabled = boolean.des(cursor)
        return { upstream = up, enabled = enabled } :: UpstreamState
    end,
}

--- Mirrors `ServerMessage::Operation`'s `{ id, op }` payload.
local operationRequest: Squash.SerDes<OperationRequest> = {
    ser = function(cursor, value)
        operation.ser(cursor, value.op)
        u32.ser(cursor, value.id)
    end,

    des = function(cursor)
        local id = u32.des(cursor)
        local op = operation.des(cursor)
        return { id = id, op = op }
    end,
}

--- Mirrors `ClientMessage::OperationResult`'s `{ id, result }` payload.
local operationResponse: Squash.SerDes<OperationResponse> = {
    ser = function(cursor, value)
        opResult.ser(cursor, value.result)
        u32.ser(cursor, value.id)
    end,

    des = function(cursor)
        local id = u32.des(cursor)
        local result = opResult.des(cursor)
        return { id = id, result = result }
    end,
}

--- Mirrors `LogLevel`.
local logLevel: Squash.SerDes<LogLevel> = Squash.literal("print", "info", "warn", "error")

--- Mirrors `LogEntry`, a plain struct: `ser` runs forward and `des` in reverse.
local logEntry: Squash.SerDes<LogEntry> = {
    ser = function(cursor, entry)
        logLevel.ser(cursor, entry.level)
        str.ser(cursor, entry.content)
    end,

    des = function(cursor)
        local content = str.des(cursor)
        local level = logLevel.des(cursor)
        return { level = level, content = content } :: LogEntry
    end,
}

local logEntries = serdeArray(logEntry)

--- Mirrors `ClientMessage`.
local clientMessage: Squash.SerDes<ClientMessage> = squashEnum({
    { type = "Shutdown" :: "Shutdown" } :: any,
    { type = "SetUpstream" :: "SetUpstream", content = Squash.T(upstreamState) },
    { type = "UpdateDom" :: "UpdateDom", content = Squash.T(domPatch) },
    { type = "EnumFamilies" :: "EnumFamilies", content = Squash.T(enumFamilies) },
    { type = "OperationResult" :: "OperationResult", content = Squash.T(operationResponse) },
    { type = "RequestActive" :: "RequestActive" } :: any,
    { type = "Log" :: "Log", content = Squash.T(logEntries) },
    { type = "RemoteCalls" :: "RemoteCalls", content = Squash.T(remoteCalls) },
})

--- Mirrors `Search`'s `{ from, query }` payload.
local searchQuery: Squash.SerDes<{ from: string, query: string }> = {
    ser = function(cursor, value)
        str.ser(cursor, value.query)
        str.ser(cursor, value.from)
    end,

    des = function(cursor)
        local from = str.des(cursor)
        local query = str.des(cursor)
        return { from = from, query = query }
    end,
}

--- Mirrors `ServerMessage`.
local serverMessage: Squash.SerDes<ServerMessage> = squashEnum({
    { type = "Hello" :: "Hello" } :: any,
    { type = "UpstreamChanged" :: "UpstreamChanged", content = Squash.T(upstreamState) },
    { type = "RequestChildren" :: "RequestChildren", content = Squash.T(optStr) },
    { type = "RequestNode" :: "RequestNode", content = Squash.T(str) },
    { type = "RequestSnapshot" :: "RequestSnapshot", content = Squash.T(optStr) },
    { type = "Search" :: "Search", content = Squash.T(searchQuery) },
    { type = "RequestNodes" :: "RequestNodes", content = Squash.T(strArray) },
    { type = "Operation" :: "Operation", content = Squash.T(operationRequest) },
    { type = "Spy" :: "Spy", content = Squash.T(spyConfig) },
})

return {
    upstream = upstream,
    upstreamState = upstreamState,
    clientMessage = clientMessage,
    serverMessage = serverMessage,
}
