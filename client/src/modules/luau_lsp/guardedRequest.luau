-- Dependencies
local types = require("@luau_lsp/types")

-- Types
type Result<T, E> = types.Result<T, E>
type HttpResponse = types.HttpResponse

--- Extract the first parameter type of a function type.
type function FirstParameter(fn)
    return fn:parameters().head[1]
end

--- Perform an HTTP request through the executor and wrap it into a `Result`.
local function guardedRequest(options: FirstParameter<typeof(request)>): Result<HttpResponse, string>
    local ok, response = pcall(request, options)
    if not ok then
        return { ok = false, error = tostring(response) }
    end

    if type(response) ~= "table" then
        return { ok = false, error = `unexpected response: {tostring(response)}` }
    end
    if not response.Success then
        return { ok = false, error = tostring(response.StatusCode) .. ": " .. tostring(response.Body) }
    end

    return { ok = true, value = response }
end

return guardedRequest
