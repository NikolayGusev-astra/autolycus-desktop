# Autolycus Desktop

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

| Layer | Tech |
|-------|------|
| Frontend | React 19 + TypeScript, Vite, Tailwind CSS 4, Zustand |
| Backend | Rust + Tauri 2, tokio, keyring-rs, serde, ssh2 |
| CI/CD | GitHub Actions (Linux AppImage/deb, Windows MSI) |

## Download

| Platform | Format | Latest |
|----------|--------|--------|
| Linux | AppImage | [v0.8.1](https://github.com/NikolayGusev-astra/autolycus-desktop/releases/download/v0.8.1/Autolycus.Desktop_0.8.1_amd64.AppImage) |
| Linux | deb | [v0.8.1](https://github.com/NikolayGusev-astra/autolycus-desktop/releases/download/v0.8.1/Autolycus.Desktop_0.8.1_amd64.deb) |
| Windows | MSI | [v0.8.1](https://github.com/NikolayGusev-astra/autolycus-desktop/releases/download/v0.8.1/Autolycus.Desktop_0.8.1_x64_en-US.msi) |

## Development

### Prerequisites
- Rust 1.71+
- Node.js 20+
- Tauri CLI: `cargo install tauri-cli`

### Build
```bash
# Frontend
npm run build

# Rust
cd src-tauri && cargo check

# Full build via Tauri
cargo tauri build
```

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
