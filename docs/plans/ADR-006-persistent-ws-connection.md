# ADR-006: Persistent WebSocket Connection

> **Статус:** принято
> **Дата:** 2026-07-17
> **Решение:** Заменить connect-per-message на одно долгоживущее
> WebSocket-соединение на весь lifetime приложения. Один reader task держит
> сокет, command handlers отправляют `prompt.submit` через mpsc-канал.
> Session_id хранится в `AppState.ws`.

---

## Контекст

ADR-004 (Phase 0) ввёл WS-транспорт, но как **connect-per-message**: каждое
сообщение открывает сокет → handshake → `session.create` → `prompt.submit` →
`read_events` → `close` (`ws_transport.rs:83-114`).

Инспекция рабочего Hermes Desktop (`apps/shared/src/json-rpc-gateway.ts`)
показала, что reference implementation держит **одно persistent соединение**
на весь lifetime приложения:

| Аспект | Reference (рабочий) | ADR-004 Phase 0 (наш) |
|---|---|---|
| Соединение | Одно persistent, `connect()` один раз при boot (`json-rpc-gateway.ts:96-186`) | Connect-per-message (`ws_transport.rs:83`) |
| Event listener | Регистрируется один раз, живёт вечно (`use-gateway-boot.ts:361`) | Читается в loop пока сокет открыт, умирает с ним |
| Reconnect | Backoff 1s→2s→4s→15s, `reconnectNow()` на wake/online | Нет |
| Точек отказа на ход | 1 (только `prompt.submit`) | 5 (connect + handshake + create + submit + read) |

Каждый повторяющийся баг «фронт не обращается к бэку» — симптом этой
архитектуры: 5 точек отказа вместо 1.

---

## Контракт

### Архитектура

```
AppState
  └── ws: WsState   (Arc<Mutex<...>> внутри)

WsState {
    state:    Mutex<ConnectionState>,
    ws_url:   Mutex<String>,
    session_id: Mutex<Option<String>>,
    cmd_tx:   Mutex<Option<mpsc::Sender<WsCommand>>>,
}

ConnectionState { Disconnected, Connecting, Connected }

WsCommand {
    SubmitPrompt { session_id: String, text: String },
    CreateSession { source: String, reply: oneshot::Sender<Result<String, WsError>> },
    Shutdown,
}
```

### Reader task

Spawn при первом `ensure_ws_connection`. Единственный владелец write half:

```
loop {
    select! {
        msg = ws.next() => parse_ws_message → emit("chat_event")
        cmd = cmd_rx.recv() => match cmd {
            SubmitPrompt { .. } => ws.send(prompt.submit JSON-RPC)
            CreateSession { source, reply } => ws.send(session.create)
                → ждать response → reply.send(session_id)
            Shutdown => break
        }
    }
}
```

На socket Close/Error → `WsState.state = Disconnected`, следующий
`ensure_ws_connection` переподключается.

### Что НЕ меняется (граница контракта)

- `chat_event` Tauri channel — фронтенд не трогаем (`ChatView.tsx:65-249`)
- `ChatEvent` enum — те же варианты, те же serde теги
- `send_message_cmd` сигнатура — `Result<String, String>`, fire-and-forget
- `parse_ws_message` / `parse_gateway_event` — без изменений

### Что меняется

| Компонент | Было (ADR-004 Phase 0) | Стало (ADR-006) |
|---|---|---|
| `send_message_via_ws` | connect-per-message, открывает/закрывает сокет | deprecated, заменён на `ensure_connection` + `submit_prompt_via_channel` |
| `AppState` | нет WS-поля | + `ws: WsState` |
| `send_message_cmd` | строит URL каждый раз, вызывает `send_message_via_ws` | `ensure_ws_connection` (lazy) + `cmd_tx.send(SubmitPrompt)` |
| Event reading | `read_events` loop в каждом ходе | reader task (один на app lifetime) |

---

## Последствия

### Положительные
- 1 точка отказа вместо 5 на каждое сообщение
- Session reuse между ходами без `session.create` каждый раз
- Reconnect с backoff вместо silent failure
- Event listener живёт постоянно, ничего не теряется между ходами

### Отрицательные
- `WsState` под `Mutex` — но contention минимальный (command handlers только
  читают state, пишут через mpsc)
- Reader task может паниковать — митигация: `catch_unwind` + WsState → Disconnected
- Per-session buffer для брифинга (накапливает токены) — митигация: очистка
  после `message.complete`

### Риски
- Concurrent `CreateSession` + `SubmitPrompt` гонка → митигация: `oneshot`
  reply в `CreateSession` блокирует до session_id
- Бэкенд перезапустился (новый port/token) → митигация: `ws_url` обновляется
  при reconnect; gateway port из `GatewayState`

### Новые зависимости
Нет — `tokio` уже `features = ["full"]` (`mpsc`, `Mutex`, `oneshot`, `select!`).

---

## Верификация

| Проверка | Критерий |
|---|---|
| T1 тесты (4) | `cargo test ws_state` зелёные |
| T2 тесты (2) | `cargo test ensure_connection` зелёные |
| T3 тесты (2) | `cargo test send_message_reuses` + `send_message_creates` |
| T4 тесты (2) | `cargo test briefing_creates_briefing_smart` + `briefing_buffer` |
| T5 тесты (3) | `cargo test tool_complete_parses_duration` + `message_complete_status` + `approval_request` |
| Регрессия | `cargo test --lib` — все предыдущие тесты зелёные |
| Фронтенд | `tsc --noEmit` — 0 ошибок (контракт не меняется) |
| E2E | `cargo tauri dev` → отправить 2 сообщения подряд → второе переиспользует session_id |

---

## Ссылки

- Supersedes transport parts of ADR-004 Phase 0
- Reference: `apps/shared/src/json-rpc-gateway.ts:96-186` (persistent connect)
- Reference: `apps/desktop/src/app/gateway/hooks/use-gateway-boot.ts:452`
- Backend event contract: `tui_gateway/server.py:1140-1144` (`_emit`)
