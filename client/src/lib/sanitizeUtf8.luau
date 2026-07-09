--- U+FFFD (the replacement character) as raw UTF-8 bytes.
local REPLACEMENT = "\239\191\189"

--- Replace invalid UTF-8 byte sequences with U+FFFD.
local function sanitizeUtf8(value: string): string
    if utf8.len(value) then
        return value
    end

    local out = {}
    local n = 0
    local i = 1

    while i <= #value do
        local length, invalidAt = utf8.len(value, i)
        if length then
            n += 1
            out[n] = value:sub(i)
            break
        end

        assert(invalidAt, "utf8.len reports the first invalid byte's position")

        if invalidAt > i then
            n += 1
            out[n] = value:sub(i, invalidAt - 1)
        end

        n += 1
        out[n] = REPLACEMENT
        i = invalidAt + 1
    end

    return table.concat(out)
end

return sanitizeUtf8
