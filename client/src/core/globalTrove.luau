if getgenv().roprobe then
    getgenv().roprobe:Destroy()
end

local globalTrove = require("@pkgs/trove").new()
getgenv().roprobe = globalTrove
return globalTrove