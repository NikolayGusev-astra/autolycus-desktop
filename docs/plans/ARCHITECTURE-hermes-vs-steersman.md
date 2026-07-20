# Архитектурный анализ: Hermes Agent vs Steersman + Дорожная карта

> **Дата:** 2026-07-18
> **Вопрос:** Что из себя представляет Hermes Agent? Что нужно для десктопного AI-ассистента руководителя? Cherry-pick или reimplement?

---

## 1. Что такое Hermes Agent (по фактам кода)

**Hermes Agent** (NousResearch) — Python-бэкенд + Electron-фронтенд. Не Tauri. Монолитный: 4 god-файла по 500-750 KB каждый (`tui_gateway/server.py` 14.5k LOC, `hermes_state.py` 315 KB, `cli.py` 757 KB, `config.py` 8.6k LOC).

### Слои

| Слой | Технология | Объём | Что делает |
|---|---|---|---|
| **TUI/CLI** | Python (curses) | ~2.5M LOC | Терминальный интерфейс — **НЕ нужен Steersman** |
| **Backend gateway** | Python (`tui_gateway/`) | 14.5k LOC | JSON-RPC over WS/stdio. ~140 RPC методов. Сессии, agent loop, tool execution |
| **Agent loop** | Python (`agent/`) | 5.5k LOC | LLM call → tool-call parse → execute → repeat (до max_iterations=90) |
| **Tools** | Python (`tools/`) | ~40k LOC | read_file, bash, write_file, MCP tools, skills, memory, code_execution |
| **Skills** | Markdown (72 шт) | docs | Процедурная память агента (инструкции как пользоваться API/CLI) |
| **Desktop frontend** | Electron + React/TS | 318 файлов | UI: chat, model picker, sessions, settings |
| **Shared contract** | TS (`apps/shared/`) | 387 LOC | `json-rpc-gateway.ts` — persistent WS клиент + event vocabulary |

### Ключевая архитектурная идея

```
Desktop (UI) ──WS JSON-RPC──► tui_gateway ──► agent loop ──► LLM
                                                  │
                                                  ├─► built-in tools (bash, read_file, ...)
                                                  ├─► MCP tools (email, jira, calendar)
                                                  ├─► skills (markdown инструкции)
                                                  └─► code_execution (агент пишет Python)
```

Backend сам выполняет ВСЁ. Desktop — тонкий клиент: отправляет prompt, получает стрим.

### Саморазвитие агента (ответ на твой вопрос)

**Да, LLM в чате может писать себе утилиты и скилы на Python.** Три механизма:

1. **`execute_code` tool** (`code_execution_tool.py`, 1.9k LOC) — LLM пишет Python скрипт, он выполняется в sandbox-подпроцессе. Скрипт может вызывать 7 инструментов (read_file, write_file, bash, web_search, ...). Агент пишет код под задачу.

2. **`skill_manage` tool** (`skill_manager_tool.py`, 1.5k LOC) — LLM создаёт/редактирует/удаляет skills (Markdown) в `~/.hermes/skills/`. Созданный skill виден в следующих сессиях.

3. **System prompt инструктирует это**: *"After completing a complex task (5+ tool calls), save the approach as a skill with skill_manage"* (`prompt_builder.py:180`). Skill-maintenance loop встроен в prompt.

**Доказательство:** в `~/.hermes/skills/` уже есть agent-created skills: `autolycus-desktop-briefing`, `smart-briefing`, `prism-3way`, `multi-agent-dev-workflow`.

---

## 2. Что у Steersman СЕЙЧАС (по фактам)

### Работает и протестировано (127+ тестов)
- **WS transport** (persistent connection, ADR-006) — chat streaming
- **Live source cards** (ADR-007) — email/jira/calendar MCP (read-only)
- **Gateway lifecycle** — spawn/stop `hermes serve`
- **Config** — YAML round-trip, HERMES_HOME resolution
- **Productivity DB** — локальные tasks/goals/projects (kanban-desktop.db)

