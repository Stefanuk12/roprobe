-- Dependencies
local Trove = require("@pkgs/trove")
local Signal = require("@pkgs/signal")

--- A wrapper around the websocket client.
local WebSocketClient = {}
WebSocketClient.__index = WebSocketClient

-- Types
export type WebSocketClientData = {
    _trove: Trove.Trove,
    _connection: typeof(WebSocket.connect("")),

    onMessage: Signal.Signal<()>,
}
export type WebSocketClient = setmetatable<WebSocketClientData, typeof(WebSocketClient)>

--- Create a new [`WebSocketClient`] object.
function WebSocketClient.new(url: string): WebSocketClient
    local trove = Trove.new()
    local connection = WebSocket.connect(url)
    trove:Add(function()
        connection:Close()
    end)

    -- Create and return the object
    local self: WebSocketClientData = {
        _trove = trove,
        _connection = connection,
        onMessage = Signal.new(),
    }
    return setmetatable(self, WebSocketClient)
end

--- Destroy this object, and clean up.
function WebSocketClient.Destroy(self: WebSocketClient)
    self._trove:Destroy()
end

--- Initialise this object.
function WebSocketClient.Initialise(self: WebSocketClient)
    local trove = self._trove

    -- Listen when for when the connection closes and clean up
    local connection
    connection = self._connection.OnClose:Connect(function()
        if connection then
            connection:Disconnect()
        end
        self:Destroy()
    end)
    trove:Add(connection)

    -- Listen for new messages
    trove:Add(self._connection.OnMessage:Connect(function(_message: string)

    end))    
end

-- Send a message.
function WebSocketClient.Send(_self: WebSocketClient)
    
end

return WebSocketClient
