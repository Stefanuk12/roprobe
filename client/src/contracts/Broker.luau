-- Dependencies
local enumVariant = require("@contracts/enumVariant")

-- Types
--- A reference to a mirrored instance by its stable per-session id.
export type DomId = string
export type Upstream = "Verde" | "LuauLsp"
export type UpstreamState = {
    upstream: Upstream,
    enabled: boolean,
}
export type DomValue =
    | enumVariant.EnumVariant<"Bool", boolean>
    | enumVariant.EnumVariant<"Float", number>
    | enumVariant.EnumVariant<"Int", number>
    | enumVariant.EnumVariant<"String", string>
    | enumVariant.EnumVariant<"Ref", DomId>
    | enumVariant.EnumVariant<"Enum", EnumItem | number>
    | enumVariant.EnumVariant<"Vector2", Vector2>
    | enumVariant.EnumVariant<"Vector3", Vector3>
    | enumVariant.EnumVariant<"Color3", Color3>
    | enumVariant.EnumVariant<"UDim", UDim>
    | enumVariant.EnumVariant<"UDim2", UDim2>
    | enumVariant.EnumVariant<"NumberRange", NumberRange>
    | enumVariant.EnumVariant<"Rect", Rect>
    | enumVariant.EnumVariant<"BrickColor", BrickColor>
    | enumVariant.EnumVariant<"CFrame", CFrame>
    | enumVariant.EnumVariant<"Float32", number>
    | enumVariant.EnumVariant<"Int32", number>
    | enumVariant.EnumVariant<"ContentId", string>
    | enumVariant.EnumVariant<"BinaryString", buffer | string>
    | enumVariant.EnumVariant<"Color3uint8", Color3>
    | enumVariant.EnumVariant<"Vector2int16", Vector2int16>
    | enumVariant.EnumVariant<"Vector3int16", Vector3int16>
    | enumVariant.EnumVariant<"Ray", Ray>
    | enumVariant.EnumVariant<"Region3", Region3>
    | enumVariant.EnumVariant<"Region3int16", Region3int16>
    | enumVariant.EnumVariant<"Axes", Axes>
    | enumVariant.EnumVariant<"Faces", Faces>
    | enumVariant.EnumVariant<"Font", Font>
    | enumVariant.EnumVariant<"NumberSequence", NumberSequence>
    | enumVariant.EnumVariant<"ColorSequence", ColorSequence>
    | enumVariant.EnumVariant<"PhysicalProperties", PhysicalProperties?>
    | enumVariant.EnumVariant<"OptionalCFrame", CFrame?>
    | enumVariant.EnumVariant<"Content", Content | ContentValue>

export type ContentValue =
    | { type: "None", content: nil }
    | { type: "Uri", content: string? }
    | { type: "Object", content: DomId }
export type DomInstance = {
    id: DomId,
    parent: DomId?,
    class: string,
    name: string,
    hasChildren: boolean,
    properties: { [string]: DomValue },
    attributes: { [string]: DomValue },
    tags: { string }?,
}
export type TagChange =
    | enumVariant.EnumVariant<"None">
    | enumVariant.EnumVariant<"Replace", { string }>
    | enumVariant.EnumVariant<"Delta", { add: { string }, remove: { string } }>

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

export type Operation =
    | enumVariant.EnumVariant<"Rename", { node: DomId, name: string }>
    | enumVariant.EnumVariant<"Delete", { node: DomId }>
    | enumVariant.EnumVariant<"Move", { node: DomId, parent: DomId? }>
    | enumVariant.EnumVariant<"Create", { parent: DomId, class: string }>
    | enumVariant.EnumVariant<"AddTag", { node: DomId, tag: string }>
    | enumVariant.EnumVariant<"RemoveTag", { node: DomId, tag: string }>
    | enumVariant.EnumVariant<"SetProperty", { node: DomId, name: string, value: DomValue }>
    | enumVariant.EnumVariant<"SetAttribute", { node: DomId, name: string, value: DomValue }>
    | enumVariant.EnumVariant<"RemoveAttribute", { node: DomId, name: string }>
    | enumVariant.EnumVariant<"RenameAttribute", { node: DomId, old: string, new: string }>
    | enumVariant.EnumVariant<"GetProperties", { node: DomId, properties: { string } }>
    | enumVariant.EnumVariant<"RunCode", { source: string }>

