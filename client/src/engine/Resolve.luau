-- Types
type Closure = (...any) -> ...any

--- Turns the broker's path strings into live instances and closures.
local Resolve = {}

--- Walk `parts` down from `from`, retrying joins so names holding a dot still resolve.
local function walk(from: Instance, parts: { string }, start: number): Instance?
    if start > #parts then
        return from
    end

    for stop = start, #parts do
        local name = table.concat(parts, ".", start, stop)
        local ok, child = pcall(function()
            return from:FindFirstChild(name)
        end)
        if ok and child then
            local found = walk(child, parts, stop + 1)
            if found then
                return found
            end
        end
    end
    return nil
end

--- The instance at a `GetFullName`-style path, with or without the `game.` prefix.
function Resolve.Instance(path: string): Instance?
    local parts = string.split(path, ".")
    if parts[1] == "game" then
        table.remove(parts, 1)
    end
    return walk(game, parts, 1)
end

--- A Lua function named `name` defined in `script`, found the gc way.
local function findInScript(script: Instance, name: string): Closure?
    for _, value in getgc() do
        if type(value) ~= "function" or not islclosure(value) or isexecutorclosure(value) then
            continue
        end

        local ok, matches = pcall(function()
            return debug.info(value :: Closure, "n") == name and getfenv(value).script == script
        end)
        if ok and matches then
            return value
        end
    end
    return nil
end

--- A closure from `path`: `Some.Script` is its main function, `Some.Script:name` a function inside it, `genv.a.b` an executor global.
function Resolve.Closure(path: string): Closure?
    if string.sub(path, 1, 5) == "genv." then
        local node: any = getgenv()
        for _, key in string.split(string.sub(path, 6), ".") do
            if type(node) ~= "table" then
                return nil
            end
            node = node[key]
        end
        return if type(node) == "function" then node else nil
    end

    local scriptPath, name = string.match(path, "^(.-):([^:]+)$")
    local script = Resolve.Instance(scriptPath or path)
    if not script then
        return nil
    end

    if name then
        return findInScript(script, name)
    end

    local ok, main = pcall(getscriptclosure, script :: any)
    return if ok then main else nil
end

return Resolve
