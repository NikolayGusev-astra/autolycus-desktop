# SDD: Persistent WebSocket Connection (ADR-006)

> **Связанный ADR:** ADR-006-persistent-ws-connection.md
> **Дата:** 2026-07-17

## 1. Обзор

Замена connect-per-message на одно долгоживущее WS-соединение для
интерактивного чата. Брифинг остаётся на connect-per-message (разовая
batch-операция, не нуждается в low-latency persistent connection).

## 2. Компоненты

### `WsState` (`ws_transport.rs`)
Persistent state connection, хранится в `AppState.ws: Arc<WsState>`:
- `state: Mutex<ConnectionState>` — Disconnected | Connecting | Connected
- `ws_url: Mutex<String>` — полный URL (`ws://127.0.0.1:<port>/api/ws?token=<...>`)
- `session_id: Mutex<Option<String>>` — default chat session, переиспользуется
- `cmd_tx: Mutex<Option<mpsc::Sender<WsCommand>>>` — канал к reader task

### `WsCommand` (`ws_transport.rs`)
Команды от Tauri handlers к reader task:
- `SubmitPrompt { session_id, text }` — отправить prompt.submit
- `CreateSession { source, reply: oneshot::Sender }` — создать сессию, ждёт ответ
- `Shutdown` — закрыть соединение

### `reader_task` (`ws_transport.rs`)
Spawn при первом `ensure_ws_connection`. Владеет WS socket (оба halves):
```
loop {
    select! {
        msg = ws.next() => parse_ws_message → emit("chat_event")
        cmd = cmd_rx.recv() => match {
            SubmitPrompt → ws.send(prompt.submit)
            CreateSession → ws.send(session.create) → reply.send(sid)
            Shutdown → break
        }
    }
}
```
На socket close/error → `state = Disconnected`, следующий `ensure_ws_connection`
переподключается.

### `ensure_ws_connection` (`ws_transport.rs`)
Idempotent: если Connected → `Ok(())`. Если Disconnected → connect_async →
wait_for_ready → spawn reader_task → store cmd_tx → Connected.

### `send_via_ws_persistent_local` (`chat.rs`)
Local-mode chat path:
1. `build_local_ws_url` (port + token из GatewayState)
2. `ensure_ws_connection` (lazy connect)
3. Resolve session_id: frontend-supplied → cached → CreateSession("desktop")
4. `submit_prompt_on_connection`

## 3. Data Flow

```
React ChatView.tsx:326
  → invoke("send_message_cmd", { request: { text, session_id } })
    → lib.rs:801 send_message_cmd
      → chat.rs:360 send_message (Local branch)
        → chat.rs:324 send_via_ws_persistent_local
          → ws_transport::ensure_ws_connection (lazy, idempotent)
          → ws_transport::create_session_on_connection (if first turn)
          → ws_transport::submit_prompt_on_connection
            → mpsc → reader_task → ws.send(prompt.submit JSON-RPC)
              → backend tui_gateway
                → events stream back over WS
                  → reader_task: parse_ws_message → emit("chat_event")
                    → React ChatView.tsx:66 listen → switch(type) → render
```

## 4. State Machine

```
                   ensure_ws_connection
  Disconnected ──────────────────────► Connecting
       ▲                                   │
       │                                   │ connect_async + wait_for_ready
       │ reader_task exits                 │ + spawn reader_task
       │ (socket close/error)              ▼
       └───────────────────────────── Connected
                                         │
              next send_message_cmd ──────┘
              (fast path: Ok(()), no reconnect)
```

## 5. Concurrency

- **reader_task** — единственный writer в `ws.write_half`. Command handlers
  НЕ пишут напрямую в сокет.
- **mpsc channel** — command handlers → reader_task. Буфер 32 команд.
- **WsState fields** под `tokio::sync::Mutex` — contention минимальный
  (handlers только читают state/session_id, пишут через mpsc).
- **CreateSession** — `oneshot::reply` блокирует caller до session_id,
  предотвращая гонку SubmitPrompt до создания сессии.

## 6. Что НЕ меняется (граница контракта)

| Контракт | Статус |
|---|---|
| `chat_event` Tauri channel | Неизменен (ChatView.tsx не трогаем) |
| `ChatEvent` enum + serde tags | Те же варианты |
| `send_message_cmd` сигнатура | `Result<String, String>` fire-and-forget |
| `parse_ws_message` / `parse_gateway_event` | Reusable в reader task |
| `done` event → session_id propagation | Сохранён |
| Remote/SSH transport | Connect-per-message (отдельная миграция) |
| Briefing (`generate_smart_briefing_cmd`) | Connect-per-message buffered (T4 решение) |

## 7. Тестовое покрытие

| Тест | Что проверяет |
|---|---|
| `ws_state_new_starts_disconnected` | Initial state |
| `connection_state_transitions` | Disconnected→Connecting→Connected |
| `ws_state_holds_session_id` | session_id persist между lock cycles |
| `ws_command_submit_prompt_serializes_correctly` | JSON-RPC frame contract |
| `ws_command_create_session_carries_source` | oneshot reply round-trip |
| `ensure_connection_fast_path_returns_if_connected` | Idempotency |
| `ensure_connection_waits_during_connecting` | Concurrent connect guard |
| `submit_prompt_on_connection_errors_when_disconnected` | Typed error |
| `create_session_on_connection_errors_when_disconnected` | Typed error |
| `tool_complete_parses_duration_seconds` | duration_s → duration_ms |
| `tool_complete_defaults_duration_to_zero_when_absent` | Fallback |
| `approval_request_event_parsed` | approval.request wire format |

**Итого: 102 теста (90 регрессия + 12 новых), все зелёные.**