export type NamedValue = { name: string, value: DomValue }
export type Reads = { properties: { NamedValue }, tags: { string }, attributes: { NamedValue } }
export type OpResult =
    | enumVariant.EnumVariant<"Ok">
    | enumVariant.EnumVariant<"Reads", Reads>
    | enumVariant.EnumVariant<"Err", string>
    | enumVariant.EnumVariant<"Output", string>

export type LogLevel = "print" | "info" | "warn" | "error"
export type LogEntry = { level: LogLevel, content: string }

--- Which way a captured remote call crossed the network boundary.
export type RemoteDirection = "Outgoing" | "Incoming"
--- A remote (or an instance an argument referenced); `id` shares the `DomSync` id scheme.
export type InstanceRef = { id: DomId, class: string, name: string, path: string }
export type CallSource = {
    script: string?,
    functionName: string?,
    chunk: string?,
    line: number?,
    isExecutor: boolean,
    actor: string?,
}
export type FunctionRef = { name: string?, chunk: string?, line: number? }
export type LuaEntry = { key: LuaValue, value: LuaValue }
export type LuaTable = {
    id: number,
    array: { LuaValue },
    entries: { LuaEntry },
    truncated: boolean,
    metatable: boolean,
}
--- One captured argument; Roblox datatypes ride the existing [`DomValue`].
export type LuaValue =
    | enumVariant.EnumVariant<"Nil">
    | enumVariant.EnumVariant<"Bool", boolean>
    | enumVariant.EnumVariant<"Number", number>
    | enumVariant.EnumVariant<"String", buffer | string>
    | enumVariant.EnumVariant<"Table", LuaTable>
    | enumVariant.EnumVariant<"Instance", InstanceRef>
    | enumVariant.EnumVariant<"Roblox", DomValue>
    | enumVariant.EnumVariant<"Buffer", buffer | string>
    | enumVariant.EnumVariant<"Function", FunctionRef>
    | enumVariant.EnumVariant<"Cycle", number>
    | enumVariant.EnumVariant<"Opaque", string>

export type RemoteCall = {
    id: number,
    direction: RemoteDirection,
    remote: InstanceRef,
    method: string,
    arguments: { LuaValue },
    returns: { LuaValue }?,
    source: CallSource,
    timestamp: number,
}

--- What the broker asks this client's spy to capture.
export type SpyConfig = {
    enabled: boolean,
    outgoing: boolean,
    incoming: boolean,
    bindables: boolean,
    maxDepth: number,
    maxEntries: number,
    maxBytes: number,
}

export type EnumEntry = { name: string, value: number }
export type EnumFamily = { name: string, items: { EnumEntry } }

export type OperationRequest = { id: number, op: Operation }
export type OperationResponse = { id: number, result: OpResult }
export type ClientMessage =
    | enumVariant.EnumVariant<"Shutdown">
    | enumVariant.EnumVariant<"SetUpstream", UpstreamState>
    | enumVariant.EnumVariant<"UpdateDom", DomPatch>
    | enumVariant.EnumVariant<"EnumFamilies", { EnumFamily }>
    | enumVariant.EnumVariant<"OperationResult", OperationResponse>
    | enumVariant.EnumVariant<"RequestActive">
    | enumVariant.EnumVariant<"Log", { LogEntry }>
    | enumVariant.EnumVariant<"RemoteCalls", { RemoteCall }>
export type SearchQuery = { from: DomId, query: string }
export type ServerMessage =
    | enumVariant.EnumVariant<"Hello">
    | enumVariant.EnumVariant<"UpstreamChanged", UpstreamState>
    | enumVariant.EnumVariant<"RequestChildren", DomId?>
    | enumVariant.EnumVariant<"RequestNode", DomId>
    | enumVariant.EnumVariant<"RequestSnapshot", DomId?>
    | enumVariant.EnumVariant<"Search", SearchQuery>
    | enumVariant.EnumVariant<"RequestNodes", { DomId }>
    | enumVariant.EnumVariant<"Operation", OperationRequest>
    | enumVariant.EnumVariant<"Spy", SpyConfig>

return true
