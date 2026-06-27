local existing = getgenv().roprobe
if existing then
    existing:Destroy()
end

local globalTrove = require("@pkgs/trove").new()
-- selene: allow(unscoped_variables)
getgenv().roprobe = globalTrove
return globalTrove
