# ADR-008: Steersman MCP Server + Executive Assistant Phases

> **Дата:** 2026-07-18
> **Связан:** ARCHITECTURE-hermes-vs-steersman.md

## Контекст

Hermes backend уже умеет: agent loop, tool calling, MCP integration, skills, code_execution, memory, cron. Steersman — read-heavy: показывает данные, но не "делает действие за тебя". Агент НЕ может создавать задачи в Steersman, отмечать письма, проставлять статусы — нет обратного канала.

## Решение

Steersman экспонирует свой API как **stdio MCP сервер**. Hermes backend видит инструменты `steersman_create_task`, `steersman_list_tasks`, `steersman_mark_email_read` и может вызывать их в чате. Это делает агента настоящим ассистентом.

---

## ФАЗА 1: Steersman MCP Server (обратный канал агенту)

### 1.1 `steersman_mcp_server.rs` — binary

**Отдельный Rust binary** (не в lib, чтобы запускаться как subprocess):
- Читает newline-delimited JSON-RPC из stdin, пишет в stdout
- `initialize` → capabilities
- `tools/list` → объявляет инструменты
- `tools/call` → вызывает Rust функции

Инструменты (wrap `productivity.rs`, `sessions.rs`):
- `steersman_list_tasks` → `productivity::list_tasks`
- `steersman_create_task` → `productivity::create_task`
- `steersman_update_task_status` → `productivity::update_task_status`
- `steersman_list_goals`, `steersman_create_goal`
- `steersman_search_sessions` → `sessions::search_sessions`

### 1.2 Регистрация в config.yaml

```yaml
mcp_servers:
  steersman:
    command: "C:/.../steersman-mcp-server.exe"
    args: []
    env:
      STEERSMAN_HOME: "~/.hermes"
```

`add_mcp_server_cmd` уже умеет это (mcp.rs). Запустить через UI или auto-register при init_app.

### 1.3 System prompt для Steersman

`AGENTS.md` в `~/.hermes/` — Hermes backend читает его и добавляет в system prompt. Опишем: "Ты ассистент руководителя в Steersman. Доступные инструменты: steersman_* (задачи/цели). MCP sources: email, jira, calendar. Cross-link задачи с обсуждениями."

### 1.4 Тесты
- JSON-RPC framing (initialize/tools-list/tools-call)
- Каждый инструмент → мок HERMES_HOME → проверить результат
- e2e: mock MCP client вызывает steersman_create_task → задача в БД

---

## ФАЗА 2: Действия из карточек (UI)

### 2.1 Email actions
Кнопки на email карточке в FeedView:
- "Ответить" → модал → `mcp__email__send_email` через stdio MCP client
- "Отметить прочитанным" → `mcp__email__mark_read`

Новые Tauri команды: `send_email_cmd`, `mark_email_read_cmd` → вызывают `mcp_client` спавн email MCP.

### 2.2 Jira actions
Кнопки на jira карточке:
- "Закрыть" → `jira_transition_issue` (status=Done)
- "Комментарий" → модал → `jira_add_comment`

Команды: `jira_transition_cmd`, `jira_comment_cmd`.

### 2.3 Тесты
- Парсеры ответов MCP для каждого действия
- UI: tsc check

---

## ФАЗА 3: System prompt + bundled skills

### 3.1 AGENTS.md
Шаблон в `src-tauri/templates/AGENTS.md` → копируется в `~/.hermes/AGENTS.md` при init. Описывает роль ассистента + доступные инструменты + формат брифинга.

### 3.2 Bundled skills
Скопировать из Hermes `skills/productivity/` и `skills/email/` в Steersman `src-tauri/templates/skills/`. При init — устанавливаются в `~/.hermes/skills/steersman/`.

### 3.3 Тесты
- AGENTS.md копируется
- skills устанавливаются (skills.rs уже умеет)

---

## ФАЗА 4: Уведомления + proactive

### 4.1 Tray icon
`tauri-plugin-system-tray` или built-in. Иконка + "X непрочитанных, Y просрочено".

### 4.2 OS notifications
`tauri-plugin-notification`. Триггеры:
- Новое письмо (poll email MCP каждые 5 мин)
- Просроченная задача (jira overdue)
- Встреча через 5 мин (calendar)

### 4.3 Тесты
- Notification command registered

---

## ФАЗА 5: Contract parity (hermes-parity.test.ts)

Адаптировать `apps/desktop/src/hermes-parity.test.ts` под Steersman:
- Проверить все RPC методы которые мы поддерживаем
- Проверить все event types которые мы парсим
- Проверить все Tauri command return types vs TS interfaces

---

## Порядок выполнения

1. **Фаза 1.1-1.2** — steersman_mcp_server binary + инструменты
2. **Фаза 1.3** — AGENTS.md + регистрация MCP
3. **Фаза 1.4** — тесты Фазы 1
4. **Фаза 5** — hermes-parity.test.ts (раньше — найдёт баги раньше)
5. **Фаза 2** — действия из карточек
6. **Фаза 3** — system prompt + skills
7. **Фаза 4** — уведомления

## Границы
- Backend логика остаётся в Hermes (agent loop, LLM, code_execution)
- Steersman = UI + MCP сервер (обратный канал) + live cards
- Не переписываем Python на Rust

## Риски
| Риск | Митигация |
|---|---|
| MCP сервер падает | Err → агент видит "tool unavailable" |
| AGENTS.md конфликтует с user файлом | Merge, не overwrite |
| Действия из карточек требуют подтверждения | Confirmation modal для write actions |
