-- Dependencies
local Http = require("@modules/luau_lsp/guardedRequest")
local Log = require("@luau_lsp/Utils/Log")
local Logger = require("@core/Logger")
local ServerEndpoints = require("@luau_lsp/ServerEndpoints")
local types = require("@luau_lsp/types")

-- Services
local HttpService = cloneref(game:GetService("HttpService"))

-- Types
type EncodedInstance = types.EncodedInstance
type HttpResponse = types.HttpResponse
type Result<T, E> = types.Result<T, E>
export type ShimsData = {}

--- Patches the environment so the plugin code can be translated to work on an executor.
local Shims = {}
Shims.__index = Shims
export type Shims = setmetatable<ShimsData, typeof(Shims)>

--- Create a new [`Shims`] object.
function Shims.new(): Shims
    local self: ShimsData = {}
    return setmetatable(self, Shims)
end

--- Destroy this object.
function Shims.Destroy(_self: Shims) end

--- `ServerEndpoints` HTTP transport -> executor `request` (no `HttpService`).
function Shims._PatchServerEndpoints(_self: Shims)
    -- We deliberately overwrite the vendored endpoints. luau-lsp's new solver mis-expands the
    -- field's `Result<HttpResponse, string>` type and fails its own subtyping check ("invalid
    -- path"), so patch through an `any` view; the replacement bodies are still type-checked.
    local Endpoints: any = ServerEndpoints
    function Endpoints.full(address: string, tree: EncodedInstance): Result<HttpResponse, string>
        return Http({
            Method = "POST",
            Url = `{address}/full`,
            Headers = {
                ["Content-Type"] = "application/json",
            },
            Body = HttpService:JSONEncode({
                tree = tree,
            }),
        })
    end

    function Endpoints.clear(address: string): Result<HttpResponse, string>
        return Http({
            Method = "POST",
            Url = `{address}/clear`,
        })
    end

    function Endpoints.getFilePaths(address: string): Result<HttpResponse, string>
        return Http({
            Method = "GET",
            Url = `{address}/get-file-paths`,
            Headers = {
                ["Content-Type"] = "application/json",
            },
        })
    end
end

--- Plugin `Log` -> roprobe [`Logger`] so output lands in the roprobe log file.
function Shims._PatchLog(_self: Shims)
    function Log._log(level: number, ...: any)
        if level == Log.Level.WARN then
            Logger:warn("luau-lsp", ...)
        elseif level == Log.Level.ERROR then
            Logger:error("luau-lsp", ...)
        elseif level == Log.Level.DEBUG then
            Logger:debug("luau-lsp", ...)
        else
            Logger:info("luau-lsp", ...)
        end
    end

    (Log :: any).error = function(...: any)
        Logger:error("luau-lsp", ...)
    end
end

--- Install every patch.
function Shims.Apply(self: Shims)
    self:_PatchServerEndpoints()
    self:_PatchLog()
end

return Shims
