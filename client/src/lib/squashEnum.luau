-- Dependencies
local Squash = require("@pkgs/squash")
local enumVariant = require("@contracts/enumVariant")

-- Vars
local u8 = Squash.u8()

-- Types
--- Re-exported from the contract, which names it without dragging Squash along.
export type EnumVariant<T, C = nil> = enumVariant.EnumVariant<T, C>
type VariantSpec = {
    type: any,
    content: Squash.SerDes<any>?,
}

--- A `SerDes` object for tagged unions (enums).
---
--- # Example
---
--- NOTE: You need to cast the type to its literal string value to get type refinements workng properly.
---
--- ```lua
--- local msg = squashEnum({
---   { type = "ping" :: "ping" },
---   { type = "data" :: "data", content = Squash.T(Squash.buffer()) },
--- })
--- ```
---
--- NOTE: Multi-value payloads (Rust-style tuple variants) must go through
--- `squashTuple`, not `Squash.tuple`.
local function squashEnum<V>(variants: { V } & { EnumVariant<any, any> }): Squash.SerDes<V>
    local specs = variants :: { VariantSpec }
    if #specs > 256 then
        error(`enum supports at most 256 variants (the tag is a single byte), got {#specs}`)
    end

    local lookup: { [any]: number } = {}
    for i, variant in specs do
        lookup[variant.type] = i
    end

    return {
        ser = function(cursor, value)
            local variant = value :: any
            local i = lookup[variant.type] or error(`unknown enum variant {tostring(variant.type)}`)
            local contentSerDes = specs[i].content
            if contentSerDes then
                contentSerDes.ser(cursor, variant.content)
            end
            u8.ser(cursor, i - 1)
        end,

        des = function(cursor)
            local i = u8.des(cursor) + 1
            local variant = specs[i] or error(`unknown enum variant tag {i - 1}`)
            return {
                type = variant.type,
                content = if variant.content then variant.content.des(cursor) else nil,
            } :: any
        end,
    }
end

return squashEnum
