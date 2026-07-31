# Release checklist

## Pre-release

- [ ] `cargo test --all-targets` passes.
- [ ] `cargo clippy --lib -q` and `cargo fmt --check` are clean.
- [ ] Review warnings and remove dead code.
- [ ] Scan the diff and diagnostics for secret values.
- [ ] Run real Hermes E2E tests against a live gateway:
      `cargo test --manifest-path src-tauri/Cargo.toml --test real_hermes_e2e -- --ignored --test-threads=1`

## Build

- [ ] Run `cargo build --release` in `src-tauri`.
- [ ] Run `npm run build` from the repository root.
- [ ] Verify both binaries exist: `steersman-desktop(.exe)` and `steersman-mcp-server(.exe)` in `target/release/`.
- [ ] Verify installers contain both binaries (NSIS, MSI, AppImage, DMG).

## Smoke test

- [ ] Launch the packaged application.
- [ ] Confirm the gateway starts and reports ready.
- [ ] Create a conversation and send a test message.
- [ ] Register the Steersman MCP server (Settings → MCP → Register Steersman).
- [ ] Verify the agent can call `steersman_*` tools (e.g., create a task from chat).

## Integration test

- [ ] Configure the Jira integration with non-production credentials.
- [ ] Verify its configured state and recovery after an application restart.

## Security

- [ ] Confirm logs contain no secrets.
- [ ] Confirm product DTOs contain no MCP types.
- [ ] Confirm new UI components do not call Tauri `invoke` directly.

## Rollback

- [ ] Install the previous released version over a copy of real user data.
- [ ] Confirm conversations, configuration, and integration data restore correctly.
