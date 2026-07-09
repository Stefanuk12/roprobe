<!-- claude-md-version: 1 -->
# CLAUDE.md

NOTE: these **ONLY** apply to the `client` folder.

Guidance for Claude Code in this repo. Only what isn't obvious from the code. This file is **versioned** (marker above) and auto-synced from `rocult/script-template@main` at session start via `.meta/scripts/check-update.luau` — it self-updates only while you haven't edited it.

Opinionated **layout template** for a client-side, executor-side Roblox Luau mod. Wires `pesde` (packages), `mise` (tasks), `darklua` (build), `luau-lsp`/`selene`/`stylua` (gates). `src/` has one placeholder per layer (`contracts/example`, `lib/roundTo`, `engine/player`, `features/gun`) to replace; run `mise init` to rename the package first.

## Architecture — 4 layers, imports flow downward only

`@contracts` (pure types, no runtime) ← `@lib` (standalone, publishable utils) ← `@engine` (game infra others build on) ← `@features` (user-facing behaviors). A layer imports only lower ones, never upward.

**Placing new code**: pure type → `@contracts`; ships standalone, nothing app-specific → `@lib`; infra only meaningful in this mod → `@engine`; user-facing behavior → `@features`. The `@lib`/`@engine` line is *publishability* — if you'd `pesde publish` it and a stranger could use it unchanged, it's `@lib`, even when the code reads generic.

## Commands

`mise` tasks (`mise.toml`), from repo root. `mise install` first (toolchain + `pesde install`).

```bash
mise format / format-check   # stylua (format-check = CI gate)
mise lint                    # selene
mise check                   # luau-lsp analyze
mise build [variant]         # bundle -> dist/<variant>-build.lua (no arg = all)
mise sourcemap / clean / init
mise ci                      # sourcemap + lint + format-check + check + build main
```

No unit tests — static analysis is the only gate. CI (`.github/workflows/check.yaml`) runs on push/PR to `main`.

## Build

`mise build` → `.meta/scripts/build.luau` bundles `src/init.luau` via darklua, one file per variant. **Variants auto-discover** from `.meta/.darklua.<variant>.json` (ships `main`; add a config to add a variant). Each config's `inject_global_value` sets build globals — `main` injects `DEBUG = false`; guard with `if DEBUG then`.

Darklua footguns luau-lsp won't catch (only the build does):
- **No `if/then/else` expressions** (`remove_if_expression`) — use `if/then` blocks or `and`/`or`.
- **No complex expressions in `{...}` interpolation** (`remove_interpolated_string` breaks on `==`, `and`/`or`, operators) — precompute a local.
- **Type-only modules must still `return` a runtime value** (`contracts/example.luau` → `return true`), else darklua errors.

## Layout & style

