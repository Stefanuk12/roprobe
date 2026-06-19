-- Dependencies
local Squash = require("@pkgs/squash")

--- Build an enum object.
local EnumBuilder = {}
EnumBuilder.__index = EnumBuilder

-- Types
export type EnumItem<Type, Data> = { type: Type, data: Data }
export type EnumBuilderData = {
    _variants: { [number]: Squash.SerDes<()> }
}
export type EnumBuilder = setmetatable<EnumBuilderData, typeof(EnumBuilder)>

--- Create a new [`EnumBuilder`] object.
function EnumBuilder.new()
    local self: EnumBuilderData = {
        _variants = {},
    }
    return setmetatable(self, EnumBuilder)
end

--- Add a new variant type.
function EnumBuilder.variant<T>(self: EnumBuilder, schema: T & {})
    table.insert(self._variants, schema)
    return self
end

--- Build the ser/de object.
function EnumBuilder.build(self: EnumBuilder): Squash.SerDes<number>
    local variants = self._variants
    local one = Squash.uint(1)

    return {
        ser = function<T>(cursor: Squash.Cursor, enum: EnumItem<number, T>)
            local variantNumber = enum.type
            local variant = variants[variantNumber]
            variant.ser(cursor, enum.data)
            one.ser(cursor, variantNumber - 1)
        end,
        des = function(cursor: Squash.Cursor)
            local variantNumber = one.des(cursor) + 1
            local variant = variants[variantNumber]
            return {
                type = variantNumber,
                data = variant.des(cursor),
            }
        end,
    }
end

return EnumBuilder
