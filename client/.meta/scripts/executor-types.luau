-- Dependencies
local fs = require("@lune/fs")

-- Constants
local PACKAGE_ROOT = "luau_packages/.pesde"
local PACKAGE_PREFIX = "stefanuk12+luau_defs"
local OUTPUT = ".meta/types"

--- luau-lsp 1.69 dropped `declare class X [extends Y] ... end` in favour of
--- `declare extern type X [extends Y] with ... end`, and rejects the whole
--- definitions file when it sees the old form (reported, misleadingly, as
--- "Failed to read definitions file"). Rewriting is a no-op once luau-defs
--- itself migrates.
local function migrate(source: string): string
    return (source:gsub("declare class ([%a%d_]+)([^\n]*)", function(name: string, rest: string): string
        local extends = rest:match("^%s+extends%s+[%a%d_]+")
        if extends then
            return `declare extern type {name}{extends} with`
        end
        return `declare extern type {name} with`
    end))
end

--- Locate the installed luau-defs package, whose path carries a version pesde picks.
local function resolvePackage(): string
    if not fs.isDir(PACKAGE_ROOT) then
        error(`{PACKAGE_ROOT} is missing - run 'mise run deps' first`)
    end

    for _, entry in fs.readDir(PACKAGE_ROOT) do
        if entry:sub(1, #PACKAGE_PREFIX) ~= PACKAGE_PREFIX then
            continue
        end

        for _, version in fs.readDir(`{PACKAGE_ROOT}/{entry}`) do
            local package = `{PACKAGE_ROOT}/{entry}/{version}/luau_defs`
            if fs.isDir(package) then
                return package
            end
        end
    end

    error(`no {PACKAGE_PREFIX} package under {PACKAGE_ROOT} - run 'mise run deps' first`)
end

local package = resolvePackage()
if not fs.isDir(OUTPUT) then
    fs.writeDir(OUTPUT)
end

-- Type definitions are migrated; the docs beside them ride along untouched so
-- both live under one version-independent path for the editor to point at.
for _, name in fs.readDir(`{package}/types`) do
    fs.writeFile(`{OUTPUT}/{name}`, migrate(fs.readFile(`{package}/types/{name}`)))
end
for _, name in fs.readDir(`{package}/docs`) do
    fs.writeFile(`{OUTPUT}/{name}`, fs.readFile(`{package}/docs/{name}`))
end

print(`Executor type definitions written to {OUTPUT}/`)
