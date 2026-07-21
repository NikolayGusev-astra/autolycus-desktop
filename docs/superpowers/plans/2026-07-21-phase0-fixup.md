# Phase 0.1 Fixup: Hermes Protocol DTOs & Anti-Corruption Layer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Hermes protocol DTOs to match actual wire format (snake_case, correct fields) and complete the anti-corruption layer with proper typed services and ESLint enforcement.

**Architecture:** 
1. Update `hermes_protocol.rs` DTOs to use `snake_case` and match real Hermes wire format (session_id + stored_session_id + info.desktop_contract)
2. Add proper `serde(rename)` annotations where field names differ from wire format
3. Update test fixtures to match real wire format
4. Ensure all 10 hermes_protocol tests pass
5. Verify typed service layer compiles and ESLint rule works correctly

**Tech Stack:** Rust (serde, tauri), TypeScript (ESLint custom rule, typed services), Vitest

---

### Task 1: Fix SessionCreateResult DTO to match Hermes wire format

**Files:**
- Modify: `src-tauri/src/hermes_protocol.rs` (SessionCreateResult struct, lines ~150-180)
- Modify: `src-tauri/src/hermes_protocol.rs` (fixtures module, session_create_result fixture)
- Test: `src-tauri/src/hermes_protocol.rs` (tests module)

- [ ] **Step 1: Write failing test for SessionCreateResult deserialization**

```rust
#[test]
fn test_session_create_result_deserializes_real_wire_format() {
    // Real Hermes wire format has snake_case fields
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "session_id": "abc-123",
            "stored_session_id": "stored-456",
            "message_count": 0,
            "messages": [],
            "info": {
                "desktop_contract": 4
            }
        }
    }"#;
    
    let envelope: JsonRpcEnvelope<serde_json::Value> = serde_json::from_str(json).unwrap();
    let result: SessionCreateResult = serde_json::from_value(envelope.result.unwrap()).unwrap();
    
    assert_eq!(result.session_id, "abc-123");
    assert_eq!(result.stored_session_id, "stored-456");
    assert_eq!(result.message_count, 0);
    assert_eq!(result.messages.len(), 0);
    assert_eq!(result.info.desktop_contract, 4);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test test_session_create_result_deserializes_real_wire_format -- --nocapture
```
Expected: FAIL - fields don't match (camelCase vs snake_case, missing stored_session_id)

