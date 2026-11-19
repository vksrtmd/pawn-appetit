# Architecture

Pawn Appetit is a Tauri desktop app with a React/Vite frontend and a Rust backend. The frontend owns interactive UI state, board surfaces, tab layout, settings forms, and local optimistic UI updates. The Rust backend owns trusted filesystem access, engine process management, database reads and writes, archive extraction, imports, exports, and native integration.

## Frontend And Backend Boundary

Frontend code lives in `src/`. It calls generated, typed Tauri commands through `@/bindings` rather than reaching into backend implementation details. The generated file is `src/bindings/generated.ts`; `src/bindings/index.ts` re-exports it and adds hand-written type overrides. Browser-only fallbacks should stay isolated behind environment helpers so the UI can run in tests without Tauri globals.

Backend code lives in `src-tauri/src/`. Tauri commands should stay thin: validate input, call normal Rust services or module functions, and return serializable data or structured errors. Domain behavior belongs in regular functions that can be tested without launching a Tauri webview.

## State Ownership

Frontend state is owned by React components, hooks, stores, and tab/session helpers. It should model current UI intent: active board, selected game, command availability, pending form state, and visible progress.

Saving a board tab goes through `save(tabId, { dir })` in `src/state/tabSession.ts`, which looks up the tab's tree store by id (`registerTabStore`; an unmounted tab's store is rehydrated from session storage). It writes to the tab's file, updates its database game, or opens a save-as dialog (no file, or a temp import file), then clears the tab's dirty flag and records a save-as file as the tab's source. A failed write rejects and leaves the tab dirty; a cancelled dialog resolves `"cancelled"`.

Backend state is stored in `AppState` and related Rust services. It owns shared engine process maps, database connection pools, caches, and long-running job state. Backend state should include stable identifiers for processes or jobs so events can be routed back to the correct frontend surface.

