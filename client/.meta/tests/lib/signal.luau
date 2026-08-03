-- Types
export type Connection = { Disconnect: (Connection) -> () }
export type Signal = {
    Connect: (Signal, handler: (...any) -> ()) -> Connection,
    Fire: (Signal, ...any) -> (),
}

--- The smallest thing that passes for an `RBXScriptSignal` or an executor
--- socket event: enough for `Trove`, `LogService.MessageOut` and `OnMessage`.
local signal = {}

--- Create a new [`Signal`].
function signal.new(): Signal
    local handlers: { (...any) -> () } = {}

    return {
        Connect = function(_, handler)
            table.insert(handlers, handler)
            return {
                Disconnect = function()
                    local index = table.find(handlers, handler)
                    if index then
                        table.remove(handlers, index)
                    end
                end,
            } :: any
        end,

        Fire = function(_, ...)
            -- Cloned, so a handler disconnecting itself mid-fire cannot skip the next one.
            for _, handler in table.clone(handlers) do
                handler(...)
            end
        end,
    }
end

return signal
