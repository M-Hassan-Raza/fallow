---
paths:
  - "editors/vscode/**"
---

# VS Code extension

Wraps the `fallow-lsp` binary with additional UI features. TypeScript codebase bundled with rolldown.

## Architecture
- `src/extension.ts` — Activation, command registration, lifecycle
- `src/client.ts`: LSP client setup (stdio transport, language selector for JS/TS/Vue/Svelte/Astro/MDX/JSON), wires `DiagnosticFilter` middleware
- `src/diagnosticFilter.ts`: client-side mute filter; caches last unfiltered diagnostics per URI, drops fallow-source diagnostics whose `code` is muted; serves both push (`handleDiagnostics`) and pull (`provideDiagnostics`) middleware paths
- `src/diagnosticMute.ts`: `LanguageStatusItem` (right-gutter, severity Warning when anything is muted), QuickPick manage UI (`createQuickPick` + `canSelectMany`), CodeAction provider (`source.fallow.mute` quick-fix), per-category and global toggle commands
- `src/download.ts` — Binary auto-download from GitHub releases (5 platform targets)
- `src/commands.ts` — Analysis and fix commands (spawns `fallow` CLI via execFile)
- Tree view providers for dead code (by issue type) and duplicates (by clone family)

## Binary resolution order
1. `fallow.lspPath` setting (explicit path, always wins)
2. Local `node_modules/.bin/` in workspace root (devDependency install)
3. `fallow-lsp` in system `PATH`
4. Previously downloaded binary in extension global storage
5. Auto-download from GitHub releases (if `fallow.autoDownload` enabled)

## Key behaviors
- **Lazy CLI analysis** — deferred until sidebar is first made visible (avoids double analysis with LSP)
- **LSP notification** — custom `fallow/analysisComplete` for real-time status bar updates
- **Config watch** — restarts LSP when `fallow.lspPath` or `fallow.trace.server` changes
- **Large buffer** — 50MB maxBuffer for CLI output on large monorepos
- **Diagnostic mute**: client-side filter backed by the LSP issue-type catalog. On startup the extension requests `fallow/issueTypes`; if the request fails (older LSP or invalid response), it falls back to the bundled `DIAGNOSTIC_CATEGORIES` list in `diagnosticFilter.ts`. Muting remains instant and does not restart the LSP. State lives in `context.workspaceState` under `fallow.diagnosticFilter.v1`. Filter ALWAYS guards on `Diagnostic.source === "fallow"` so TypeScript / ESLint diagnostics flow through untouched. Cache is bounded by `onDidCloseTextDocument` eviction. Keep the fallback list in sync with `DIAGNOSTIC_ISSUE_TYPES` / `fallow/issueTypes` in `crates/lsp/src/main.rs` plus diagnostics emitted outside the issue-type catalog; vitest coverage flags drift.

## Settings
`fallow.lspPath`, `fallow.autoDownload`, `fallow.issueTypes`, `fallow.duplication.threshold`, `fallow.duplication.mode`, `fallow.production`, `fallow.changedSince`, `fallow.trace.server`

`fallow.changedSince` mirrors the CLI's `--changed-since`: when set to a git ref, the LSP scopes diagnostics (and the CLI-driven sidebar) to files changed since that ref. Forwarded via `initializationOptions.changedSince` and as `--changed-since <ref>` to the CLI in `commands.ts`. Empty string means full scope. Changing the setting restarts the LSP and re-runs the sidebar analysis.

## Development
```bash
cd editors/vscode
pnpm install
pnpm run build     # rolldown production bundle
pnpm run watch     # development watch mode
pnpm run lint      # tsc --noEmit
pnpm run test      # unit + integration tests (vitest)
pnpm run package   # vsce package --no-dependencies
```

## Version management
Extension version is set from the git tag by CI — do not manually update `editors/vscode/package.json` version. The release workflow handles everything.
