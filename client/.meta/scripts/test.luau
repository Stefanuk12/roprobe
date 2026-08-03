-- Dependencies
local broker = require("../tests/lib/broker")
local fs = require("@lune/fs")
local process = require("@lune/process")
local stdio = require("@lune/stdio")

-- Constants
local TESTS_DIR = ".meta/tests"
local SPEC_PATTERN = "^(.+)%.spec%.luau$"

-- These drive a real broker over a real socket, so a build has to exist.
local binary = broker.binary()
if not binary then
    stdio.ewrite("No broker binary; build one first (`direnv exec ../broker cargo build`).\n")
    process.exit(1)
    return
end
print(`[test] against {binary}`)

local specs: { string } = {}
for _, entry in fs.readDir(TESTS_DIR) do
    local name: string? = entry:match(SPEC_PATTERN)
    if name ~= nil then
        table.insert(specs, name)
    end
end
table.sort(specs)

local failures = 0
for _, name in specs do
    local spec = require(`../tests/{name}.spec`) :: { name: string, run: ((boolean, string) -> ()) -> () }
    print(`\n{spec.name}`)

    local ok, err = pcall(spec.run, function(passed: boolean, label: string)
        print(`  {if passed then "ok  " else "FAIL"}  {label}`)
        if not passed then
            failures += 1
        end
    end)

    if not ok then
        print(`  FAIL  {name} threw: {err}`)
        failures += 1
    end
end

if failures > 0 then
    stdio.ewrite(`\n{failures} check(s) failed\n`)
    process.exit(1)
    return
end
print("\nall checks passed")
