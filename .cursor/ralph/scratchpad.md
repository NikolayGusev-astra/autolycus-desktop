---
iteration: 1
max_iterations: 25
completion_promise: "PHASE_1C_4_RECONCILIATION_VERIFIED"
---

Implement Phase 1C.4: real reconnect reconciliation and event routing for the persistent Local gateway client. The audit found three P0 gaps and two P1 gaps that must be closed.

Deliverables (all must be done AND verified by passing tests + green CI before emitting the completion promise):

1. FIX DISCONNECT SUSPEND. The reader task's disconnect cleanup must suspend the bindings of the CURRENT generation (== my_generation), not just older ones. Add `suspend_generation(dead_generation: u64)` to SessionRegistry that matches connection_generation == dead_generation. Call it (or suspend_all) in reader_task cleanup. The existing mark_stale_for_generation stays for the cross-generation case. Write a NEW integration test that actually closes the mock socket and verifies the registry transitions the right binding to Suspended — not a unit test that manually calls the registry method with a fabricated generation.

2. REAL session.resume RPC. Add SessionResumeParams { stored_session_id } and SessionResumeResult { session_id, stored_session_id, ... } to hermes_protocol.rs. Add a typed wrapper `resume_session_on_connection(ws_state, stored_id) -> Result<SessionResumeResult, WsError>` in ws_transport.rs using call_rpc with method "session.resume".

3. RECONCILIATION BEFORE Connected. In ensure_ws_connection, AFTER the compatibility handshake passes and BEFORE setting ConnectionState::Connected: enumerate the registry's suspended bindings that have a stored_session_id, call session.resume for each, update the binding with the new live ID via set_live (same stored ID, new generation). Add SessionState::Resuming and ResumeFailed variants. If a resume fails, leave that binding in ResumeFailed (do NOT mark Active). Update the reconnect test to exercise this against the mock backend (two real session.resume calls returning new live IDs).

4. EVENT ROUTING via registry. Add a RoutedChatEvent { conversation_id, event } envelope. The reader task must call registry.route_event(live_session_id) for EVERY event (not just session.info), wrap the ChatEvent with the resolved conversation_id, and emit RoutedChatEvent. Unknown live sessions are logged and NOT emitted to a random conversation. Update make_tauri_emitter / the event channel name or payload to carry conversation_id. Frontend payload must include conversation_id for every event.

5. REGISTRY AUTHORITATIVE. Remove the global WsState.session_id field (or make it clearly deprecated/private with no production reads/writes). The persistent local path must register EVERY created session in the registry. The legacy request.session_id path must still work but via an adapter: create a synthetic ConversationId from the session_id and register it, so the registry always has a binding for any active session. Update or remove the old test that asserts the global session_id is cached.

6. OutcomeUnknown coverage. Store idempotency in PendingRequest (or look it up from the method name) and classify OutcomeUnknown consistently for: disconnect, RPC timeout, send failure. call_rpc must return OutcomeUnknown for non-idempotent methods on timeout, not just RpcTimeout. Update tests accordingly.

Verification (do not emit the promise until ALL pass):
- cargo test --manifest-path src-tauri/Cargo.toml --all-targets → all green, including a NEW reconnect integration test that: creates 2 sessions, disconnects the socket, reconnects, verifies 2 real session.resume calls happened, verifies new live IDs in registry, verifies interleaved events route to correct conversation_id.
- npm run clippy:strict → clean (ws_transport.rs zero warnings).
- npm run clippy:check → no new warnings beyond baseline.
- npm run lint:check → no new violations beyond baseline.
- cargo fmt --manifest-path src-tauri/Cargo.toml --check → clean.
- CI run on the pushed commit concludes success.

Do NOT claim completion based on partial work. If a deliverable is only unit-tested but not integration-tested against the mock backend, it is not done.
