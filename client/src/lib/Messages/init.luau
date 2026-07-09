-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local dom = require("@lib/Messages/dom")
local frames = require("@lib/Messages/frames")
local tags = require("@lib/Messages/tags")
local values = require("@lib/Messages/values")

-- Frames
local clientMessage = frames.clientMessage
local serverMessage = frames.serverMessage

-- Types
-- Wire shapes live in the [`Broker`](@contracts/Broker) contract; re-exported so codec consumers share one module.
export type Upstream = Broker.Upstream
export type UpstreamState = Broker.UpstreamState
export type DomValue = Broker.DomValue
export type ContentValue = Broker.ContentValue
export type DomInstance = Broker.DomInstance
export type TagChange = Broker.TagChange
export type DomUpdate = Broker.DomUpdate
export type DomPatch = Broker.DomPatch
export type ClientMessage = Broker.ClientMessage
export type ServerMessage = Broker.ServerMessage

local Messages = {}

-- Serdes
Messages.upstream = frames.upstream
Messages.upstreamState = frames.upstreamState
Messages.domValue = values.domValue
Messages.domInstance = dom.domInstance
Messages.tagChange = dom.tagChange
Messages.domUpdate = dom.domUpdate
Messages.domPatch = dom.domPatch
Messages.clientMessage = clientMessage
Messages.serverMessage = serverMessage
Messages.enumFamilies = values.familyNames

-- Tag introspection
Messages.tagOf = tags.tagOf
Messages.wrappable = tags.wrappable
Messages.attributeSafeTag = tags.attributeSafeTag
Messages.knownTag = tags.knownTag
Messages.contentMatchesTag = tags.contentMatchesTag
Messages.value = tags.value

--- Encode a [`ClientMessage`] into a binary frame for the broker.
function Messages.encode(message: ClientMessage): buffer
    local cursor = Squash.cursor(256)
    clientMessage.ser(cursor, message)
    return Squash.tobuffer(cursor)
end

--- Decode a binary frame from the broker.
--- Throws on malformed/unknown frames, so callers on untrusted data should `pcall`.
function Messages.decode(frame: buffer | string): ServerMessage
    local buf = if type(frame) == "string" then buffer.fromstring(frame) else frame
    return serverMessage.des(Squash.frombuffer(buf))
end

return Messages
