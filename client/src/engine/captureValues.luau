-- Dependencies
-- `@lib/Messages/tags` rather than the `Messages` barrel: the barrel drags in the
-- whole Squash codec, which the actor bundle would carry for nothing.
local Broker = require("@contracts/Broker")
local sourceInfo = require("@lib/sourceInfo")
local tags = require("@lib/Messages/tags")

-- Types
type InstanceRef = Broker.InstanceRef
type LuaEntry = Broker.LuaEntry
type LuaTable = Broker.LuaTable
type LuaValue = Broker.LuaValue

--- How much of a value to walk before giving up, and how to name the instances in it.
export type CaptureOptions = {
    maxDepth: number,
    maxEntries: number,
    maxBytes: number,
    --- The mirrored id an instance is known by, or `""` when it has none.
    resolveId: (Instance) -> string,
}

-- One pass over one call's arguments; `seen` maps a table to the id a `Cycle`
-- back-reference points at, and `nodes` is what the whole call has spent so far.
type Pass = {
    options: CaptureOptions,
    seen: { [any]: number },
    nextTableId: number,
    nodes: number,
}

--- A whole call's table budget. `maxEntries` bounds one table, but nesting
--- multiplies it, and the walk runs on the game's thread before its call goes out.
local MAX_NODES = 4096

local NIL: LuaValue = { type = "Nil" :: "Nil", content = nil }
--- A value whose own capture threw, which a game-controlled `__index` can cause.
local UNCAPTURABLE: LuaValue = { type = "Opaque" :: "Opaque", content = "<uncapturable>" }

--- Describe an instance well enough to name and cross-link it.
local function captureInstance(instance: Instance, resolveId: (Instance) -> string): InstanceRef
    local okClass, class = pcall(function()
        return instance.ClassName
    end)
    local okName, name = pcall(function()
        return instance.Name
    end)
    local okPath, path = pcall(function()
        return instance:GetFullName()
    end)

    local resolved = if okName then name else "?"
    local okId, id = pcall(resolveId, instance)
    return {
        id = if okId then id else "",
        class = if okClass then class else "?",
        name = resolved,
        path = if okPath then path else resolved,
    }
end

--- Describe a function by where it was defined, since the closure cannot travel.
local function captureFunction(value: (...any) -> ...any): LuaValue
    return { type = "Function" :: "Function", content = sourceInfo(value) }
end

local capture: (pass: Pass, value: any, depth: number) -> LuaValue

--- Walk a game-controlled table rawly: a metatable on it could otherwise turn
--- reading an argument into running the game's code on our thread.
local function captureTable(pass: Pass, value: any, depth: number): LuaValue
    local existing = pass.seen[value]
    if existing then
        return { type = "Cycle" :: "Cycle", content = existing }
    end

    local hasMetatable = false
    if getrawmetatable then
        local ok, metatable = pcall(getrawmetatable, value)
        hasMetatable = ok and metatable ~= nil
    end

    local id = pass.nextTableId
    pass.nextTableId += 1

    -- Past the depth cap the shape is still worth reporting, just not its
    -- contents. Left out of `seen`: a later, shallower sighting would otherwise
    -- collapse into a `Cycle` pointing at this empty shell.
    if depth >= pass.options.maxDepth then
        return {
            type = "Table" :: "Table",
            content = {
                id = id,
                array = {},
                entries = {},
                truncated = true,
                metatable = hasMetatable,
            },
        }
    end
    pass.seen[value] = id

    local captured: LuaTable = {
        id = id,
        array = {},
        entries = {},
        truncated = false,
        metatable = hasMetatable,
    }

    local budget = math.min(pass.options.maxEntries, math.max(MAX_NODES - pass.nodes, 0))
    local length = rawlen(value)
    for index = 1, math.min(length, budget) do
        captured.array[index] = capture(pass, rawget(value, index), depth + 1)
    end

    local taken = #captured.array
    pass.nodes += taken
    if length > taken then
        captured.truncated = true
    end

    -- Already known to be short, so the hash walk is skipped: `next` steps over
    -- every array slot to reach the hash part, inside a metamethod on the hot path.
    -- A table whose array part merely filled the budget exactly still has to walk,
    -- or a dropped hash part would be reported as a complete table.
    if taken >= budget and captured.truncated then
        return { type = "Table" :: "Table", content = captured }
    end

    -- `next` rather than a generic `for`, which an `__iter` metamethod would hijack.
    local key = next(value)
    while key ~= nil do
        -- The array part is already captured, so skip the keys it covered.
        local isArrayIndex = type(key) == "number" and key >= 1 and key <= length and key % 1 == 0
        if not isArrayIndex then
            if taken >= budget then
                captured.truncated = true
                break
            end

            taken += 1
            pass.nodes += 1
            table.insert(
                captured.entries,
                {
                    key = capture(pass, key, depth + 1),
                    value = capture(pass, rawget(value, key), depth + 1),
                } :: LuaEntry
            )
        end

        key = next(value, key)
    end

    return { type = "Table" :: "Table", content = captured }
end

--- Convert one live Luau value into its wire shape, never yielding and never
--- running the game's code.
function capture(pass: Pass, value: any, depth: number): LuaValue
    local kind = typeof(value)

    if value == nil then
        return NIL
    elseif kind == "boolean" then
        return { type = "Bool" :: "Bool", content = value }
    elseif kind == "number" then
        return { type = "Number" :: "Number", content = value }
    elseif kind == "string" then
        local text = value :: string
        return { type = "String" :: "String", content = string.sub(text, 1, pass.options.maxBytes) }
    elseif kind == "buffer" then
        local blob = value :: buffer
        local size = math.min(buffer.len(blob), pass.options.maxBytes)
        local truncated = buffer.create(size)
        buffer.copy(truncated, 0, blob, 0, size)
        return { type = "Buffer" :: "Buffer", content = truncated }
    elseif kind == "Instance" then
        return { type = "Instance" :: "Instance", content = captureInstance(value, pass.options.resolveId) }
    elseif kind == "table" then
        return captureTable(pass, value, depth)
    elseif kind == "function" then
        return captureFunction(value)
    end

    -- Everything Roblox already has a wire shape for rides the DOM's value codec.
    if tags.tagOf(value) then
        -- Except a `Content` naming an object: that codec resolves one through
        -- DomSync and throws otherwise, and the throw lands at encode time, where
        -- it costs the whole batch rather than this one value.
        if kind == "Content" then
            local source = (value :: Content).SourceType
            if source ~= Enum.ContentSourceType.None and source ~= Enum.ContentSourceType.Uri then
                return { type = "Opaque" :: "Opaque", content = kind }
            end
        end

        return { type = "Roblox" :: "Roblox", content = tags.value(value) }
    end

    return { type = "Opaque" :: "Opaque", content = kind }
end

--- Capture a call's worth of values, sharing one cycle table across them so a
--- table passed twice is sent once and back-referenced after.
local function captureValues(values: { any }, count: number, options: CaptureOptions): { LuaValue }
    local pass: Pass = { options = options, seen = {}, nextTableId = 0, nodes = 0 }

    local captured = table.create(count) :: { LuaValue }
    for index = 1, count do
        local ok, result = pcall(capture, pass, values[index], 0)
        captured[index] = if ok then result :: LuaValue else UNCAPTURABLE
    end
    return captured
end

return captureValues
