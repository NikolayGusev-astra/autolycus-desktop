---
iteration: 1
max_iterations: 25
completion_promise: "PHASE_1C_4_FINAL_VERIFIED"
---

Phase 1C.4 final fixup: profile lifecycle, repeated-reconnect safety, interruption semantics. Five narrow deliverables from the fourth audit. Promise only true when ALL pass + green CI.

1. PROFILE THROUGH THE LIFECYCLE. Add `profile: Option<String>` to SessionCreateParams (skip_serializing_if None) and `profile_name: Option<String>` to SessionCreateInfo (serde default). create_session_on_connection takes a requested_profile param. chat.rs reads result.info.profile_name (falling back to requested) and registers the binding with the real ProfileId, NOT ProfileId::empty(). session.resume sends the profile from the binding. Add integration test: two conversations with the SAME durable ID but DIFFERENT profiles → two distinct resume calls with different profile params.

2. INTERRUPTED RESUMING → SUSPENDED. take_suspended_for_resume transitions Suspended→Resuming. If a disconnect happens DURING reconciliation (before resume completes), the binding is stuck in Resuming and the next reconnect never picks it up. Fix: store the resume attempt generation (Resuming { attempt_generation }) OR track it separately. On reader cleanup, Resuming bindings whose attempt_generation == my_generation go back to Suspended. In reconcile_sessions, a ConnectionLost/RpcTimeout/OutcomeUnknown error on resume returns the binding to Suspended (retryable), NOT ResumeFailed. Only genuine backend errors (4007 not found, malformed) → ResumeFailed. Integration test: gen1 active → disconnect → gen2 starts resume → server closes socket before responses → bindings back to Suspended → gen3 reconnect → both resume succeed.

3. SAFE-BY-DEFAULT RPC CLASSIFICATION. Replace the blacklist (is_idempotent via NON_IDEMPOTENT_METHODS) with an explicit allowlist. Only session.status, session.history, session.list, session.active_list, and session.resume are Safe. Everything else (session.create, session.close, prompt.submit, approvals, AND unknown future methods) is OutcomeUnknown. Unknown method defaults to OutcomeUnknown (safe-by-default).

4. CLOSED-CHANNEL OutcomeUnknown. The `.map_err(|_| GatewayClientError::ConnectionLost)??` in call_rpc converts a closed oneshot to ConnectionLost regardless of method. Change it to use interruption_error(method) so prompt.submit with a closed channel returns OutcomeUnknown. Add InterruptionCause enum (Disconnect/Timeout/SendFailure/ClosedChannel) to OutcomeUnknown so the error message is accurate, not always "interrupted by disconnect".

5. REAL INTERLEAVED ROUTING TEST. The current reconnect test submits A then B sequentially and checks only that each conversation got ≥1 event. Rewrite the mock to send genuinely interleaved events (A.delta, B.delta, A.complete, B.complete) and assert the EXACT sequence of conversation_ids.

Verification: cargo test --all-targets green, clippy:strict clean, clippy:check + lint:check no growth, fmt clean, CI success.
