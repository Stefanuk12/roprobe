-- Dependencies
local Squash = require("@pkgs/squash")
local sanitizeUtf8 = require("@lib/sanitizeUtf8")
local serdeArray = require("@lib/serdeArray")

-- Scalars
local rawStr = Squash.string()
local str: Squash.SerDes<string> = {
    ser = function(cursor, value)
        rawStr.ser(cursor, sanitizeUtf8(value))
    end,
    des = rawStr.des,
}
local boolean = Squash.boolean() :: Squash.SerDes<boolean>
local f32 = Squash.f32() :: Squash.SerDes<number>
local f64 = Squash.f64()
local i32 = Squash.i32() :: Squash.SerDes<number>
local i64 = Squash.i64()
local u8 = Squash.u8() :: Squash.SerDes<number>
local u16 = Squash.u16() :: Squash.SerDes<number>
local u32 = Squash.u32() :: Squash.SerDes<number>
local rawBuffer = Squash.buffer()
local optStr = Squash.opt(str)
local vlq = Squash.vlq()
local strArray = serdeArray(str)

return {
    str = str,
    boolean = boolean,
    f32 = f32,
    f64 = f64,
    i32 = i32,
    i64 = i64,
    u8 = u8,
    u16 = u16,
    u32 = u32,
    rawBuffer = rawBuffer,
    optStr = optStr,
    vlq = vlq,
    strArray = strArray,
}
