--- The spy's other half, run inside an actor's Luau VM, where the main thread's
--- metamethod hooks do not exist. Installs the same [`RemoteHook`] there and
--- relays what it captures over the comm channel.
---
--- Bundled to `dist/actor-build.lua` and embedded in the main build as a string;
--- `run_on_actor` compiles it per VM with the channel id, the JSON-encoded
--- settings (only scalars survive the crossing) and the actor as its varargs.

-- Dependencies
local Broker = require("@contracts/Broker")
local Executor = require("@engine/executor")
local RemoteHook = require("@engine/hooks/RemoteHook")
local uniqueId = require("@engine/uniqueId")

-- Services
local HttpService = cloneref(game:GetService("HttpService"))

-- Constants
--- The event name captured calls are relayed under, matched by the main thread.
local RELAY_EVENT = "roprobe.call"
--- The event the main thread broadcasts to unhook every VM it injected.
local UNLOAD_EVENT = "roprobe.unload"

-- Types
type LuaValue = Broker.LuaValue
type RemoteCall = Broker.RemoteCall
type SpyConfig = Broker.SpyConfig

-- The actor comes last because it is the one that can be absent: a dropped nil
-- mid-vararg would shift the settings into its slot.
local channelId: number, settings: string, actor: Instance? = ...

-- Held as a local: writing through `getgenv()` in place reads as an assignment
-- to an undeclared global to selene.
local env = getgenv()

-- Re-injecting would stack a second hook and report every call twice. Keyed on
-- the channel, not a flag: a settings change opens a new one, and refusing that
-- injection would leave the VM hooked to a channel nobody listens on.
local previous = env.roprobe_actor_hook
if previous then
    if env.roprobe_actor_channel == channelId then
        return true
    end
    pcall(function()
        previous:Destroy()
    end)
    -- Cleared with the hook it named, or the guard above would refuse a later
    -- injection on this channel and strand the VM.
    env.roprobe_actor_hook = nil
    env.roprobe_actor_channel = nil
end

local getCommChannel = Executor.getCommChannel
if not getCommChannel then
    return true
end

local okChannel, channel = pcall(getCommChannel, channelId)
if not okChannel or not channel then
    return true
end

local okConfig, config = pcall(function(): SpyConfig
    return HttpService:JSONDecode(settings)
end)
if not okConfig then
    return true
end

--- Which actor this VM belongs to, for naming the calls it reports. Asked from
--- inside first: an executor that pools several actors onto one VM is the only
--- one that knows, and the handed-in actor is only what the main thread guessed.
local function resolveActor(): string?
    local resolve = Executor.getCurrentActor
    if resolve then
        local ok, current = pcall(resolve)
        if ok and typeof(current) == "Instance" then
            actor = cloneref(current)
        end
    end

    if typeof(actor) ~= "Instance" then
        return nil
    end

    local resolved = actor :: Instance
    local ok, path = pcall(function()
        return resolved:GetFullName()
    end)
    if ok then
        return path
    end
    return resolved.Name
end

--- A buffer cannot cross the channel, so it travels as the bytes it holds; the
--- codec takes a string for either shape.
local function flatten(values: { LuaValue })
    for _, value in values do
        if value.type == "Buffer" and type(value.content) == "buffer" then
            value.content = buffer.tostring(value.content :: buffer)
        elseif value.type == "Table" then
            local captured = value.content
            flatten(captured.array)
            for _, entry in captured.entries do
                flatten({ entry.key, entry.value })
            end
        end
    end
end

local hook = RemoteHook.new(config, function(instance: Instance): string
    -- No DOM mirror in here, so an instance is only nameable by its `UniqueId`.
    return uniqueId(instance) or ""
end, {
    actor = resolveActor(),
    -- The main VM owns the shared state; this one only adds the outgoing calls
    -- its own parallel scripts make.
    ownsSharedState = false,
})

-- The channel is a bindable of our own, so the spy would capture its own relays.
hook:Ignore(channel :: any)

hook.onCall:Connect(function(call: RemoteCall)
    flatten(call.arguments)
    if call.returns then
        flatten(call.returns)
    end

    pcall(function()
        channel:Fire(RELAY_EVENT, call)
    end)
end)

hook:Initialise()
-- Claimed only once the hook is in place, so a VM that fell out above is left as
-- it was found.
env.roprobe_actor_hook = hook
env.roprobe_actor_channel = channelId

-- No trove reaches into this VM, so the teardown arrives over the channel.
local unload: RBXScriptConnection
unload = channel.Event:Connect(function(event: string)
    if event ~= UNLOAD_EVENT then
        return
    end

    hook:Destroy()
    -- Only this connection holds the hook, so it has to let go or every
    -- re-injection leaves one behind.
    unload:Disconnect()
    if env.roprobe_actor_hook == hook then
        env.roprobe_actor_hook = nil
        env.roprobe_actor_channel = nil
    end
end)

return true
