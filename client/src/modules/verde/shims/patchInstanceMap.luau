-- Dependencies
local InstanceMap = require("@verde/Shared/InstanceMap")

--- Shim for Verde's `InstanceMap`.
---
--- Calls `cloneref` on each new addition to avoid detections.
local applied = false

--- Attach the clone-ref metatable to Verde's `InstanceMap`. Idempotent, and
--- must run before Verde starts populating the map.
local function patchInstanceMap()
    if applied then
        return
    end
    applied = true

    setmetatable(InstanceMap :: any, {
        __newindex = function(map, key, value)
            if typeof(value) == "Instance" then
                value = cloneref(value)
            end
            -- `rawset` so we don't re-enter this handler on the absent key.
            rawset(map :: any, key, value)
        end,
    })
end

return patchInstanceMap
