---
iteration: 1
max_iterations: 20
completion_promise: "PHASE_1C_4_ATOMIC"
---

Phase 1C.4 atomic-transition fixup: three gaps from the sixth audit.

1. ATOMIC RUNTIME LOCK (P0). state, cmd_tx, and generation must change under ONE mutex to eliminate the TOCTOU race where ensure checks them separately, reader cleanup runs between checks, then ensure sets Connected. Introduce ConnectionRuntime { generation, state, cmd_tx } behind a single Mutex. Both ensure_ws_connection finalization and reader cleanup acquire this same lock, making Connected and Disconnected mutually exclusive atomic transitions. finalize_connection(generation, target) checks generation+cmd_tx+state atomically, then sets target or returns ConnectionLost.
   ✅ DONE

2. REMOVE Protocol(_) FROM INTERRUPTION (P1). Protocol errors (malformed response, deserialization failure) must NOT be treated as retryable interruption — they cause an infinite reconnect loop against a persistently incompatible backend. Only ConnectionLost/RpcTimeout/OutcomeUnknown are interruption; Protocol → ResumeFailed.
   ✅ DONE

3. REAL INTERLEAVED ROUTING TEST (P1). Mock must accumulate two prompt.submit requests, then send genuinely interleaved events (A.delta, B.delta, A.tool_start, B.approval_request, A.complete, B.complete). Test asserts the EXACT conversation_id+tag sequence.
   ✅ DONE

Verification: cargo test --all-targets green, clippy:strict clean, clippy:check + lint:check no growth, fmt clean, CI success.
   ✅ ALL TESTS PASS
