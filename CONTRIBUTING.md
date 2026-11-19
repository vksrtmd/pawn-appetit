# Contributing

## Setup

```sh
pnpm install
pnpm dev
```

Database downloads need `VITE_SERVER_URL`, the base URL of the file server that serves game and puzzle databases. Set it in a `.env` file at the repo root:

```sh
VITE_SERVER_URL=https://example.com/databases
```

Without it, the "Web" tab of the Add Database dialog shows an error. CI sets it from the `SERVER_URL` repository variable.

## Commands

| Command            | What it does                                                                          |
| ------------------ | ------------------------------------------------------------------------------------- |
| `pnpm check`       | Every check CI runs: formatting, lint, typecheck, Vitest, rustfmt, Clippy, Rust tests |
| `pnpm test`        | Frontend tests                                                                        |
| `pnpm test:rust`   | Rust tests                                                                            |
| `pnpm fmt`         | Format the frontend                                                                   |
| `pnpm lint:fix`    | Fix frontend lint issues where possible                                               |
| `pnpm lint:rust`   | Clippy                                                                                |
| `pnpm tauri build` | Build the app; output in `src-tauri/target/release`                                   |

Run Cargo through the `pnpm` scripts: they pass `--no-default-features`. The default `custom-protocol` feature embeds `dist/`, so a bare `cargo test` or `cargo clippy` fails without it; run `pnpm build-vite` first if you need a default-feature build.

## Code layout

The Rust backend (filesystem, chess engines, databases) is in [src-tauri/src](./src-tauri/src/); the React frontend is in [src](./src/). See [docs/architecture.md](./docs/architecture.md) for the boundary between them, state ownership, Tauri command flow and the engine lifecycle.

## Pull requests

Branch from `main`, run `pnpm check`, and try every feature you touched in a built app (`pnpm tauri build -b none`) before opening the PR.
