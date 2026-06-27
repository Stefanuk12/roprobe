--- Makes every property of `object` optional.
export type function Partial(object)
    local result = types.newtable()
    for key, property in object:properties() do
        result:setproperty(key, types.unionof(property.read, types.singleton(nil)))
    end
    return result
end

return nil
