-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local dom = require("@lib/Messages/dom")
local primitives = require("@lib/Messages/primitives")
local squashEnum = require("@lib/squashEnum")

-- Primitives
local boolean = primitives.boolean
local optStr = primitives.optStr
local str = primitives.str
local strArray = primitives.strArray

-- Dom
local domPatch = dom.domPatch

-- Types
type ClientMessage = Broker.ClientMessage
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
        return { upstream = up, enabled = enabled }
    end,
}

--- Mirrors `ClientMessage`.
local clientMessage: Squash.SerDes<ClientMessage> = squashEnum({
    { type = "Shutdown" :: "Shutdown" } :: any,
    { type = "SetUpstream" :: "SetUpstream", content = Squash.T(upstreamState) },
    { type = "UpdateDom" :: "UpdateDom", content = Squash.T(domPatch) },
    { type = "EnumFamilies" :: "EnumFamilies", content = Squash.T(strArray) },
}) :: any

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
}) :: any

return {
    upstream = upstream,
    upstreamState = upstreamState,
    clientMessage = clientMessage,
    serverMessage = serverMessage,
}
