-- Dependencies
local Trove = require("@pkgs/trove")

-- Held as a local: writing through `getgenv()` in place reads as an assignment
-- to an undeclared global to selene.
local env = getgenv()

-- Destroy any existing instances
local existing = env.roprobe
if existing then
    existing:Destroy()
end

-- Create and return a new one
local globalTrove = Trove.new()
env.roprobe = globalTrove
return globalTrove
