# Pawn Appétit

Tauri desktop app: React/Vite frontend in `src/`, Rust backend in `src-tauri/`.

Read `docs/architecture.md` before changing code that crosses the frontend/backend boundary, and `GLOSSARY.md` before naming a domain concept.

## Checks

`pnpm check` is the full check: formatting, lint, typecheck, Vitest, then rustfmt, Clippy and Rust tests. CI runs the same command. Run it before committing.

- Rust tests: `pnpm test:rust -- <filter>`. Run Cargo through the `pnpm` scripts (`test:rust`, `lint:rust`): they pass `--no-default-features`. A bare `cargo test` or `cargo clippy` embeds `dist/` and, without it, fails with `error: proc macro panicked`.
- A cold Rust build in a fresh worktree takes about 6 minutes: start `pnpm test:rust` and `pnpm check` with `run_in_background: true`.
- A fresh worktree has no `node_modules`: start `pnpm install --frozen-lockfile` in the background as your first step. Vitest and `pnpm check` need it (`Failed to resolve import "@testing-library/jest-dom/vitest"` means it is missing).
