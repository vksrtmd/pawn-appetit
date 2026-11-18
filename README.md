# Pawn Appétit

A cross-platform chess desktop app for game analysis, engine play and opening repertoire training. Built with Tauri: a Rust backend and a React frontend.

## Features

- Game Management - Store and analyze your games from lichess.org and chess.com
- Multi-Engine Analysis - Supports all UCI chess engines for deep position evaluation
- Repertoire Training - Prepare openings and train them with spaced repetition
- Engine & Database Management - Simple installation and management of engines and databases
- Position Search - Absolute or partial position search in your database
- Cross-Platform - Runs on Windows, macOS, and Linux

## Building from Source

Install the [Tauri prerequisites](https://tauri.app/start/prerequisites/) and [pnpm](https://pnpm.io/), then:

```bash
git clone https://github.com/vksrtmd/pawn-appetit.git
cd pawn-appetit
pnpm install
pnpm tauri dev     # run in development mode
pnpm tauri build   # production build, output in src-tauri/target/release
```

`pnpm check` runs every check CI runs. See [CONTRIBUTING.md](./CONTRIBUTING.md) for the other commands and [docs/architecture.md](./docs/architecture.md) for how the frontend and backend fit together.

### Docker Build

```bash
docker build -t pawn-appetit .
docker run -d --name pawn-appetit-app pawn-appetit
docker cp pawn-appetit-app:/output/pawn-appetit ./pawn-appetit
```

## Privacy & Telemetry

Pawn Appétit includes opt-in telemetry. It is off by default and nothing is sent, including the first-run event, until you turn it on in the settings. Collected data includes:

- Application version, OS, and architecture
- Country code taken from your system locale only (no IP lookup service is used)
- Basic usage events

**No personal information, IP addresses, or game content is collected.** You can turn telemetry off again at any time in the settings.

## License

Pawn Appétit is licensed under the [GPL-3.0 License](./LICENSE).

## Acknowledgments

This is a fork of [Pawn Appétit](https://github.com/Pawn-Appetit/pawn-appetit), itself a fork of [En Croissant](https://github.com/franciscoBSalgueiro/en-croissant).