### Стубы / сломано
- `cronjobs.rs` — config-only, НЕТ scheduler/executor (только чтение jobs.json)
- `approval_response` — отправляется как JSON-stringified chat message (хрупко, скорее сломано)
- `config_health.rs` — проверяет устаревшие endpoints

### ОТСУТСТВУЕТ для ассистента руководителя
- **Write-back действия**: reply email, mark-read, Jira transition/comment, calendar create/accept — **НЕТ**
- **Уведомления**: OS notifications, tray, push — **НЕТ**
- **Agent self-knowledge**: нет system prompt, агент НЕ знает про API Steersman, не может вызывать локальные DB — **НЕТ**
- **Task cross-linking**: только текст в брифинге, не выполняется
- **Learning**: memory.md — ручные заметки, нет preference extraction

### Архитектурный разрыв
Steersman — **read-heavy / action-light**: показывает данные и пересылает чат. Почти нет "ассистент делает действие за тебя".

---

## 3. Что нужно ассистенту руководителя

| Способность | Статус | Приоритет |
|---|---|---|
| Чат с LLM (стрим, reasoning, tool calls) | ✅ Работает | — |
| Живые карточки источников (почта/задачи/встречи) | ✅ Read-only работает | — |
| **Действия из карточек** (ответить, закрыть задачу) | ❌ Нет | **P0** |
| **Брифинг дня** (анализ + предложения) | ⚠️ Базовый | **P0** |
| **Агент знает API Steersman** (может создать задачу, отметить письмо) | ❌ Нет system prompt | **P0** |
| **Cross-linking задач↔обсуждений** | ❌ Только текст | P1 |
| **Уведомления** (новое письмо, просрочка) | ❌ Нет | P1 |
| **Learning** (предпочтения, автostatus) | ❌ Нет | P2 |
| **Агент пишет утилиты/скилы** | ✅ Backend умеет | — |

---

## 4. Cherry-pick vs Reimplement — вердикт

### НЕ cherry-pick (Python, нельзя перенести в Rust)
- Backend gateway, agent loop, tools — это **Python god-modules**. Перенос в Rust = rewrite, не cherry-pick. У нас уже есть работающий Rust transport (127 тестов).

### Cherry-pick контракт + тесты (TS, тот же стек)
- **`json-rpc-gateway.ts`** (387 LOC) — эталонный WS клиент + полная event vocabulary
- **`model-options.ts`** — контракт model.options RPC (полный каталог моделей)
- **`hermes-parity.test.ts`** — контракт-тесты (детектор расхождений)
- **state.db schema** (sessions/messages/usage) — чистая, переиспользуемая
- **Skill format** (SKILL.md + frontmatter) + bundled skills (productivity, email)
- **System prompt design** (SKILLS_GUIDANCE, self-improvement loop)

### Ключевая стратегическая развилка

**Steersman должен стать "Hermes-совместимым desktop"** — не переписывать backend, а:
1. Запускать Hermes backend как subprocess (уже делаем: `gateway.rs`)
2. Говорить с ним на его JSON-RPC контракте (WS persistent connection — уже делаем)
3. **Дать агенту инструменты Steersman** через MCP — это НОВОЕ

---

## 5. Дорожная карта (ФАЗЫ)

### ФАЗА 1: Agent знает API Steersman (P0)
**Проблема:** агент не может создавать задачи в Steersman, отмечать письма, проставлять статусы.

**Решение:** Steersman экспонирует свой API как **MCP сервер**. Тогда backend Hermes агент видит инструменты `steersman_create_task`, `steersman_mark_email_read`, `steersman_update_jira_status` — и может их вызывать.

- `src-tauri/src/steersman_mcp_server.rs` — MCP сервер на Rust (stdio), инструменты для работы с productivity.db, sessions, briefing
- Регистрация в `config.yaml mcp_servers.steersman` — агент видит его автоматически
- Agent system prompt: добавить блок о возможностях Steersman

