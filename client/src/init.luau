-- Dependencies
local globalTrove = require("@core/globalTrove")
local Verde = require("@modules/verde")

-- Load Verde
local verde = globalTrove:Add(Verde.new())
verde:Start()