- [ ] **Step 3: Fix SessionCreateResult struct with correct serde annotations**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionCreateResult {
    pub session_id: String,
    #[serde(rename = "stored_session_id")]
    pub stored_session_id: String,
    #[serde(rename = "message_count")]
    pub message_count: usize,
    pub messages: Vec<serde_json::Value>,
    pub info: SessionCreateInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionCreateInfo {
    #[serde(rename = "desktop_contract")]
    pub desktop_contract: u32,
}
```

- [ ] **Step 4: Update fixture to match real wire format**

```rust
pub fn session_create_result_fixture() -> JsonRpcEnvelope<SessionCreateResult> {
    JsonRpcEnvelope {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::Value::Number(1.into())),
        result: Some(SessionCreateResult {
            session_id: "abc-123".to_string(),
            stored_session_id: "stored-456".to_string(),
            message_count: 0,
            messages: vec![],
            info: SessionCreateInfo { desktop_contract: 4 },
        }),
        error: None,
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd src-tauri && cargo test test_session_create_result_deserializes_real_wire_format -- --nocapture
```
Expected: PASS

- [ ] **Step 6: Run all hermes_protocol tests**

```bash
cd src-tauri && cargo test hermes_protocol -- --nocapture
```
Expected: All 10 tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/hermes_protocol.rs
git commit -m "fix: SessionCreateResult DTO matches real Hermes wire format (snake_case, stored_session_id, info.desktop_contract)"
```

---

### Task 2: Fix PromptSubmitResult DTO to match Hermes wire format

**Files:**
- Modify: `src-tauri/src/hermes_protocol.rs` (PromptSubmitResult struct)
- Modify: `src-tauri/src/hermes_protocol.rs` (fixtures module, prompt_submit_result fixture)
- Test: `src-tauri/src/hermes_protocol.rs` (tests module)

- [ ] **Step 1: Write failing test for PromptSubmitResult deserialization**

```rust
#[test]
fn test_prompt_submit_result_deserializes_real_wire_format() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 2,
        "result": {
            "session_id": "abc-123",
            "response": "Hello world",
            "tools_called": [],
            "tokens": {"input": 10, "output": 20, "total": 30}
        }
    }"#;
    
    let envelope: JsonRpcEnvelope<serde_json::Value> = serde_json::from_str(json).unwrap();
    let result: PromptSubmitResult = serde_json::from_value(envelope.result.unwrap()).unwrap();
    
    assert_eq!(result.session_id, "abc-123");
    assert_eq!(result.response, "Hello world");
    assert_eq!(result.tools_called.len(), 0);
    assert_eq!(result.tokens.total, 30);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test test_prompt_submit_result_deserializes_real_wire_format -- --nocapture
```
Expected: FAIL

- [ ] **Step 3: Fix PromptSubmitResult struct with correct serde annotations**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptSubmitResult {
    pub session_id: String,
    pub response: String,
    #[serde(rename = "tools_called")]
    pub tools_called: Vec<ToolCallInfo>,
    pub tokens: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallInfo {
    pub name: String,
    #[serde(rename = "tool_call_id")]
    pub tool_call_id: String,
    pub input: serde_json::Value,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub total: u32,
}
```

- [ ] **Step 4: Update fixture to match real wire format**

```rust
pub fn prompt_submit_result_fixture() -> JsonRpcEnvelope<PromptSubmitResult> {
    JsonRpcEnvelope {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::Value::Number(2.into())),
        result: Some(PromptSubmitResult {
            session_id: "abc-123".to_string(),
            response: "Hello world".to_string(),
            tools_called: vec![],
            tokens: TokenUsage { input: 10, output: 20, total: 30 },
        }),
        error: None,
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd src-tauri && cargo test test_prompt_submit_result_deserializes_real_wire_format -- --nocapture
```
Expected: PASS

- [ ] **Step 6: Run all hermes_protocol tests**

```bash
cd src-tauri && cargo test hermes_protocol -- --nocapture
```
Expected: All 10 tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/hermes_protocol.rs
git commit -m "fix: PromptSubmitResult DTO matches real Hermes wire format (snake_case fields)"
```

---

### Task 3: Fix GatewayEvent enum to use snake_case variant names

**Files:**
- Modify: `src-tauri/src/hermes_protocol.rs` (GatewayEvent enum and all variants)
- Modify: `src-tauri/src/hermes_protocol.rs` (fixtures for gateway events)
- Test: `src-tauri/src/hermes_protocol.rs` (tests module)

- [ ] **Step 1: Write failing test for GatewayEvent deserialization**

```rust
#[test]
fn test_gateway_event_deserializes_snake_case() {
    let json = r#"{
        "jsonrpc": "2.0",
        "method": "events.stream",
        "params": {
            "type": "tool_start",
            "tool_name": "read_file",
            "tool_call_id": "tc-123"
        }
    }"#;
    
    let envelope: JsonRpcEnvelope<GatewayEvent> = serde_json::from_str(json).unwrap();
    assert!(matches!(envelope.method, Some(ref m) if m == "events.stream"));
    
    match envelope.params {
        GatewayEvent::ToolStart { tool_name, tool_call_id } => {
            assert_eq!(tool_name, "read_file");
            assert_eq!(tool_call_id, "tc-123");
        }
        _ => panic!("Expected ToolStart variant"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test test_gateway_event_deserializes_snake_case -- --nocapture
```
Expected: FAIL - enum variants use camelCase internally

- [ ] **Step 3: Fix GatewayEvent enum with #[serde(rename_all = "snake_case")]**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayEvent {
    ToolStart {
        tool_name: String,
        #[serde(rename = "tool_call_id")]
        tool_call_id: String,
    },
    ToolEnd {
        tool_name: String,
        #[serde(rename = "tool_call_id")]
        tool_call_id: String,
        output: String,
        #[serde(rename = "duration_ms")]
        duration_ms: u64,
    },
    Streaming {
        content: String,
    },
    StreamingEnd,
    Status {
        status: String,
    },
    Error {
        message: String,
        #[serde(rename = "error_code")]
        error_code: Option<String>,
    },
    Thinking {
        content: String,
    },
    ToolResult {
        #[serde(rename = "tool_call_id")]
        tool_call_id: String,
        output: String,
        #[serde(rename = "duration_ms")]
        duration_ms: u64,
        status: String,
    },
    ApprovalRequest {
        #[serde(rename = "request_id")]
        request_id: String,
        tool_name: String,
        tool_input: String,
        action: String,
        #[serde(rename = "command_class")]
        command_class: String,
    },
    ApprovalDecision {
        #[serde(rename = "request_id")]
        request_id: String,
        decision: String,
    },
    PipelineStatus {
        backend: String,
        model: Option<String>,
        #[serde(rename = "tokens_used")]
        tokens_used: Option<u64>,
        #[serde(rename = "tokens_limit")]
        tokens_limit: Option<u64>,
        #[serde(rename = "cost_usd")]
        cost_usd: Option<f64>,
    },
}
```

- [ ] **Step 4: Update all gateway event fixtures to use snake_case**

```rust
pub fn gateway_ready_event_fixture() -> JsonRpcEnvelope<GatewayEvent> {
    JsonRpcEnvelope {
        jsonrpc: "2.0".to_string(),
        method: Some("events.stream".to_string()),
        params: Some(GatewayEvent::Status {
            status: "gateway_ready".to_string(),
        }),
        id: None,
        error: None,
    }
}

pub fn gateway_tool_start_event_fixture() -> JsonRpcEnvelope<GatewayEvent> {
    JsonRpcEnvelope {
        jsonrpc: "2.0".to_string(),
        method: Some("events.stream".to_string()),
        params: Some(GatewayEvent::ToolStart {
            tool_name: "read_file".to_string(),
            tool_call_id: "tc-123".to_string(),
        }),
        id: None,
        error: None,
    }
}
// ... update all other fixtures similarly
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd src-tauri && cargo test test_gateway_event_deserializes_snake_case -- --nocapture
```
Expected: PASS

- [ ] **Step 6: Run all hermes_protocol tests**

```bash
cd src-tauri && cargo test hermes_protocol -- --nocapture
```
Expected: All 10 tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/hermes_protocol.rs
git commit -m "fix: GatewayEvent enum uses snake_case variant names matching Hermes wire format"
```

---

### Task 4: Fix JSON-RPC request DTOs (SessionCreateRequest, PromptSubmitRequest)

**Files:**
- Modify: `src-tauri/src/hermes_protocol.rs` (request structs)
- Test: `src-tauri/src/hermes_protocol.rs` (tests module)

- [ ] **Step 1: Write failing test for SessionCreateRequest serialization**

```rust
#[test]
fn test_session_create_request_serializes_to_snake_case() {
    let req = SessionCreateRequest {
        source: "desktop".to_string(),
        model: Some("gpt-4".to_string()),
        system_prompt: Some("You are helpful".to_string()),
    };
    
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["source"], "desktop");
    assert_eq!(json["model"], "gpt-4");
    assert_eq!(json["system_prompt"], "You are helpful");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test test_session_create_request_serializes_to_snake_case -- --nocapture
```
Expected: FAIL - fields serialize as camelCase

- [ ] **Step 3: Fix request structs with snake_case serialization**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SessionCreateRequest {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "system_prompt")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PromptSubmitRequest {
    pub session_id: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "system_prompt")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tools")]
    pub tools: Option<Vec<ToolSpec>>,
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd src-tauri && cargo test test_session_create_request_serializes_to_snake_case -- --nocapture
```
Expected: PASS

- [ ] **Step 5: Run all hermes_protocol tests**

```bash
cd src-tauri && cargo test hermes_protocol -- --nocapture
```
Expected: All 10 tests pass

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/hermes_protocol.rs
git commit -m "fix: JSON-RPC request DTOs serialize to snake_case matching Hermes wire format"
```

---

### Task 5: Verify full CI passes (Rust + TypeScript)

**Files:**
- None (verification only)

- [ ] **Step 1: Run cargo fmt check**

```bash
cd src-tauri && cargo fmt --check
```
Expected: PASS (no output)

- [ ] **Step 2: Run cargo clippy (warnings only, not -D)**

```bash
cd src-tauri && cargo clippy --all-targets 2>&1 | tail -20
```
Expected: No new errors (pre-existing warnings OK)

- [ ] **Step 3: Run all Rust tests**

```bash
cd src-tauri && cargo test --all-targets 2>&1 | tail -30
```
Expected: 166+ tests pass

- [ ] **Step 4: Run TypeScript typecheck**

```bash
cd /c/Users/n.gusev/ZCodeProject/autolycus-desktop && npm run typecheck
```
Expected: PASS (no output)

- [ ] **Step 5: Run ESLint (should flag component violations, not service/store/hook files)**

```bash
cd /c/Users/n.gusev/ZCodeProject/autolycus-desktop && npm run lint 2>&1 | head -50
```
Expected: Errors only in component files (App.tsx, ConnectScreen.tsx, etc.), NOT in src/services/, src/hooks/, src/stores/, src-tauri/

- [ ] **Step 6: Run frontend tests**

```bash
cd /c/Users/n.gusev/ZCodeProject/autolycus-desktop && npm run test
```
Expected: PASS

- [ ] **Step 7: Commit any remaining changes**

```bash
git add -A
git commit -m "ci: verify full CI passes after Phase 0.1 fixup"
```

---

### Task 6: Push to main

**Files:**
- None

- [ ] **Step 1: Verify current branch is main**

```bash
git branch --show-current
```
Expected: main

- [ ] **Step 2: Push to origin**

```bash
git push origin main
```
Expected: Success

- [ ] **Step 3: Verify CI passes on GitHub Actions**

Check: https://github.com/NikolayGusev-astra/autolycus-desktop/actions

Expected: All workflows green

---

**Plan complete.** Phase 0 is now truly done with correct Hermes wire format DTOs.