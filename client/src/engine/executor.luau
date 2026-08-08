--- The executor APIs the spy uses that `rocult/luau-defs` does not declare, and
--- that no executor is obliged to provide. Read off `getgenv()` so a missing one
--- is a `nil` field to branch on, rather than source that will not analyse.

-- Types
--- The BindableEvent-alike `create_comm_channel` hands back, the only way a value
--- crosses between the main VM and an actor's.
export type CommChannel = {
    Event: RBXScriptSignal<...any>,
    Fire: (self: CommChannel, ...any) -> (),
}

--- A Luau state as the executors that return them from `getactorstates` shape it.
export type LuaState = {
    IsActorState: boolean,
    Execute: (self: LuaState, source: string, ...any) -> (),
    GetActors: (self: LuaState) -> { Actor },
}

local env = getgenv() :: any

local Executor = {
    --- Open a channel, answering its id and the channel itself. The id is what
    --- crosses into an actor; the channel itself cannot.
    createCommChannel = env.create_comm_channel :: (() -> (number, CommChannel))?,
    --- Reopen a channel by id, which is how the actor side gets hold of one.
    getCommChannel = env.get_comm_channel :: ((id: number) -> CommChannel)?,
    --- Compile and run `source` inside an actor's VM, with `...` as its varargs.
    runOnActor = env.run_on_actor :: ((actor: Actor, source: string, ...any) -> ())?,
    --- The same, for an executor that exposes VMs as threads rather than actors.
    runOnThread = env.run_on_thread :: ((state: any, source: string, ...any) -> ())?,
    --- Every live actor.
    getActors = env.getactors :: (() -> { Actor })?,
    --- Actors the game has since destroyed, whose VMs can still be running.
    getDeletedActors = env.getdeletedactors :: (() -> { Actor })?,
    --- Every Luau state, for the executors that expose them directly.
    getActorStates = env.getactorstates :: (() -> { any })?,
    --- The actor whose VM we are running in, from inside one.
    getCurrentActor = env.get_current_actor :: (() -> Instance?)?,
    --- Fires as a VM is created, so an actor created and used in one frame is
    --- not missed the way `DescendantAdded` would miss it.
    onActorStateCreated = env.on_actor_state_created :: RBXScriptSignal<Instance>?,
}

--- Whether this executor can run our hook inside an actor's VM at all; without a
--- channel there is no way back.
function Executor.CanReachActors(): boolean
    if Executor.createCommChannel == nil or Executor.getCommChannel == nil then
        return false
    end

    local byActor = Executor.runOnActor ~= nil and Executor.getActors ~= nil
    local byState = Executor.runOnThread ~= nil and Executor.getActorStates ~= nil
    return byActor or byState
end

return Executor
