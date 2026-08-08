--- The instance's `UniqueId`, or `nil` when the executor cannot read one.
--- Services and fresh instances report an all-zero id, which would collide, so
--- only a genuinely unique one counts.
local function uniqueId(instance: Instance): string?
    if not gethiddenproperty then
        return nil
    end

    local ok, value = pcall(gethiddenproperty, instance, "UniqueId")
    if not ok or value == nil then
        return nil
    end

    local id = tostring(value)
    if id:match("^0*$") then
        return nil
    end
    return id
end

return uniqueId
