-- Dependencies
local LuauLSP = require("@modules/luau_lsp")
local Verde = require("@modules/verde")
local globalTrove = require("@core/globalTrove")

-- Vars
local host = "localhost"

-- Load Verde
local verde = globalTrove:Add(Verde.new(`ws://{host}:33663`))
verde:Start()

-- Load the Luau Language Server companion
local luauLsp = globalTrove:Add(LuauLSP.new({
    host = host,
}))
luauLsp:Start()
