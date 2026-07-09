-- Dependencies
local Signal = require("@pkgs/signal")
local Trove = require("@pkgs/trove")

--- A wrapper around the websocket client.
local WebSocketClient = {}
WebSocketClient.__index = WebSocketClient

-- Types
export type WebSocketClientData = {
    _trove: Trove.Trove,
    _connection: typeof(WebSocket.connect("")),

    onMessage: Signal.Signal<string>,
    onClose: Signal.Signal<()>,
}
export type WebSocketClient = setmetatable<WebSocketClientData, typeof(WebSocketClient)>

--- Create a new [`WebSocketClient`] object.
function WebSocketClient.new(url: string): WebSocketClient
    local trove = Trove.new()
    local connection = WebSocket.connect(url)
    trove:Add(function()
        connection:Close()
    end)

    local self: WebSocketClientData = {
        _trove = trove,
        _connection = connection,
        onMessage = Signal.new(),
        onClose = Signal.new(),
    }
    trove:Add(self.onMessage)
    trove:Add(self.onClose)
    return setmetatable(self, WebSocketClient)
end

--- Destroy this object, and clean up.
function WebSocketClient.Destroy(self: WebSocketClient)
    self._trove:Destroy()
end

--- Initialise this object.
function WebSocketClient.Initialise(self: WebSocketClient)
    local trove = self._trove

    -- Tear down once the socket closes.
    local connection
    connection = self._connection.OnClose:Connect(function()
        if connection then
            connection:Disconnect()
        end

        self.onClose:Fire()
        self:Destroy()
    end)
    trove:Add(connection)

    -- Handle any new messages
    trove:Add(self._connection.OnMessage:Connect(function(message: string)
        self.onMessage:Fire(message)
    end))
end

--- Send a message.
function WebSocketClient.Send(self: WebSocketClient, message: buffer | string)
    self._connection:Send(if type(message) == "buffer" then buffer.tostring(message) else message)
end

return WebSocketClient
