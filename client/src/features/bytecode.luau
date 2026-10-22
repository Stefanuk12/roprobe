-- Dependencies
local Broker = require("@contracts/Broker")
local Resolve = require("@engine/Resolve")
local captureValues = require("@engine/captureValues")

-- Types
type ClientMessage = Broker.ClientMessage
type ClosureInfo = Broker.ClosureInfo
type LuaValue = Broker.LuaValue

-- Constants
local CAPTURE = { maxDepth = 4, maxEntries = 128, maxBytes = 4096 }

--- Reads a script's bytecode and a closure's guts for the broker.
local Bytecode = {}

--- A failed request, answered under its id so the asker stops waiting.
local function fail(request: number, reason: string): ClientMessage
    return {
        type = "OperationResult" :: "OperationResult",
        content = { id = request, result = { type = "Err" :: "Err", content = reason } },
    }
end

--- Capture a sparse executor table (constants hold nil gaps) up to its highest index.
local function captureSparse(values: { any }, resolveId: (Instance) -> string): { LuaValue }
    local count = 0
    for key in values do
        if type(key) == "number" and key > count then
            count = key
        end
    end
    return captureValues(values, count, {
        maxDepth = CAPTURE.maxDepth,
        maxEntries = CAPTURE.maxEntries,
        maxBytes = CAPTURE.maxBytes,
        resolveId = resolveId,
    })
end

--- The compiled bytecode of the script at `path`.
function Bytecode.Fetch(request: number, path: string): ClientMessage
    local script = Resolve.Instance(path)
    if not script then
        return fail(request, `script not found: {path}`)
    end

    local ok, bytes = pcall(getscriptbytecode, script :: any)
    if not ok then
        return fail(request, `bytecode failed: {bytes}`)
    end

    return {
        type = "BytecodeData" :: "BytecodeData",
        content = { request = request, scriptPath = path, bytes = bytes },
    }
end

--- Name, constants, upvalues, protos and source of the closure at `path`.
function Bytecode.Inspect(request: number, path: string, resolveId: (Instance) -> string): ClientMessage
    local target = Resolve.Closure(path)
    if not target then
        return fail(request, `closure not found: {path}`)
    end

    local ok, result = pcall(function()
        local info = getinfo(target)

        local protos: { string } = {}
        for index, proto in getprotos(target) do
            local named, protoInfo = pcall(getinfo, proto)
            local label: string? = if named then protoInfo.name else nil
            table.insert(protos, label or `proto_{index}`)
        end

        return {
            request = request,
            name = info.name or "",
            constants = captureSparse(getconstants(target), resolveId),
            upvalues = captureSparse(getupvalues(target), resolveId),
            protos = protos,
            source = info.source or info.short_src or "",
            nparams = info.numparams or 0,
        }
    end)
    if not ok then
        return fail(request, `inspect failed: {result}`)
    end

    return { type = "ClosureInfo" :: "ClosureInfo", content = result :: ClosureInfo }
end

return Bytecode
