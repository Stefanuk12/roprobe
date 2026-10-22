-- Dependencies
local Broker = require("@contracts/Broker")
local captureValues = require("@engine/captureValues")

-- Types
type ClientMessage = Broker.ClientMessage

-- Constants
local MAX_MATCHES = 200
local MAX_DEPTH = 8
local CAPTURE = { maxDepth = 3, maxEntries = 8, maxBytes = 256 }

--- Hunts string constants across every script's closures.
local Scan = {}

--- Record each string constant in `fn` (and its protos) containing `pattern`.
local function scanClosure(fn: (...any) -> ...any, label: string, pattern: string, hits: { any }, depth: number)
    local ok, constants = pcall(getconstants, fn)
    if ok then
        for index, value in constants do
            if type(value) == "string" and string.find(value, pattern, 1, true) then
                table.insert(hits, { script = label, index = index, value = value })
            end
        end
    end

    if depth >= MAX_DEPTH then
        return
    end

    local protoOk, protos = pcall(getprotos, fn)
    if not protoOk then
        return
    end
    for _, proto in protos do
        if #hits >= MAX_MATCHES then
            return
        end
        scanClosure(proto, label, pattern, hits, depth + 1)
    end
end

--- Every plain-text hit of `pattern`, one table per hit, answered as a `ClosureInfo`.
function Scan.Constants(request: number, pattern: string, resolveId: (Instance) -> string): ClientMessage
    local hits: { any } = {}
    for _, script in getscripts() do
        if #hits >= MAX_MATCHES then
            break
        end

        local ok, main = pcall(getscriptclosure, script :: any)
        if ok then
            scanClosure(main, script:GetFullName(), pattern, hits, 0)
        end
    end

    local matches = {}
    for _, hit in hits do
        local captured = captureValues({ hit }, 1, {
            maxDepth = CAPTURE.maxDepth,
            maxEntries = CAPTURE.maxEntries,
            maxBytes = CAPTURE.maxBytes,
            resolveId = resolveId,
        })
        table.insert(matches, captured[1])
    end

    return {
        type = "ClosureInfo" :: "ClosureInfo",
        content = {
            request = request,
            name = "scan",
            constants = matches,
            upvalues = {},
            protos = {},
            source = "",
            nparams = 0,
        },
    }
end

return Scan
