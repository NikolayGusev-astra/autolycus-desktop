# Штурман Desktop

**Native desktop AI assistant** — full port of [Hermes Agent](https://github.com/NousResearch/hermes-agent) desktop from Electron to Tauri 2 + Rust.

[![Build](https://github.com/NikolayGusev-astra/autolycus-desktop/actions/workflows/release.yml/badge.svg)](https://github.com/NikolayGusev-astra/autolycus-desktop/actions/workflows/release.yml)
[![Release](https://img.shields.io/github/v/release/NikolayGusev-astra/autolycus-desktop)](https://github.com/NikolayGusev-astra/autolycus-desktop/releases)

## Features

- **ConnectScreen v2** — 3 modes: Local / Remote / SSH with auto-discovery
- **OAuth Login** — device code flow via keyring
- **Config Health** — auto-check + auto-fix
- **ThemeProvider** — 12 themes, light/dark/system, radius toggle
- **AgentMarkdown** — markdown rendering with syntax highlighting
- **Gateway Control** — start/stop/monitor gateway
- **Tools Manager** — toggle tools + category filter
- **Diagnose Screen** — health dashboard + auto-fix
- **Versions** — app/tauri/rust/OS info
- **Credential Pool** — keyring-based credential storage

## Architecture

Steersman Desktop is a single-user Tauri application: the React frontend calls a Rust product API, which manages conversations, runtime supervision, and integration capabilities while keeping gateway, SSH, credential, and MCP details behind backend boundaries. Local, remote, and SSH modes share the same product-facing conversation flow; credentials live in the operating system credential store with an encrypted local fallback.

| Layer | Tech |
|-------|------|
| Frontend | React 19 + TypeScript, Vite, Tailwind CSS 4, Zustand |
| Backend | Rust + Tauri 2, tokio, keyring-rs, serde, ssh2 |
| CI/CD | GitHub Actions (Linux AppImage/deb, Windows MSI) |

## Download

| Platform | Format | Latest |
|----------|--------|--------|
| Linux | AppImage | [releases](https://github.com/NikolayGusev-astra/autolycus-desktop/releases) |
| Linux | deb | [releases](https://github.com/NikolayGusev-astra/autolycus-desktop/releases) |
| Windows | NSIS / MSI | [releases](https://github.com/NikolayGusev-astra/autolycus-desktop/releases) |

## Development

### Prerequisites
- Rust 1.93.1, as pinned in `src-tauri/rust-toolchain.toml`
- Node.js 22+
- Platform dependencies required by Tauri 2. Linux contributors can use the package list in `.github/workflows/ci.yml`.

### Build
```bash
# Frontend
npm run build

# Rust
cd src-tauri && cargo check

# Full build via Tauri
cargo tauri build
```

### Quick start

```bash
npm ci
npm run dev
# In a second terminal
npm run tauri dev
```

Choose Local mode on the connection screen, start the gateway, then create a conversation and send a message.

### Configuration locations

The application resolves Hermes data from `HERMES_HOME` when set, otherwise from `%LOCALAPPDATA%\hermes` on Windows or `~/.hermes` on Unix-like systems. `desktop.json`, `config.yaml`, `.env`, integration data, and session state live there. OS-managed credentials hold remote and integration secrets; when the platform credential service is unavailable, integration secrets use an encrypted local fallback.

### Integration setup

Open Integrations, select Jira, enter its server URL and credentials, enable the required capabilities, and run the integration test action. Restart the application once to confirm the enabled integration recovers successfully. Use a test project while validating a release.

### Known limitations

- Some product features still use in-memory repositories and do not persist across restarts.
- Steersman Desktop is a single-user desktop application. It does not provide multi-user tenancy or shared-server administration.

### CI
```bash
# Trigger release build
gh workflow run release.yml
```

## Porting Status

All 20 features ported from [fathah/hermes-desktop](https://github.com/fathah/hermes-desktop):

| Sprint | Features | Release |
|--------|----------|---------|
| 1 | auth.rs, credential pool, OAuth, keyring | v0.7.0 |
| 2 | config_health.rs, ConfigHealthBanner | v0.7.0 |
| 3 | ThemeProvider (12 themes), AgentMarkdown, constants.ts | v0.8.0 |
| 4 | DiagnoseScreen, auto_fix_config_cmd | v0.8.0 |
| 5 | Versions, GatewayScreen, ToolsScreen, useDiscoveredModels, InstallScreen | v0.8.0 |
| 6 | ConnectScreen v2: Local/Remote/SSH, auto-discovery | v0.8.1 |

## License

MIT
