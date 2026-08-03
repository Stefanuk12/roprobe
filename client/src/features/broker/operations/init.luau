-- Dependencies
local Broker = require("@contracts/Broker")
local DomSync = require("@features/broker/DomSync")
local DomWatch = require("@features/broker/DomWatch")
local Messages = require("@lib/Messages")

-- Executor environment: `getproperties` (property enumeration) isn't in the
-- typed defs, so reach it through the global table as `any`.
local executor = getgenv() :: any

-- Types
type DomSync = DomSync.DomSync
type DomValue = Broker.DomValue
type DomWatch = DomWatch.DomWatch
type NamedValue = Broker.NamedValue
type OpResult = Broker.OpResult
type Operation = Broker.Operation

--- What every handler needs: the id map (resolve nodes) and the watch (refresh
--- the mirror after a mutation).
export type Context = {
    sync: DomSync,
    watch: DomWatch?,
}

-- Constants
local SKIP_PROPERTIES: { [string]: true } = {
    Source = true,
    LinkedSource = true,
}
local RUN_CHUNK_NAME = "roprobe"

--- Applies relayed DOM operations against the live game and reads it back, with the broker owning all upstream interpretation.
local Operations = {}

-- Results -------------------------------------------------------------------

local OK: OpResult = { type = "Ok" :: "Ok" }

--- A failure result carrying an error code.
local function err(code: string): OpResult
    return { type = "Err" :: "Err", content = code }
end

--- Format the error frame / stack trace properly with all of roprobe's stuff.
local function formatFrame(level: number): string?
    local source: string?, line: number?, name: string? = debug.info(level + 1, "sln")
    if source == nil or source == "[C]" then
        return nil
    end

    local at = source:match('^%[string "(.*)"%]$') or source
    if at == "" then
        at = RUN_CHUNK_NAME
    end

    if name ~= nil and name ~= "" then
        return `Script '{at}', Line {line} - function {name}`
    end
    return `Script '{at}', Line {line}`
end

--- Build an `xpcall` handler reporting the error and the frames above `boundary` the roprobe function that called in. 
local function tracebackTo(boundary: any): (any) -> string
    return function(message: any): string
        local frames: { string } = {}

        -- 1 is this handler, so the throw sits at 2.
        local level = 2
        while true do
            local frame: any = debug.info(level, "f")
            if frame == nil or frame == boundary then
                break
            end

            local formatted = formatFrame(level)
            if formatted then
                table.insert(frames, formatted)
            end
            level += 1
        end

        -- Luau prefixes its own errors with the wrapped chunk name; unwrap it so
        -- the message and the frames name the script the same way.
        local head = (tostring(message):gsub('%[string "(.-)"%]', "%1"))
        if #frames == 0 then
            return head
        end

        local stack = table.concat(frames, "\n")
        return `{head}\nStack Begin\n{stack}\nStack End`
    end
end

-- Value conversion ----------------------------------------------------------

