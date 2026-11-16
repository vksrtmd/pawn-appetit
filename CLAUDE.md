# Pawn Appetit

Domain terms: `GLOSSARY.md`. Architecture and the full check list: `docs/architecture.md` ("Test Commands"). There is no `docs/adr/` yet.

## Checks

- Each worktree has its own `node_modules`: run `pnpm install --frozen-lockfile` first (about a minute).
- The frontend full check, as CI runs it: `pnpm test`, `pnpm typecheck`, `pnpm lint`, `pnpm fmt:check`.
- Read a check's exit code from the command itself (`pnpm typecheck; echo $?`). A pipe such as `| tail` reports the pipe's last command.
- Every `pnpm test` run writes its results to `node_modules/.vitest/last-run.json`. When a full run fails, read the failing tests from there (`jq '.testResults[] | select(.status=="failed") | .assertionResults[] | select(.status=="failed") | .fullName' node_modules/.vitest/last-run.json`) before rerunning: a flaky failure may not come back.

## Commits

Conventional commits (`commitlint.config.js`). A `test:` commit changes tests only: a change to app code under `src/` goes in its own `fix:` or `feat:` commit. `scripts/check-test-commit.sh` enforces this in the commit-msg hook and in CI.
