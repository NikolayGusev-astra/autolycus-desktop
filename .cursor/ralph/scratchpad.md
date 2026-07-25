---
iteration: 1
max_iterations: 25
completion_promise: "PHASE_1C_4_COMPLETE"
---

Phase 1C.4 final-2 fixup: lifecycle race fix, attempt_generation, real gen3 recovery test, real interleaving + InterruptionCause. Four deliverables from the fifth audit.

1. LIFECYCLE RACE FIX. ensure_ws_connection must NOT set Connected after an interruption during reconciliation. Add `interrupted: Vec<ConversationId>` to ReconciliationReport. reconcile_sessions pushes interrupted conversations there (not just return_to_suspended). After reconciliation, if !interrupted.is_empty() OR the socket died (generation mismatch / cmd_tx is None / state==Disconnected), return Err(ConnectionLost) instead of overwriting Connected. Never set Connected after the reader task has exited.

2. ATTEMPT_GENERATION for Resuming. SessionState::Resuming { attempt_generation: u64 }. take_suspended_for_resume(generation) records the attempt generation. suspend_generation(dead_generation) handles both Active(connection_generation==dead) AND Resuming(attempt_generation==dead). This lets reader cleanup correctly identify which Resuming bindings belong to the dead connection.

3. REAL gen3 RECOVERY TEST. A genuine integration test: gen1 active → disconnect → gen2 starts real session.resume → gen2 socket closes before responses → bindings return to Suspended → gen3 reconnects → both resume succeed. NO manual take_suspended/suspend_generation calls — the mock must close the socket during resume and the reader task cleanup must fire naturally.

4. REAL INTERLEAVING + InterruptionCause. Mock must accumulate two prompt.submit requests, then send genuinely interleaved events (A.delta, B.delta, A.complete, B.complete) and the test asserts the EXACT conversation_id sequence. Also add InterruptionCause enum (Disconnect/Timeout/SendFailure/ClosedChannel) to OutcomeUnknown so Display is accurate. Safe methods get ConnectionLost on disconnect (not RpcTimeout), RpcTimeout on timeout.

Verification: cargo test --all-targets green, clippy:strict clean, clippy:check + lint:check no growth, fmt clean, CI success.