**Результат:** агент в чате может "создай задачу на подготовку отчёта" → `steersman_create_task` → задача в kanban-desktop.db → видна в Ленте.

### ФАЗА 2: Действия из карточек (P0)
**Проблема:** карточки read-only.

**Решение:** кнопки действий в карточках → invoke Tauri команд → MCP tools (email mark_read, jira transition).

- Email card: "Ответить" / "Отметить прочитанным" → `mcp__email__mark_read`, `mcp__email__send_email`
- Jira card: "Закрыть" / "Комментарий" → `mcp__jira__jira_transition_issue`, `jira_add_comment`
- Calendar card: "Принять" (если RSVP)

### ФАЗА 3: System prompt + skills (P1)
- Скопировать bundled skills (productivity/, email/) в `~/.hermes/skills/steersman/`
- System prompt Steersman: "Ты ассистент руководителя. Вот твои инструменты: [MCP tools + steersman API]. Cross-link задачи с обсуждениями."
- Agent сможет писать скилы под повторяющиеся рабочие процессы

### ФАЗА 4: Уведомления + proactive (P1)
- Tray icon, OS notifications (новое письмо, просроченная задача, встреча через 5 мин)
- Cron-driven проверки в фоне (backend уже умеет cron)

### ФАЗА 5: Learning + memory (P2)
- Preference extraction из approval patterns ("пользователь всегда одобряет class=read")
- Memory snapshots → system prompt (Hermes уже умеет USER.md)

---

## 6. Рекомендация: не cherry-pick модули, а bridge

**Hermes backend уже запущен** (`hermes serve` через `gateway.rs`). Backend умеет:
- Agent loop с tool calling ✅
- MCP integration (email/jira/calendar) ✅
- Skills (create/edit/view) ✅
- code_execution (агент пишет Python) ✅
- Memory (MEMORY.md, USER.md) ✅
- Cron ✅

**Steersman должен не дублировать это в Rust, а:**
1. ✅ Быть тонким UI клиентом (уже есть)
2. ✅ Показывать live data (уже есть, ADR-007)
3. ❌ **Дать агенту обратный канал** — Steersman MCP сервер (ФАЗА 1)
4. ❌ **Дать UI действия** — кнопки на карточках (ФАЗА 2)

**Архитектура целиком:**
```
Steersman Desktop (Tauri/Rust/React)
  ├─ UI: Лента + Чат + Карточки
  ├─ WS persistent → Hermes backend (agent loop, tools, MCP, skills)
  ├─ Steersman MCP server ← агент вызывает steersman_create_task и т.д.
  └─ Live source cards: прямые MCP запросы (email/jira/calendar)
```

Это **максимально переиспользует** Hermes backend (вся agent логика, инструменты, саморазвитие) и добавляет только то, чего нет — UI, live cards, обратный канал.

---

## 7. Конкретные следующие шаги

| # | Задача | Фаза | Сложность |
|---|---|---|---|
| 1 | `hermes-parity.test.ts` — адаптировать контракт-тесты под Steersman | — | S (детектор багов) |
| 2 | Steersman MCP server (Rust stdio, инструменты для productivity.db) | 1 | M |
| 3 | Регистрация `mcp_servers.steersman` в config.yaml | 1 | S |
| 4 | System prompt для Steersman (briefing + capabilities) | 1 | S |
| 5 | Email actions (reply/mark_read из карточки) | 2 | M |
| 6 | Jira actions (transition/comment из карточки) | 2 | M |
| 7 | Bundled skills (productivity/email) | 3 | S |
| 8 | Tray + OS notifications | 4 | M |

**Начать с #1** (`hermes-parity.test.ts`) — дешёвый превентивный детектор багов контракта. Потом #2 (Steersman MCP server) — это откроет агенту обратный канал и сделает его настоящим ассистентом.
