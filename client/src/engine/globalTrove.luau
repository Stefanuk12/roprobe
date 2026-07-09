-- Dependencies
local Trove = require("@pkgs/trove")

-- Destroy any existing instances
local existing = getgenv().roprobe
if existing then
    existing:Destroy()
end

-- Create and return a new one
local globalTrove = Trove.new()
getgenv().roprobe = globalTrove
return globalTrove
