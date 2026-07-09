-- Dependencies
local Broker = require("@contracts/Broker")
local Squash = require("@pkgs/squash")
local primitives = require("@lib/Messages/primitives")
local serdeArray = require("@lib/serdeArray")
local squashEnum = require("@lib/squashEnum")

-- Primitives
local boolean = primitives.boolean
local f32 = primitives.f32
local f64 = primitives.f64
local i32 = primitives.i32
local i64 = primitives.i64
local optStr = primitives.optStr
local rawBuffer = primitives.rawBuffer
local str = primitives.str
local u16 = primitives.u16
local u32 = primitives.u32
local u8 = primitives.u8

-- Types
type ContentValue = Broker.ContentValue
type DomValue = Broker.DomValue

local NO_FAMILY = 0xffff
local enumFamilies = Enum:GetEnums()
local familyIndexByType: { [Enum]: number } = {}
local familyNames: { string } = {}
for index, family in enumFamilies do
    familyIndexByType[family] = index - 1
    familyNames[index] = tostring(family)
end

--- Mirrors `Enum(familyIndex, value)`
local enumValue: Squash.SerDes<EnumItem | number> = {
    ser = function(cursor, value)
        if typeof(value) == "EnumItem" then
            local enumItem = value :: EnumItem
            u32.ser(cursor, enumItem.Value)
            u16.ser(cursor, familyIndexByType[enumItem.EnumType] :: number)
        else
            u32.ser(cursor, value :: number)
            u16.ser(cursor, NO_FAMILY)
        end
    end,

    des = function(cursor)
        local index = u16.des(cursor)
        local raw = u32.des(cursor)
        if index == NO_FAMILY then
            return raw
        end
        return (enumFamilies[index + 1] :: any):FromValue(raw)
    end,
}

local vector2Value = Squash.Vector2(f32)
local vector3Value = Squash.Vector3(f32)
local color3Value = Squash.Color3()
local udimValue = Squash.UDim(f32)
local udim2Value = Squash.UDim2(f32)
local numberRangeValue = Squash.NumberRange(f32)
local rectValue = Squash.Rect(f32)
local brickColorValue = Squash.BrickColor()
local cframeValue: Squash.SerDes<CFrame> = {
    ser = function(cursor, value)
        local components = { value:GetComponents() }
        for index = 12, 1, -1 do
            f32.ser(cursor, components[index])
        end
    end,

    des = function(cursor)
        local components = table.create(12)
        for index = 1, 12 do
            components[index] = f32.des(cursor)
        end
        return (CFrame.new :: any)(table.unpack(components)) :: CFrame
    end,
}
local binaryStringValue: Squash.SerDes<buffer | string> = {
    ser = function(cursor, value)
        rawBuffer.ser(cursor, if type(value) == "string" then buffer.fromstring(value) else value)
    end,

    des = function(cursor)
        return rawBuffer.des(cursor)
    end,
}

