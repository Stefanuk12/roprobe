-- Ensure the URL has a path component.
local function normaliseUrl(url: string): string
    local scheme: string, rest: string = "", url
    local matchedScheme, matchedRest = url:match("^(%a+://)(.*)$")
    if matchedScheme and matchedRest then
        scheme, rest = matchedScheme, matchedRest
    end

    local authority: string = rest:match("^[^/?#]*") or ""
    local tail = rest:sub(#authority + 1)

    if tail == "" then
        return scheme .. authority .. "/"
    elseif tail:sub(1, 1) ~= "/" then
        return scheme .. authority .. "/" .. tail
    end
    return url
end
return normaliseUrl
