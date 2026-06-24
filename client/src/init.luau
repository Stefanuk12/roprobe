-- Dependencies
local LuauLSP = require("@modules/luau_lsp")
local Verde = require("@modules/verde")
local globalTrove = require("@core/globalTrove")

-- Vars
local config = {
    host = "localhost",
    verde = true,
    luauLsp = false,
}

-- Load Verde
if config.verde then
    local verde = globalTrove:Add(Verde.new(`ws://{config.host}:9000`))
    verde:Start()
end

-- Load the Luau Language Server companion
-- NOTE: this does not use lazy loading and can take a while on some games, recommended to disable for now.
if config.luauLsp then
    local luauLsp = globalTrove:Add(LuauLSP.new({
        host = config.host,
    }))
    luauLsp:Start()
end