local contentIdValue = str
local color3uint8Value = Squash.Color3()
local vector2int16Value = Squash.Vector2int16()
local vector3int16Value = Squash.Vector3int16()
local rayValue = Squash.Ray(f32)
local region3Value = Squash.Region3(f32)
local region3int16Value = Squash.Region3int16()
local axesValue = Squash.Axes()
local facesValue = Squash.Faces()
local fontValue: Squash.SerDes<Font> = {
    ser = function(cursor, value)
        u8.ser(cursor, value.Style.Value)
        u16.ser(cursor, value.Weight.Value)
        str.ser(cursor, value.Family)
    end,

    des = function(cursor)
        local family = str.des(cursor)
        local weight = u16.des(cursor)
        local style = u8.des(cursor)
        return Font.new(
            family,
            (Enum.FontWeight:FromValue(weight) :: Enum.FontWeight?) or Enum.FontWeight.Regular,
            (Enum.FontStyle:FromValue(style) :: Enum.FontStyle?) or Enum.FontStyle.Normal
        )
    end,
}
local numberKeypointValue = Squash.NumberSequenceKeypoint(f32)
local numberKeypoints = serdeArray(numberKeypointValue)
local numberSequenceValue: Squash.SerDes<NumberSequence> = {
    ser = function(cursor, value)
        numberKeypoints.ser(cursor, value.Keypoints)
    end,

    des = function(cursor)
        local keypoints = numberKeypoints.des(cursor)
        return (NumberSequence.new :: ({ NumberSequenceKeypoint }) -> NumberSequence)(keypoints)
    end,
}
local colorKeypoints = serdeArray(Squash.ColorSequenceKeypoint())
local colorSequenceValue: Squash.SerDes<ColorSequence> = {
    ser = function(cursor, value)
        colorKeypoints.ser(cursor, value.Keypoints)
    end,

    des = function(cursor)
        local keypoints = colorKeypoints.des(cursor)
        return (ColorSequence.new :: ({ ColorSequenceKeypoint }) -> ColorSequence)(keypoints)
    end,
}
local physPropsValue: Squash.SerDes<PhysicalProperties> = {
    ser = function(cursor, props)
        f32.ser(cursor, props.AcousticAbsorption)
        f32.ser(cursor, props.ElasticityWeight)
        f32.ser(cursor, props.FrictionWeight)
        f32.ser(cursor, props.Elasticity)
        f32.ser(cursor, props.Friction)
        f32.ser(cursor, props.Density)
    end,

    des = function(cursor)
        local density = f32.des(cursor)
        local friction = f32.des(cursor)
        local elasticity = f32.des(cursor)
        local frictionWeight = f32.des(cursor)
        local elasticityWeight = f32.des(cursor)
        local acousticAbsorption = f32.des(cursor)
        return PhysicalProperties.new(
            density,
            friction,
            elasticity,
            frictionWeight,
            elasticityWeight,
            acousticAbsorption
        )
    end,
}
local physicalPropertiesValue = Squash.opt(physPropsValue)
local optionalCFrameValue = Squash.opt(cframeValue)
local contentValue: Squash.SerDes<any> = {
    ser = function(cursor, value)
        if typeof(value) == "Content" then
            local content = value :: Content
            if content.SourceType == Enum.ContentSourceType.None then
                u8.ser(cursor, 0)
            elseif content.SourceType == Enum.ContentSourceType.Uri then
                optStr.ser(cursor, content.Uri)
                u8.ser(cursor, 1)
            else
                error("Content object references must be resolved to mirrored ids via DomSync")
            end
            return
        end

        local variant = value :: ContentValue
        if variant.type == "None" then
            u8.ser(cursor, 0)
        elseif variant.type == "Uri" then
            optStr.ser(cursor, variant.content)
            u8.ser(cursor, 1)
        elseif variant.type == "Object" then
            str.ser(cursor, variant.content)
            u8.ser(cursor, 2)
        else
            error(`unknown ContentValue type {tostring((variant :: any).type)}`)
        end
    end,

    des = function(cursor)
        local tag = u8.des(cursor)
        if tag == 0 then
            return { type = "None" }
        elseif tag == 1 then
            return { type = "Uri", content = optStr.des(cursor) }
        elseif tag == 2 then
            return { type = "Object", content = str.des(cursor) }
        end
        error(`unknown ContentValue tag {tag}`)
    end,
}
local domValue: Squash.SerDes<DomValue> = squashEnum({
    { type = "Bool" :: "Bool", content = Squash.T(boolean) },
    { type = "Float" :: "Float", content = Squash.T(f64) },
    { type = "Int" :: "Int", content = Squash.T(i64) },
    { type = "String" :: "String", content = Squash.T(str) },
    { type = "Ref" :: "Ref", content = Squash.T(str) },
    { type = "Enum" :: "Enum", content = Squash.T(enumValue) },
    { type = "Vector2" :: "Vector2", content = Squash.T(vector2Value) },
    { type = "Vector3" :: "Vector3", content = Squash.T(vector3Value) },
    { type = "Color3" :: "Color3", content = Squash.T(color3Value) },
    { type = "UDim" :: "UDim", content = Squash.T(udimValue) },
    { type = "UDim2" :: "UDim2", content = Squash.T(udim2Value) },
    { type = "NumberRange" :: "NumberRange", content = Squash.T(numberRangeValue) },
    { type = "Rect" :: "Rect", content = Squash.T(rectValue) },
    { type = "BrickColor" :: "BrickColor", content = Squash.T(brickColorValue) },
    { type = "CFrame" :: "CFrame", content = Squash.T(cframeValue) },
    { type = "Float32" :: "Float32", content = Squash.T(f32) },
    { type = "Int32" :: "Int32", content = Squash.T(i32) },
    { type = "ContentId" :: "ContentId", content = Squash.T(contentIdValue) },
    { type = "BinaryString" :: "BinaryString", content = Squash.T(binaryStringValue) },
    { type = "Color3uint8" :: "Color3uint8", content = Squash.T(color3uint8Value) },
    { type = "Vector2int16" :: "Vector2int16", content = Squash.T(vector2int16Value) },
    { type = "Vector3int16" :: "Vector3int16", content = Squash.T(vector3int16Value) },
    { type = "Ray" :: "Ray", content = Squash.T(rayValue) },
    { type = "Region3" :: "Region3", content = Squash.T(region3Value) },
    { type = "Region3int16" :: "Region3int16", content = Squash.T(region3int16Value) },
    { type = "Axes" :: "Axes", content = Squash.T(axesValue) },
    { type = "Faces" :: "Faces", content = Squash.T(facesValue) },
    { type = "Font" :: "Font", content = Squash.T(fontValue) },
    { type = "NumberSequence" :: "NumberSequence", content = Squash.T(numberSequenceValue) },
    { type = "ColorSequence" :: "ColorSequence", content = Squash.T(colorSequenceValue) },
    { type = "PhysicalProperties" :: "PhysicalProperties", content = Squash.T(physicalPropertiesValue) },
    { type = "OptionalCFrame" :: "OptionalCFrame", content = Squash.T(optionalCFrameValue) },
    { type = "Content" :: "Content", content = Squash.T(contentValue) },
}) :: any

return {
    domValue = domValue,
    familyNames = familyNames,
}
