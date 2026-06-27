-- Dependencies
local InstanceMap = require("@verde/Shared/InstanceMap")

-- Services
local HttpService = cloneref(game:GetService("HttpService"))

-- Types
type Payload = {
    code: string,
    description: string?,
    nodeId: string?,
    extensionId: string?,
}

--- Sanitises the extension id.
local function sanitiseId(raw: string?): string
    if not raw or raw == "" then
        return "extension"
    end
    local cleaned = (raw:gsub("[%c\r\n]", ""))
    cleaned = (cleaned:gsub("^%s+", ""):gsub("%s+$", ""))
    if cleaned == "" then
        return "extension"
    end
    if #cleaned > 128 then
        cleaned = cleaned:sub(1, 128)
    end
    return cleaned
end

--- Runs a given payload, arbitary Luau code from the bridge.
local function execute_luau(payload: Payload): (any, string?)
    -- Make sure we got some code
    if type(payload.code) ~= "string" or payload.code == "" then
        return false, "execute_luau: missing code"
    end

    local extensionId = sanitiseId(payload.extensionId)

    -- Find the instance in Verde's instance map
    local selection: Instance? = nil
    if payload.nodeId then
        selection = InstanceMap[payload.nodeId]
        if not selection then
            return false, `execute_luau: instance not found: {payload.nodeId}`
        end
    end

    -- Wrap the payload so we can give it the context it needs
    local wrapped = `return function(ctx)\n{payload.code}\nend`
    local loader, loadError = loadstring(wrapped, `=roprobe: {extensionId}`)
    if not loader then
        return false, `execute_luau: compile failed: {tostring(loadError)}`
    end

    -- Try to run the loader, ensuring it returns a function
    local topOk, fnOrErr = pcall(loader)
    if not topOk then
        return false, `execute_luau: top-level error: {tostring(fnOrErr)}`
    end
    if type(fnOrErr) ~= "function" then
        return false, "execute_luau: wrapper did not return a function"
    end

    -- Run the main function, given the context
    local ctx = {
        selection = selection,
        extensionId = extensionId,
    }

    local userResult: any
    local runOk, runErr = pcall(function()
        userResult = fnOrErr(ctx)
    end)

    -- Make sure it ran successfully
    if not runOk then
        return false, `execute_luau: script error: {tostring(runErr)}`
    end

    -- Return the results in a JSON-serialised way.
    -- NOTE: JSON serialiased for security reasons.
    if userResult ~= nil then
        local encodeOk, encodeErr = pcall(function()
            HttpService:JSONEncode(userResult)
        end)
        if not encodeOk then
            return false, `execute_luau: return value not JSON-serialisable: {tostring(encodeErr)}`
        end
    end

    return { value = userResult }
end
return execute_luau