- **Aliases** (`.luaurc`): `@pkgs` (roblox_packages), `@contracts`, `@lib`, `@engine`, `@features`, `@src`, `@lune`. String requires (`require("@features/gun")`); stylua's `sort_requires` orders them.
- **Deps** via `pesde` (own index or wally) → `roblox_packages/`, `luau_packages/`, `lune_packages/` (gitignored). Executor types from `rocult/luau-defs` (`@custom`/`@luarmor`/`@luraph`, wired in `.vscode/settings.json`).
- **Primitives**: `@pkgs/trove` (lifecycle), `@pkgs/signal` (events), `@pkgs/promise` (async) — prefer over ad-hoc equivalents.
- **Formatting** is stylua's job (`.meta/stylua.toml`, `mise format`). selene: `std = "roblox"` (executor globals allowed).
- **Naming** (identifier casing + `_private`: defer to the Roblox guide): folders `snake_case`; multi-export `@lib`/`@engine` modules `PascalCase` (e.g. `SpringModel`); single-function/value files and `@features`/`@contracts` modules lowercase (`gun.luau`, `roundTo.luau`). Public methods are `PascalCase` — deliberate override of the Roblox guide (which uses camelCase).
- **Commits**: conventional, scoped — `feat(Scope):`, `fix(Scope):`, `refactor(Scope):`, `chore(Scope):`.
- **No defensive clamps** the source already guarantees; prefer fundamental fixes over band-aids. Keep single-caller predicate checks inline; extract only at ≥2 callers or real shared state — and don't extract small single-use helpers, inline them.
- **Comments**: inline comments are terse — one short line of intent, never multi-line prose (e.g. `-- Destroy the previous instance, so it does not interfere`). Section labels (`-- Dependencies`, `-- Types`, `-- Constants`) and `---` docs on public APIs are house style — see `features/gun.luau`. (darklua strips all comments from builds.)
- **Read both style guides in full — fetch the URLs, don't rely on memory** — follow where they don't conflict with the above: [Roblox Lua Style Guide](https://roblox.github.io/lua-style-guide/), [Kampfkarren's Luau guidelines](https://github.com/Kampfkarren/kampfkarren-luau-guidelines/blob/main/README.md).
- **Avoid `pcall`/`xpcall`** where the code can be rewritten to avoid them (flag on review); reserve for genuinely fallible game/executor APIs.

## Classes & lifecycle

Every class follows this shape (see `features/gun.luau`):

```luau
-- Dependencies
local Trove = require("@pkgs/trove")

-- Types
type Trove = Trove.Trove
export type GunData = { _trove: Trove, ammo: number }

--- A sample gun.
local Gun = {}
Gun.__index = Gun
export type Gun = setmetatable<GunData, typeof(Gun)>

--- Create a new [`Gun`].
function Gun.new()
    local self: GunData = { _trove = Trove.new(), ammo = 30 }
    return setmetatable(self, Gun)
end

--- Wire connections/tasks onto `_trove`.
function Gun.Initialise(self: Gun) end

--- Destroy and clean up.
function Gun.Destroy(self: Gun)
    self._trove:Destroy()
end
```

- **Dot syntax + explicit `self`**: `function Class.Method(self: Class, ...)`, never `Class:Method` (the colon form breaks luau-lsp's `self` analysis; callers may still use `:`).
- **Type**: `export type XData = {...}` then `export type X = setmetatable<XData, typeof(X)>`.

### No memory leaks (Trove)

- Any class owning connections/tasks/Instances holds a `_trove` and reclaims it in `Destroy` (`self._trove:Destroy()`). **Never** store a raw connection/Instance/task on `self` outside the trove; register via `_trove:Connect`/`:Add`/`:Construct`.
- `@engine/globalTrove` is the root Trove, cached in `getgenv()`. On re-execution it `Destroy`s the previous run's trove (waits, then makes a fresh one) — add top-level objects to it so one teardown reclaims everything. Executor scripts re-run into a persistent env, so any un-troved connection leaks across runs.

## Roblox / executor specifics

- Executor-side: mount UI to `cloneref(game:GetService("CoreGui"))` (or `gethui()`), not `PlayerGui`. Wrap privileged CoreGui ops narrowly in `setthreadidentity(8)`, then restore.
- Instance creation: properties first, `Parent` last. Don't annotate `Instance.new` locals — the string literal types them.
- `rawget` only to read fields off game-controlled Lua tables; direct access for our own classes and Roblox Instances.

## Local naming (refinements the guides don't cover)

- **Helper verbs**: `build` (in-memory value), `get` (plain accessor), `ensure` (lazy get-or-create), `resolve` (lookup/normalize/select), `compute` (derived math), `xToY`/`fromX` (convert). `new` = metatable constructor only.
- **Booleans**: interrogative prefix — `isX`, `hasX`, `shouldX`, `canX`, `wasX`. Bare adjective (`enabled`, `visible`) only for a `SetX(self, x: boolean)` param + its field; toggle/keybind payloads are `enabled: boolean`.
- **Collections**: arrays are bare plural nouns (never `*List`); lookup maps `xByKey` (`modelById`); sets `{ [K]: true }` plural nouns (no `Set` suffix). `Map`/`Set`/`List` suffixes for type aliases only.
- **Loops**: unused vars bare `_`; counters `i` then `j`; drop an unneeded for-in key to `_`.
- **Spell nouns out**: `config` not `cfg`, `position` not `pos`, `direction` not `dir`, `index` not `idx`, `cframe` not `cf`. `fov`/`ui` stay short.
- **Instance locals**: class instances lowerCamel of the class (`springModel`); Roblox Instances by role noun (`frame`, `container`), never the lowercased className. Trove field is `_trove`; stored connections `_<descriptor>Connection`.

## Auditing game calls

When asked to find game-specific function calls, read the targets + their required files, then output only `call — file:line` (no context). "Game-specific" = a function from the *game's own* code, required via instance path (`require(game. ...)`). Exclude:
- Built-in Luau + Roblox engine methods (e.g. `Workspace:Raycast`).
- Functions defined inside the calling function.
- `@`-aliased modules (our own code).
- Calls already wrapped in whatever safe-call helper the project designates (if any).

```lua
local x = require(game.ReplicatedStorage.ModuleScript)  -- flag x.bar()
local y = require("@lib/foo")                            -- do NOT flag y.bar()
```

## CI/CD (`.github/workflows/`)

- **check** — format-check + lint + analyze on push/PR to `main`.
- **release** — manual; tags from `pesde.toml` version, GitHub release + `dist/main-build.lua`.
- **luarmor_deploy** — manual; deploys to Luarmor (replace the `script-id` matrix; needs `LUARMOR_API_KEY` + `TWOCAPTCHA_API_KEY`).

## Changelog

End-user-facing — keep broad and vague; no developer/implementation detail, no security/anti-cheat/bypass specifics, nothing revealing how a feature works.
