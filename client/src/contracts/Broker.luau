-- Dependencies
local squashEnum = require("@lib/squashEnum")

-- Types
--- A reference to a mirrored instance by its stable per-session id (mirrors the
--- Rust `DomId`).
export type DomId = string
export type Upstream = "Verde" | "LuauLsp"
export type UpstreamState = {
    upstream: Upstream,
    enabled: boolean,
}
export type DomValue =
    | squashEnum.EnumVariant<"Bool", boolean>
    | squashEnum.EnumVariant<"Float", number>
    | squashEnum.EnumVariant<"Int", number>
    | squashEnum.EnumVariant<"String", string>
    | squashEnum.EnumVariant<"Ref", DomId>
    | squashEnum.EnumVariant<"Enum", EnumItem | number>
    | squashEnum.EnumVariant<"Vector2", Vector2>
    | squashEnum.EnumVariant<"Vector3", Vector3>
    | squashEnum.EnumVariant<"Color3", Color3>
    | squashEnum.EnumVariant<"UDim", UDim>
    | squashEnum.EnumVariant<"UDim2", UDim2>
    | squashEnum.EnumVariant<"NumberRange", NumberRange>
    | squashEnum.EnumVariant<"Rect", Rect>
    | squashEnum.EnumVariant<"BrickColor", BrickColor>
    | squashEnum.EnumVariant<"CFrame", CFrame>
    | squashEnum.EnumVariant<"Float32", number>
    | squashEnum.EnumVariant<"Int32", number>
    | squashEnum.EnumVariant<"ContentId", string>
    | squashEnum.EnumVariant<"BinaryString", buffer | string>
    | squashEnum.EnumVariant<"Color3uint8", Color3>
    | squashEnum.EnumVariant<"Vector2int16", Vector2int16>
    | squashEnum.EnumVariant<"Vector3int16", Vector3int16>
    | squashEnum.EnumVariant<"Ray", Ray>
    | squashEnum.EnumVariant<"Region3", Region3>
    | squashEnum.EnumVariant<"Region3int16", Region3int16>
    | squashEnum.EnumVariant<"Axes", Axes>
    | squashEnum.EnumVariant<"Faces", Faces>
    | squashEnum.EnumVariant<"Font", Font>
    | squashEnum.EnumVariant<"NumberSequence", NumberSequence>
    | squashEnum.EnumVariant<"ColorSequence", ColorSequence>
    | squashEnum.EnumVariant<"PhysicalProperties", PhysicalProperties?>
    | squashEnum.EnumVariant<"OptionalCFrame", CFrame?>
    | squashEnum.EnumVariant<"Content", Content | ContentValue>

export type ContentValue =
    | { type: "None", content: nil }
    | { type: "Uri", content: string? }
    | { type: "Object", content: DomId }
export type DomInstance = {
    id: DomId,
    parent: DomId?,
    class: string,
    name: string,
    properties: { [string]: DomValue },
    attributes: { [string]: DomValue },
    tags: { string }?,
}
export type TagChange =
    | squashEnum.EnumVariant<"None">
    | squashEnum.EnumVariant<"Replace", { string }>
    | squashEnum.EnumVariant<"Delta", { add: { string }, remove: { string } }>

export type DomUpdate = {
    id: DomId,
    properties: { [string]: DomValue },
    removedProperties: { string },
    attributes: { [string]: DomValue },
    removedAttributes: { string },
    tags: TagChange?,
}
export type DomPatch = {
    upserts: { DomInstance },
    removals: { DomId },
    updates: { DomUpdate },
}

export type ClientMessage =
    | squashEnum.EnumVariant<"Shutdown">
    | squashEnum.EnumVariant<"SetUpstream", UpstreamState>
    | squashEnum.EnumVariant<"UpdateDom", DomPatch>
    | squashEnum.EnumVariant<"EnumFamilies", { string }>
export type SearchQuery = { from: DomId, query: string }
export type ServerMessage =
    | squashEnum.EnumVariant<"Hello">
    | squashEnum.EnumVariant<"UpstreamChanged", UpstreamState>
    | squashEnum.EnumVariant<"RequestChildren", DomId?>
    | squashEnum.EnumVariant<"RequestNode", DomId>
    | squashEnum.EnumVariant<"RequestSnapshot", DomId?>
    | squashEnum.EnumVariant<"Search", SearchQuery>
    | squashEnum.EnumVariant<"RequestNodes", { DomId }>

-- Type-only module; darklua requires a runtime return value.
return true
