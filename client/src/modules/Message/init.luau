-- Dependencies
local EnumBuilder = require("@self/EnumBuilder")
local Squash = require("@pkgs/squash")

-- Types
type EnumItem<A, B> = EnumBuilder.EnumItem<A, B>
export type WebsocketMessage = | EnumItem<"1", string> | EnumItem<"1", number>

-- Create the enum
local message = EnumBuilder.new():variant(Squash.string):variant(Squash.number):build()

message.ser(1 :: any, {
    type = 1,
    data = 2,
})
return message
