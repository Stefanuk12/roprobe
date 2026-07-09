-- Dependencies
local Broker = require("@contracts/Broker")

-- Types
type DomValue = Broker.DomValue

local Tags = {}

--- The `typeof` -> `DomValue.type` mapping for wrappable raw values.
local VALUE_TAGS: { [string]: string } = {
    boolean = "Bool",
    number = "Float",
    string = "String",
    buffer = "BinaryString",
    EnumItem = "Enum",
    Vector2 = "Vector2",
    Vector3 = "Vector3",
    Color3 = "Color3",
    UDim = "UDim",
    UDim2 = "UDim2",
    NumberRange = "NumberRange",
    Rect = "Rect",
    BrickColor = "BrickColor",
    CFrame = "CFrame",
    Vector2int16 = "Vector2int16",
    Vector3int16 = "Vector3int16",
    Ray = "Ray",
    Region3 = "Region3",
    Region3int16 = "Region3int16",
    Axes = "Axes",
    Faces = "Faces",
    Font = "Font",
    NumberSequence = "NumberSequence",
    ColorSequence = "ColorSequence",
    PhysicalProperties = "PhysicalProperties",
    Content = "Content",
}

--- The `DomValue.type` -> expected `typeof(content)` mapping.
--- Exceptions: `Enum` also takes a raw number;
-- `Int`/`Float32`/`Int32` share `number`;
--- `BinaryString` also takes a string;
--- `PhysicalProperties`/`OptionalCFrame` take nil (None);
--- `Content` takes a [`ContentValue`] table.
local CONTENT_TYPEOF: { [string]: string } = {
    Bool = "boolean",
    Float = "number",
    Int = "number",
    String = "string",
    Ref = "string",
    Enum = "EnumItem",
    Vector2 = "Vector2",
    Vector3 = "Vector3",
    Color3 = "Color3",
    UDim = "UDim",
    UDim2 = "UDim2",
    NumberRange = "NumberRange",
    Rect = "Rect",
    BrickColor = "BrickColor",
    CFrame = "CFrame",
    Float32 = "number",
    Int32 = "number",
    ContentId = "string",
    BinaryString = "buffer",
    Color3uint8 = "Color3",
    Vector2int16 = "Vector2int16",
    Vector3int16 = "Vector3int16",
    Ray = "Ray",
    Region3 = "Region3",
    Region3int16 = "Region3int16",
    Axes = "Axes",
    Faces = "Faces",
    Font = "Font",
    NumberSequence = "NumberSequence",
    ColorSequence = "ColorSequence",
    PhysicalProperties = "PhysicalProperties",
    OptionalCFrame = "CFrame",
    Content = "Content",
}

--- `DomValue.type` tags legal as Roblox *attribute* types.
--- Only `Float` (attributes store doubles); not objects, enums, or width/serialization-internal tags.
local ATTRIBUTE_SAFE_TAGS: { [string]: boolean } = {
    Bool = true,
    Float = true,
    String = true,
    Vector2 = true,
    Vector3 = true,
    Color3 = true,
    UDim = true,
    UDim2 = true,
    NumberRange = true,
    Rect = true,
    BrickColor = true,
    CFrame = true,
    Font = true,
    NumberSequence = true,
    ColorSequence = true,
}

--- The `DomValue.type` tag [`Messages.value`] would use for this raw value, or `nil` if not wrappable.
function Tags.tagOf(value: any): string?
    return VALUE_TAGS[typeof(value)]
end

--- Whether [`Messages.value`] can wrap this raw value as a [`DomValue`].
function Tags.wrappable(value: any): boolean
    return VALUE_TAGS[typeof(value)] ~= nil
end

--- Whether a `DomValue.type` tag is a legal Roblox *attribute* type.
function Tags.attributeSafeTag(tag: string): boolean
    return ATTRIBUTE_SAFE_TAGS[tag] == true
end

--- Whether `tag` is one of the known `DomValue.type` tags.
function Tags.knownTag(tag: string): boolean
    return CONTENT_TYPEOF[tag] ~= nil
end

--- Whether `content` has the right runtime type for a pre-wrapped `{ type = tag, content }` [`DomValue`], so callers reject a mistyped value before it hits the wire.
--- Unknown tags return `false`.
function Tags.contentMatchesTag(tag: string, content: any): boolean
    local expected = CONTENT_TYPEOF[tag]
    if expected == nil then
        return false
    end
    if tag == "Enum" and typeof(content) == "number" then
        return true
    end
    if tag == "BinaryString" and typeof(content) == "string" then
        return true
    end
    if (tag == "PhysicalProperties" or tag == "OptionalCFrame") and content == nil then
        return true
    end
    if tag == "Content" and typeof(content) == "table" then
        local variant = content :: any
        return variant.type == "None"
            or (variant.type == "Uri" and (variant.content == nil or typeof(variant.content) == "string"))
            or (variant.type == "Object" and typeof(variant.content) == "string")
    end
    return typeof(content) == expected
end

--- Wrap a raw Luau value as a [`DomValue`]; numbers become `Float`.
--- For integers build `{ type = "Int", content = n }` yourself - `Int` must be an integer within +-2^53 or the i64 encoding silently alters it.
function Tags.value(value: any): DomValue
    local tag = VALUE_TAGS[typeof(value)]
    assert(tag ~= nil, `cannot wrap a {typeof(value)} as a DomValue`)
    return { type = tag, content = value } :: any
end

return Tags
