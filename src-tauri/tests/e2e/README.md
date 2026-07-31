# E2E test plan

The test suite is arranged in five layers. Keep failures at the lowest layer that can prove the behavior.

| Layer | Command | Covers |
| --- | --- | --- |
| 1. Rust domain and service | `cargo test --lib` | Integration validation, credential handling, lifecycle transitions, rollback, and persistence ports. |
| 2. Rust product API | `cargo test --test product_api_e2e` | Gateway conversation flow, typed product errors, capability routing, and the fake-runtime integration lifecycle. |
| 3. Frontend store | `npm run test -- src/stores/conversationStore.test.ts` | Product event handling, streamed messages, pending interactions, and cancellation. |
| 4. Full frontend suite | `npm run test` | All frontend units and integration-style tests. |
| 5. CI gate | `cargo test --manifest-path src-tauri/Cargo.toml --all-targets && npm run typecheck && npm run test` | Cross-platform build checks, Rust tests, TypeScript checks, frontend tests, lint, and clippy ratchets. |

Run the Rust commands from `src-tauri`. Run the npm commands from the repository root. CI runs the complete gate on pushes and pull requests to `main`; see `.github/workflows/ci.yml`.

## Manual release checklist

- Configure Gmail, Jira, and the local filesystem integrations with real credentials.
- Confirm each integration reaches Ready, survives a status refresh, and exposes no credential value in the UI or logs.
- Disable and re-enable each integration. Confirm credentials remain available after disable.
- Remove an integration. Confirm its runtime stops before its credentials and instance are removed.
- Send a message through local, remote, and SSH connections. Confirm streaming text, thinking, tool status, errors, and completion render in the selected conversation only.
- Exercise approval, clarification, secret, and privilege prompts. Cancel each once and confirm its pending prompt disappears.
- Disconnect the gateway during a prompt and confirm the UI reports a typed failure without crashing.
