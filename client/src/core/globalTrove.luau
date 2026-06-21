local existing = getgenv().roprobe
if existing then
    -- A prior (possibly incompatible) injection may have left `roprobe` set to a
    -- value without a `Destroy` method, so guard the teardown to avoid aborting
    -- bootstrap before Verde/LuauLSP load.
    pcall(function()
        existing:Destroy()
    end)
end

local globalTrove = require("@pkgs/trove").new()
getgenv().roprobe = globalTrove
return globalTrove