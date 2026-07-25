---
iteration: 1
max_iterations: 25
completion_promise: "PHASE_1C_4_FIXUP_VERIFIED"
---

Phase 1C.4 fixup: correct the session.resume wire contract and close the remaining gaps from the third audit. The completion promise is only true when ALL of the following are done AND verified by passing tests + green CI.

Deliverables:

1. CORRECT session.resume wire contract. Hermes reads `params.session_id` (the durable ID), NOT `stored_session_id`. It returns 4006 "session_id required" if absent. Fix SessionResumeParams to send `session_id` (the durable ID), plus optional `profile` and `cols`. Fix SessionResumeResult to read the live ID from `session_id`, durable ID from `resumed` (or `session_key`), plus message_count/messages/info. The field names MUST match the real Hermes tui_gateway/server.py contract.

2. CORRECT mock backend. The mock must: accept `session.resume` with `params.session_id`, return `{"session_id": new_live, "resumed": durable, "session_key": durable, ...}`, and return error 4006 when `session_id` is missing. The current mock uses `stored_session_id` and must be fixed so it no longer encodes the wrong schema. All existing tests using the mock must still pass.

3. DROP unknown session-scoped events. Currently the reader task creates a RoutedChatEvent with conversation_id=None for unknown sessions and emits it. For events that CARRY a session_id (session-scoped), an unknown live session must be logged and the event must NOT be emitted (early continue). Only truly global events (no session_id field) may be emitted with conversation_id=None.

4. REAL server-initiated disconnect test. The reconnect test must NOT manually set Disconnected/Shutdown/suspend_generation. Instead: the mock server closes the first WebSocket; the test waits for WsState to reach Disconnected on its own; verifies bindings became Suspended automatically; then reconnects and verifies real session.resume calls. This proves the reader task cleanup actually fires on real socket close.

5. UNIFIED interruption classification. Add a single fn interruption_error(method) that returns OutcomeUnknown for non-idempotent, RpcTimeout/ConnectionLost for idempotent. Apply it to ALL paths: call_rpc caller timeout, reader task deadline (cleanup_tick), socket disconnect, send failure, closed reply channel. The 5s cleanup_tick currently always sends RpcTimeout — fix it.

6. PROFILE-aware durable identity. Add DurableSessionRef { profile, stored_session_id } (ProfileId can be a newtype String). SessionBinding stores the profile alongside the durable ID. Reconciliation indexes by (profile, stored_session_id). session.resume sends the profile.

7. DEGRADED state for partial reconciliation. reconcile_sessions returns a ReconciliationReport { restored, failed }. ensure_ws_connection sets Connected only if all succeeded; if some failed, set a Degraded state (add ConnectionState::Degraded) so callers know not all conversations resumed. Do NOT silently go fully Connected when resumes failed.

Verification (do not emit promise until ALL pass):
- cargo test --manifest-path src-tauri/Cargo.toml --all-targets → all green, including the corrected reconnect test with REAL server disconnect.
- npm run clippy:strict → clean.
- npm run clippy:check → no growth.
- npm run lint:check → no growth.
- cargo fmt --check → clean.
- CI run on the pushed commit → success.

Do NOT emit the promise based on partial work or unit-only tests. The session.resume wire format must match the real Hermes contract (session_id field, not stored_session_id), verified by the corrected mock returning 4006 on missing session_id.