--- A live Roblox value as a generic [`DomValue`]: instances become `Ref`s (by mirror id), enum items `Enum`s, wrappable values their natural tag, and anything else its text.
local function toDomValue(context: Context, value: any): DomValue
    local kind = typeof(value)
    if kind == "Instance" then
        local instance = value :: Instance
        -- Mirror the referenced instance (and its ancestors) so the broker can
        -- resolve the reference to a name/class link rather than a raw id.
        local watch = context.watch
        if watch and instance ~= game then
            watch:SyncNode(instance)
        end
        local id = context.sync:IdOf(instance)
        if id ~= nil then
            return { type = "Ref", content = id }
        end
        return { type = "String", content = instance.Name }
    elseif kind == "EnumItem" then
        return { type = "Enum", content = value }
    elseif kind ~= "Content" and Messages.wrappable(value) then
        return Messages.value(value)
    end

    -- The opaque `User` DataType (Player.User) tostrings as "U1." + unpadded base64 of its raw bytes, which we re-pad and decode, degrading to text otherwise.
    local text = tostring(value)
    local encoded = text:match("^U1%.(.+)$")
    if encoded then
        local padded = encoded .. string.rep("=", (4 - #encoded % 4) % 4)
        local decodeOk, bytes = pcall(crypt.base64decode, padded)
        if decodeOk then
            return { type = "BinaryString", content = bytes }
        end
    end
    return { type = "String", content = text }
end

--- Resolve a scalar (a name or a value) to an `EnumItem` of `enumType`.
local function enumFromScalar(enumType: Enum, scalar: any): EnumItem?
    if type(scalar) == "string" then
        for _, item in enumType:GetEnumItems() do
            if item.Name == scalar then
                return item
            end
        end
    elseif type(scalar) == "number" then
        for _, item in enumType:GetEnumItems() do
            if item.Value == scalar then
                return item
            end
        end
    end
    return nil
end

-- Helpers -------------------------------------------------------------------

--- Resolve a node id to its live instance, or a failure result.
local function resolveNode(context: Context, id: string): (Instance?, OpResult?)
    local instance = context.sync:InstanceOf(id)
    if not instance then
        return nil, err("unknown_node")
    end
    return instance, nil
end

--- Re-mirror an instance so a mutation's name/class/parent reaches the broker.
local function refreshNode(context: Context, instance: Instance)
    local watch = context.watch
    if watch then
        watch:SyncNode(instance)
    end
end

--- Read a property, falling back to `gethiddenproperty` for non-scriptable ones.
local function readProperty(instance: Instance, name: string): (boolean, any)
    local ok, value = pcall(function()
        return (instance :: any)[name]
    end)
    if ok then
        return true, value
    end

    if gethiddenproperty then
        local hiddenOk, hiddenValue = pcall(gethiddenproperty, instance, name)
        if hiddenOk then
            return true, hiddenValue
        end
    end
    return false, nil
end

--- Assign a property, making it scriptable first if the direct write fails.
local function assignProperty(instance: Instance, name: string, value: any): (boolean, string?)
    local target = instance :: any
    local ok, writeErr = pcall(function()
        target[name] = value
    end)
    if ok then
        return true, nil
    end

    if setscriptable then
        pcall(setscriptable, instance, name, true)
        local retryOk, retryErr = pcall(function()
            target[name] = value
        end)
        if retryOk then
            return true, nil
        end
        return false, tostring(retryErr)
    end
    return false, tostring(writeErr)
end

--- Apply a [`DomValue`] to a property, coercing a bare scalar to the live enum.
local function applyDomValue(instance: Instance, name: string, domValue: DomValue): (boolean, string?)
    local readOk, current = readProperty(instance, name)
    local value: any = domValue.content
    if readOk and typeof(current) == "EnumItem" and typeof(value) ~= "EnumItem" then
        local resolved = enumFromScalar(current.EnumType, value)
        if resolved == nil then
            return false, "invalid_enum_value"
        end
        value = resolved
    end
    return assignProperty(instance, name, value)
end

-- Handlers ------------------------------------------------------------------

local function renameInstance(context: Context, content: { node: string, name: string }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    instance.Name = content.name
    refreshNode(context, instance)
    return OK
end

local function deleteInstance(context: Context, content: { node: string }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    instance:Destroy()
    return OK
end

local function moveNode(context: Context, content: { node: string, parent: string? }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    -- An absent parent detaches the node; DomWatch follows the removal.
    if content.parent == nil then
        instance.Parent = nil
        return OK
    end
    local parent, parentFailure = resolveNode(context, content.parent)
    if not parent then
        return parentFailure :: OpResult
    end
    instance.Parent = parent
    refreshNode(context, instance)
    return OK
end

local function createInstance(context: Context, content: { parent: string, class: string }): OpResult
    local parent, failure = resolveNode(context, content.parent)
    if not parent then
        return failure :: OpResult
    end
    local ok, created = pcall(function()
        return Instance.new(content.class)
    end)
    if not ok then
        return err("invalid_class")
    end
    created.Parent = parent
    refreshNode(context, created)
    return OK
end

local function addTag(context: Context, content: { node: string, tag: string }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    instance:AddTag(content.tag)
    return OK
end

local function removeTag(context: Context, content: { node: string, tag: string }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    instance:RemoveTag(content.tag)
    return OK
end

local function setProperty(context: Context, content: { node: string, name: string, value: DomValue }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    local ok, assignErr = applyDomValue(instance, content.name, content.value)
    if not ok then
        return err(assignErr or "assign_failed")
    end
    refreshNode(context, instance)
    return OK
end

local function setAttribute(context: Context, content: { node: string, name: string, value: DomValue }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    instance:SetAttribute(content.name, content.value.content)
    return OK
end

local function removeAttribute(context: Context, content: { node: string, name: string }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    instance:SetAttribute(content.name, nil)
    return OK
end

local function renameAttribute(context: Context, content: { node: string, old: string, new: string }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end
    if content.old == content.new then
        return OK
    end
    instance:SetAttribute(content.new, instance:GetAttribute(content.old))
    instance:SetAttribute(content.old, nil)
    return OK
end

--- The property names to read: the broker's dump-driven list when present,
--- otherwise the executor's own enumeration as a fallback.
local function propertyNames(instance: Instance, requested: { string }?): { string }
    if requested and #requested > 0 then
        return requested
    end

    local enumerate = executor.getproperties
    if enumerate then
        local listOk, names = pcall(enumerate, instance)
        if listOk and type(names) == "table" then
            return names
        end
    end
    return {}
end

local function getProperties(context: Context, content: { node: string, properties: { string } }): OpResult
    local instance, failure = resolveNode(context, content.node)
    if not instance then
        return failure :: OpResult
    end

    local properties: { NamedValue } = {}
    for _, name in propertyNames(instance, content.properties) do
        if SKIP_PROPERTIES[name] then
            continue
        end
        local readOk, value = readProperty(instance, name)
        if not readOk then
            continue
        end
        table.insert(properties, { name = name, value = toDomValue(context, value) })
    end

    local attributes: { NamedValue } = {}
    for name, value in instance:GetAttributes() do
        table.insert(attributes, { name = name, value = toDomValue(context, value) })
    end

    return {
        type = "Reads",
        content = { properties = properties, tags = instance:GetTags(), attributes = attributes },
    }
end

--- Compile and run `source`, capturing its `print`/`warn` output and returned values as text, with a compile error becoming an `Err` and a runtime one an `Err` carrying its stack trace.
local function runCode(_context: Context, content: { source: string }): OpResult
    local chunk, compileError = loadstring(content.source, RUN_CHUNK_NAME)
    if not chunk then
        return err(tostring(compileError))
    end

    local lines: { string } = {}
    local function capture(...)
        local parts = {}
        for index = 1, select("#", ...) do
            parts[index] = tostring(select(index, ...))
        end
        table.insert(lines, table.concat(parts, "\t"))
    end

    -- Proxy the chunk's globals so `print`/`warn` are captured, reads/writes of
    -- everything else pass through to the real environment.
    if setfenv and getfenv then
        local base = getfenv(chunk)
        setfenv(chunk, setmetatable({ print = capture, warn = capture }, { __index = base, __newindex = base }))
    end

    -- `xpcall`, not `pcall`: the handler runs before the stack unwinds, so the
    -- frames that raised the error are still there to walk. The trace stops here,
    -- at the frame that called the chunk.
    local returned = table.pack(xpcall(chunk, tracebackTo(runCode)))
    if not returned[1] then
        return err(tostring(returned[2]))
    end
    for index = 2, returned.n do
        table.insert(lines, tostring(returned[index]))
    end
    return { type = "Output" :: "Output", content = table.concat(lines, "\n") }
end

-- Dispatch ------------------------------------------------------------------

--- Route one typed operation to its handler, narrowing the payload by tag.
local function apply(context: Context, operation: Operation): OpResult
    if operation.type == "Rename" then
        return renameInstance(context, operation.content)
    elseif operation.type == "Delete" then
        return deleteInstance(context, operation.content)
    elseif operation.type == "Move" then
        return moveNode(context, operation.content)
    elseif operation.type == "Create" then
        return createInstance(context, operation.content)
    elseif operation.type == "AddTag" then
        return addTag(context, operation.content)
    elseif operation.type == "RemoveTag" then
        return removeTag(context, operation.content)
    elseif operation.type == "SetProperty" then
        return setProperty(context, operation.content)
    elseif operation.type == "SetAttribute" then
        return setAttribute(context, operation.content)
    elseif operation.type == "RemoveAttribute" then
        return removeAttribute(context, operation.content)
    elseif operation.type == "RenameAttribute" then
        return renameAttribute(context, operation.content)
    elseif operation.type == "GetProperties" then
        return getProperties(context, operation.content)
    elseif operation.type == "RunCode" then
        return runCode(context, operation.content)
    end
    return err(`unsupported_operation:{(operation :: any).type}`)
end

--- Apply `operation`, returning its typed [`OpResult`], with any handler error becoming an `Err` result carrying its stack trace rather than propagating.
function Operations.dispatch(context: Context, operation: Operation): OpResult
    local ok, result = xpcall(apply, tracebackTo(Operations.dispatch), context, operation)
    if not ok then
        return err(tostring(result))
    end
    return result
end

return Operations