Everything stored under a tab id belongs to the same module: the persisted tree (`open(tabId, tree)` writes it before the tab is added to the tab list, so the board's first render shows it), the side keys that tie a tab to a local, Chess.com or Lichess game (`getSideKey`/`setSideKey`), and the per-tab atoms. `duplicate(tabId)` copies the tree, source and side keys. `dispose(tabId)` removes the tree, side keys, missing-moves entry and per-tab atoms and calls `commands.killEngines(tabId)`; every way of closing a tab (`closeActiveTab`, `closeOtherTabs`, `closeTabsToRight`, `closeAllTabs`, the close button, replacing the lone "new" tab in `createTab`) disposes the tabs it removes. A per-tab atom is declared with `tabAtomFamily` (`src/state/tabAtoms.ts`), which registers it so `dispose` can remove it. `src/state/__tests__/tabAtoms.test.ts` fails when other code declares a per-tab atom family or touches a tab's session-storage keys.

## Tauri Command Flow

Commands are registered in `src-tauri/src/lib.rs` through the Specta/Tauri builder and exposed to TypeScript bindings during debug desktop builds. Frontend code invokes commands through the generated bindings. Backend commands emit typed events for progress and async results when a single command response is not enough.

When adding or changing a command, update the Rust command signature, regenerate or verify bindings as needed, and add focused tests for the underlying Rust function. Avoid changing public command names unless migration is intentional.

## Engine Lifecycle

Engine binaries are installed and launched by Rust code. The backend starts UCI processes, sends setup and search commands, reads engine output, and emits analysis or play-session updates to the frontend. The frontend should treat engines as asynchronous services and render readiness, searching, stopped, and failure states explicitly. A search ends only with the engine's `bestmove`: the final `BestMovesPayload` (progress 100, every earlier one stays below) carries it, and engine games play it. `stop` is followed by `isready`; the `bestmove` of the stopped search is ignored, and the engine takes a new search once it answers `readyok`.

Every UCI process lives in the `EngineRegistry` (`src-tauri/src/chess/registry.rs`), under the key `(tab id, engine path)`. Live analysis, engine games and Game Review all start engines through `get_or_start`: overlapping starts for one key share one process, and a process that is killed while it is still starting is dropped (and so killed) before it is handed out.

Each engine has an owner, the tab whose closing must end it. `kill_engines(tab)` ends every engine whose owner or key tab equals `tab` exactly (tab "1" does not match "10"). The board starts engines under derived ids (`<tab>white`, `<tab>black`) and passes the tab as `owner` to `get_best_moves`; Game Review runs under the key `report_<tab>` and passes the tab as `owner` to `analyze_game`, so closing a tab cancels its review. Ending a slot goes through `EngineProcess::shutdown(grace)`: `quit`, wait up to `grace`, then force-kill; a review's engine is killed at once.

Engine output is parsed in one place, `EngineProcess::handle_line`, for live analysis and Game Review. An engine's log keeps its most recent 2,000 lines. `get_engine_config` reads an engine's name and options through the same spawn and `uci` handshake, with a timeout.

## Database Modules

Database work is currently organized under `src-tauri/src/db/` plus related import, PGN, puzzle, and search modules. Rust owns SQLite connections, schema access, game normalization, position search, and import/export work. Frontend code should request data through commands instead of reading database files directly.

Every write to a game database goes through `src-tauri/src/db/write.rs`. A writer runs its SQL in `write::apply` with the kind of change it made (`Change::GamesInserted`, `GameEdited`, `GameDeleted`, `GamesDeleted`, `PlayersMerged`); in the same transaction `apply` refreshes the stored counts and, for edits, deletes and merges, removes the players, events and sites the change left without a game (those of the one edited or deleted game; every row after a bulk delete; none after a merge). The module takes a `SqliteConnection`, not app state. Writers don't call the count refresh or orphan sweep themselves. The one exception is `core::db_counts`, which counts a file that predates the stored counts and writes the result back.

Deleting one game (`Change::GameDeleted`) checks only that game's players, event and site for orphans; the other changes sweep every table.

What position search keeps between calls lives in `db/cache.rs`, keyed by database file: the game rows of a small file (up to 50,000 games) and finished search results. After its transaction commits, `apply` calls `cache::invalidate` for the connection's file, and `write::drop_database` calls it too, so the next search reads the file as it is. Each file has a generation that `invalidate` bumps; a search stores rows or results only under the generation it read the file at. The Game Review novelty check (`position_in_db`) scans through the same batched, index-filtered path, stops at the first match and takes no search permit, so it runs beside panel searches.

Each file's pool (`db/connection.rs`) always uses the safe settings (rollback journal, foreign keys on, busy timeout), opens connections on demand and upgrades the schema (end-time columns, the date-sort expression index) once, when the pool is created. The importer connects on its own with the fast import settings (`ConnectionOptions::import`) and never joins the pool: a file it creates runs with no journal and is deleted if the import fails; an existing file gets an in-memory rollback journal, so a failed import rolls back. Long database commands run their blocking work on `spawn_blocking`.

Long-running database operations should report progress through typed events and should avoid blocking UI workflows.

## Test Commands

`pnpm check` runs all of them in order, frontend first, and is what CI runs. Use these commands from the repository root:

```sh
pnpm check
pnpm test
pnpm lint
pnpm fmt:check
pnpm test:rust
pnpm lint:rust
pnpm fmt:rust:check
```

Rust backend checks use `--no-default-features` so contributors can run unit tests from a fresh checkout without building the frontend first. A bare `cargo test` or `cargo clippy` builds with default features and embeds `../dist`: without a `dist/` directory, as in a fresh worktree, it fails with `error: proc macro panicked`. Use the `pnpm` scripts, or create it first (`mkdir -p dist`). Use `pnpm build-vite` before default-feature production sanity checks.

## Common Workflows

Run the desktop app with `pnpm dev`. Build frontend assets with `pnpm build-vite`. Build the Tauri app with `pnpm tauri build`.

For backend-only work, prefer focused Rust tests under `src-tauri/src/` and run `pnpm test:rust`. For frontend work, add Vitest coverage near the changed components or hooks and run `pnpm test`.

Component tests use React Testing Library: render with `renderWithProviders` from `src/test/render.tsx` (wraps `MantineProvider`; `src/test/setup.ts` adds the jsdom shims and jest-dom matchers). Mock `react-i18next` and `@/bindings` per test.
