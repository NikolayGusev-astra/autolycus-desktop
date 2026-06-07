# autolycus-desktop v0.3.0 — UI-улучшения (OpenHuman heist)

> Источник вдохновления: [tinyhumansai/openhuman](https://github.com/tinyhumansai/openhuman)
> Статус: спецификация утверждена, реализация не начата
> Дата: 2026-06-08

---

## 1. Контекст анализа

### Что сделано

Проведён полный анализ OpenHuman (GNU license, v0.57.18, 950K+ строк):
- Клонирован репозиторий, прочитаны ключевые модули: security/policy/, approval/, prompt_injection/, cwd_jail/, tokenjuice/, subconscious/, memory_tree/, agent/, composio/, mcp_server/, channels/
- Клонирован autolycus-desktop (v0.1.0), прочитаны все 35 файлов: компоненты, store, Rust core, types

### Prism-анализ (WHERE/WHEN/WHY)

**WHERE** — пробелы в нашей архитектуре:
- Rust StreamEvent: нет approval events, нет pipeline_status, нет structured tool_result
- TS types: нет ApprovalRequest, PipelineStatus, ToolResult
- GatewayStore: нет pendingApproval, pipelineStatus
- MessageList: нет рендера approval карточки
- MessageBubble/Tool: только иконка + текст, нет syntax highlighting, timing, copy
- ChatInput: нет дизейбла по agentStatus
- Header/StatusBar: дублируют друг друга, нет модели/токенов/cost

**WHEN** — боли пользователя:
- Агент запускает `terminal(rm -rf /)` → выполнится без approval (🔴 критично)
- 5 tool calls подряд → видно только `🔧 tool_name` (🟡 непонятно)
- Бэкенд падает → "отключено" в двух местах (🟢 пережить)
- Агент думает 2 мин → можно писать, сообщение потеряется (🟡 UX)

**WHY не берём остальное из OpenHuman:**
- Memory Tree → требует embedding infra, vector store. У нас session_search + memory в Hermes core
- Subconscious Engine → требует persistent scheduler + memory backend в core
- Composio 118 интеграций → managed backend зависимость, убивает opensource
- TokenJuice → Rust-only, требует правки hermes-agent core, не desktop
- CWD Jail / Landlock → per-platform код, сложно тестить → v0.4.0
- Prompt Injection Detection → regex-only, ложные срабатывания → v0.4.0
- Voice/PTT/MCP/Model Council/Web3 → не наш слой / не наша зрелость

**Вердикт: 5 пунктов достаточно для v0.3.0. Больше брать нечего.**

---

## 2. План изменений (5 пунктов)

### Пункт 1: StreamEvent expansion

**Цель:** Новые типы событий в Rust core и TS types.

**Файлы:**
- `src-tauri/src/lib.rs` — расширить `StreamEvent` enum
- `src/lib/types.ts` — добавить TS типы

**Rust (lib.rs):**
```rust
// Добавить в StreamEvent enum:
ToolResult {      // заменяет слабый tool_result
    tool_call_id, name, input, output, duration_ms, status
},
ApprovalRequest {  // approval gate
    request_id, tool_name, tool_input, action, command_class
},
ApprovalDecision { // ответ пользователя
    request_id, decision  // "approved" | "denied" | "approved_always"
},
PipelineStatus {   // для Header v2
    backend, model, tokens_used, tokens_limit, costUsd
},
```

**В parse_stream_event() добавить парсинг:**
- `"tool_result"` → StreamEvent::ToolResult
- `"approval_request"` → StreamEvent::ApprovalRequest
- `"approval_decision"` → StreamEvent::ApprovalDecision
- `"pipeline_status"` → StreamEvent::PipelineStatus

**TypeScript (types.ts):**
```ts
export interface ToolResult { tool_call_id, name, input, output, duration_ms, status }
export interface ApprovalRequest { request_id, tool_name, tool_input, action, command_class }
export interface ApprovalDecision { request_id, decision }
export interface PipelineStatus { backend, model?, tokens_used?, tokens_limit?, costUsd? }
```

**Зависимости:** Нет. Бэкенд (tui_gateway) должен начать отдавать новые event'ы — это отдельная задача.

---

### Пункт 2: ToolResult component (замена текстового tool_progress)

**Цель:** Визуализация выполнения инструментов с деталями.

**Файлы:**
- `src/components/chat/ToolResult.tsx` — новый компонент
- `src/components/chat/MessageBubble.tsx` — заменить рендеринг tool role
- `src/App.tsx` — новый кейс `"tool_result"` в handleAgentEvent

**Дизайн:**
```
┌──────────────────────────────────────────────┐
│ 🔧 read_file  ·  142ms  ·  ✅ OK            │  ← header, кликабельный
├──────────────────────────────────────────────┤
│ ▸ Input: {"path": "/root/wiki/..."}          [Copy]
│ ▸ Output:                                    [Copy]
│   LINE_NUM|CONTENT                           │
│   1!/root/wiki/README.md                     │
│   2!# OpenHuman                              │
└──────────────────────────────────────────────┘
```

**Фичи:**
- Иконки по типу инструмента (Terminal для execute_code, File для read_file, Globe для web_search)
- Input как JSON с syntax highlighting (react-syntax-highlighter — уже в зависимостях)
- Output как Markdown или `<pre>` для кнопки/Terminal output
- Copy button на input и output
- Время выполнения (durationMs)
- Для ошибок — красная рамка + crash icon
- Autoclose после 30s если done (не захламлять чат)

**Зависимости:** Пункт 1 (StreamEvent::ToolResult)

---

### Пункт 3: ApprovalCard (inline, НЕ модалка)

**Цель:** Показывать approval requests инлайн в потоке сообщений, не модалкой.

**Файлы:**
- `src/components/chat/ApprovalCard.tsx` — новый компонент
- `src/stores/gatewayStore.ts` — добавить pendingApproval + setPendingApproval
- `src/App.tsx` — кейс `"approval_request"` и `"approval_decision"`
- `src/components/chat/MessageList.tsx` — рендер ApprovalCard перед ChatInput

**Дизайн:**
```
┌──────────────────────────────────────────────────┐
│ 🛡️  Агент хочет выполнить команду               │
│                                                   │
│ [execute_code]  —  🔴 class: destructive          │
│ ─────────────────────────────────────────────────│
│ { "code": "import os; os.system('rm -rf /')" }    │
│                                                   │
│   Отклонить  ·  Разово  ·  🔁 Всегда для tool    │
└──────────────────────────────────────────────────┘
```

**Логика:**
- `approval_request` → setPendingApproval(request) → показывать ApprovalCard
- `Разово` → client.call("approval.decide", { decision: "approved" })
- `Всегда` → client.call("approval.decide", { decision: "approved_always" })
- `Отклонить` → client.call("approval.decide", { decision: "denied" })
- `approval_decision` → clearPendingApproval()
- 10-min TTL → auto-deny (опционально, v0.3.0 без TTL)

**Цвета по command_class:**
- `read` → зелёный
- `write` → жёлтый
- `network` → синий
- `install` → оранжевый
- `destructive` → красный

**Зависимости:** Пункт 1 (StreamEvent::ApprovalRequest/Decision)

---

### Пункт 4: Header v2 + StatusBar simplification

**Цель:** Убрать дублирование, добавить информацию о модели/токенах.

**Файлы:**
- `src/components/layout/Header.tsx` — расширить
- `src/components/layout/StatusBar.tsx` — упростить
- `src/stores/gatewayStore.ts` — добавить pipelineStatus

**Header v2:**
```
┌──────────────────────────────────────────────────────────┐
│ 💬 Основной  ·  🤖 claude-sonnet-4  ·  🟢 connected    │
│     12.4K/100K tokens ·  $0.023              [⏏️ Откл] │
└──────────────────────────────────────────────────────────┘
```

**StatusBar (упрощён):**
```
┌──────────────────────────────────────────────────────────┐
│ Autolycus Desktop v0.3.0  ·  mode: local               │
└──────────────────────────────────────────────────────────┘
```

**Логика:**
- `pipeline_status` event → обновлять токены/модель/cost
- `connected` → пульс + текст (без дублирования agentStatus)
- Disconnect остаётся в Header

**Зависимости:** Пункт 1 (StreamEvent::PipelineStatus)

---

### Пункт 5: ChatInput v2 — дизейбл + context

**Цель:** Дизейблить инпут пока агент занят, показывать контекст.

**Файлы:**
- `src/components/chat/ChatInput.tsx` — расширить
- `src/components/chat/ChatView.tsx` — передать agentStatus

**Дизайн:**
- `idle` → обычный инпут
- `thinking` → disabled + placeholder "Агент думает..."
- `streaming` → disabled + placeholder "Агент отвечает..."
- `tool_calling` → disabled + placeholder "Выполняет: {tool_name}"

**Зависимости:** Пункт 2 (tool_result даёт tool_name для placeholder)

---

## 3. Порядок реализации

```
#1 StreamEvent expansion (lib.rs + types.ts)
  → #2 ToolResult component (ToolResult.tsx + MessageBubble.tsx + App.tsx)
  → #3 ApprovalCard (ApprovalCard.tsx + gatewayStore + MessageList)
  → #4 Header v2 + StatusBar (Header.tsx + StatusBar.tsx + gatewayStore)
  → #5 ChatInput v2 (ChatInput.tsx + ChatView.tsx)
```

Каждый пункт — независимый коммит. Можно тестировать поэтапно.

---

## 4. Что требует изменений в бэкенде (tui_gateway)

Эти изменения НЕ входят в scope desktop v0.3.0, но нужны для работы фич:

1. **`tool_result` event** — вместо `tool_progress`, с полями: tool_call_id, name, input, output, duration_ms, status
2. **`approval_request` event** — когда tool требует approval: request_id, tool_name, tool_input, action, command_class
3. **`approval.decide` RPC** — метод для принятия решения: request_id, decision
4. **`pipeline_status` heartbeat** — периодический event с backend, model, tokens_used, tokens_limit, costUsd

---

## 5. Оценка

- **TSX:** ~400 строк (5 компонентов)
- **Rust:** ~100 строк (3 новых enum variant + парсинг)
- **TypeScript types:** ~50 строк
- **Бэкенд (tui_gateway):** ~200 строк Python (отдельная задача)
