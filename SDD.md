# Штурман Desktop — Спецификация разработки (SDD)
## Единый документ: требования, архитектура, состояние, план доработок

**Версия документа:** 2.0  
**Версия приложения:** 3.2.0  
**Дата:** 2026-07-12

---

## 1. НАЗНАЧЕНИЕ ПРОДУКТА

«Штурман» — десктопный командный центр руководителя 2026: единое окно
управления рабочей деятельностью через AI-ассистента (Hermes Agent).

**Архитектура:**
- **Hermes Agent** = AI-движок + коннекторы (почта, Telegram, Jira, MCP, RSS).
- **Десктоп (Tauri 2 + React 19 + Rust)** = командная поверхность: фид, действия, трекинг.

**Стек:** Tauri 2, React 19, TypeScript, Tailwind CSS 4, Zustand, Rust,
SQLite (state.db Hermes + kanban-desktop.db десктопа).

---

## 2. ВСЕ ТРЕБОВАНИЯ (из истории сессий)

### 2.1. Автообнаружение и подключение
- [x] Автообнаружение локального Hermes Agent (discovery.rs)
- [x] CREATE_NO_WINDOW на всех spawn (без окон python/hermes)
- [x] Старт шлюза через hermes.exe gateway (не python -m hermes)
- [x] Readiness по HTTP /health, не stdout
- [x] Авто-подключение при наличии инстанса (shturman.ai "Подключен")
- [x] Мастер установки Hermes при отсутствии (install_hermes_cmd)
- [x] Onboarding wizard: remote vs install → provider → key → soul
- [x] 3 режима подключения: локальный, SSH-туннель, HTTP gateway API
- [x] **Профили подключения** (несколько сохранённых серверов) — ProfilesScreen + profiles.rs

### 2.2. Единый фид (главный экран)
- [x] list_feed_cmd: объединяет сессии из state.db Hermes по источникам
- [x] Карточки с иконками источников (📧/✈️/📋)
- [x] Динамические колонки по источникам
- [x] Toggle columns/list layout
- [x] Retry при пустом фиде (init timing)
- [x] Клик по карточке → загрузка конкретной сессии
- [x] Постановка задач из карточки (ListChecks → create_task)
- [x] **Generative-UI действия на карточках** (Ответить/Делегировать/Резюме) — FeedCard
- [ ] **Фильтрация по приоритету** (AI-оценка важности)

### 2.3. Брифинги
- [x] Сводный AI-брифинг (hero-блок на главном экране)
- [x] Per-source брифинги (по каждому источнику отдельно)
- [x] Кнопка генерации/обновления
- [x] **Авто-генерация брифинга при запуске** (не ждать клика) — autoBriefRef в FeedView
- [ ] **Структурированные карточки-метрики** вместо plain text

### 2.4. Чат (Ассистент)
- [x] Стриминг SSE токенов (бэкенд корректен)
- [x] Стабильный streaming ID (streamingMsgIdRef)
- [x] measureElement для динамической высоты virtualizer
- [x] Кнопка "Новая сессия" (SquarePen)
- [x] History toggle (PanelRight)
- [x] Slash-команды (/model /clear /compact /profile /help /tasks)
- [x] Очистка поля ввода при отправке (fire-and-forget invoke)
- [x] Голосовой ввод: MediaRecorder + Groq STT + Web Speech fallback
- [x] URL/медиа вложения
- [x] session_id из done-event → продолжение сессии
- [x] Приветствие из soul.md
- [ ] **Кнопка "Извлечь задачи"** на каждом ответе агента
- [ ] **Токен-счётчик в чате** (опционально через uiStore)

### 2.5. Продуктивность (Задачи/Цели/Проекты/Протоколы)
- [x] CRUD: create/list/update/delete для tasks/goals/projects
- [x] Inline-редактирование (кнопки всегда видимы)
- [x] Drill-down: Цель → Проекты → Задачи (breadcrumb + back)
- [x] project_id на tasks (assign task to project)
- [x] goal_id на projects (assign project to goal)
- [x] Прогресс целей (slider + progress bar)
- [x] Приоритеты задач (1-5, цветовая индикация)
- [x] assignee поле в задачах — **Готово** (B1)
- [x] Протоколы: форма загрузки файла/URL/текста → агент-обработка
- [x] **Sections/sub-projects** (как Todoist sections внутри проекта) — **Готово** (B9)
- [x] **Labels/теги** для кросс-проектной группировки — **Готово** (B10)
- [x] **Kanban-доска** (колонки по статусам, drag-and-drop) — **Готово** (B5)
- [ ] **Jira-синк** (двусторонняя синхронизация статусов) — **Не сделано** (B8)
- [x] **Делегирование** человеку с уведомлением — **Готово** (B6)

### 2.6. Источники (коннекторы)
- [x] SourcesTab: Telegram (bot token) + Email (IMAP/SMTP)
- [x] Запись в .env Hermes через get_env_cmd/set_env_cmd
- [x] Статус-бейджи "Настроено/Не настроено"
- [x] **Множественные инстансы** (несколько почт, TG-ботов) — **Готово** (B4)
- [ ] **RSS-ленты** (через skills + cron Hermes) — **Не сделано** (B16)
- [ ] **YouTube-каналы** (мониторинг через skills) — **Не сделано** (B16)
- [ ] **Telegram-каналы по тематике** (фильтрация) — **Не сделано** (B17)
- [x] **Управление skills/cron** из десктопа — **Готово** (B12)

### 2.7. Credential Pool
- [x] Двусторонняя синхронизация с auth.json Hermes
- [x] Чтение/запись/удаление credential pool
- [x] fingerprint (sha256:16hex) для env-source
- [x] resolve_secret (manual→access_token; env→.env)
- [x] STT ключи ищутся в credential pool + keyring + .env
- [x] **Список моделей по провайдеру** (загрузка из /v1/models) — ModelsTab

### 2.8. Настройки Hermes
- [x] GeneralTab: язык, тема (Light/Dark), токен-счётчик
- [x] AppearanceTab: System/Light/Dark, radius toggle
- [x] SoulTab: персона + редактор soul.md
- [x] ConnectionTab: local/remote/ssh
- [x] SourcesTab: Telegram + Email
- [x] CredentialsTab: CRUD credential pool
- [x] ModelsTab: provider select + model picker (/v1/models)
- [x] HermesSectionTab: agent/tts (config.yaml read/write)
- [x] GatewayTab: start/stop/status
- [x] DiagnoseTab: health check + autofix
- [x] **MCP servers** (list/add/remove) — **Готово** (B13)
- [x] **Skills management** (list/enable/disable) — **Готово** (B12)
- [x] **Cron jobs management** (list/create/pause) — **Готово** (B12)

### 2.9. Дизайн
- [x] 2 темы: Light (#f82530 на #fff) + Dark (navy #1a1a2e + coral #e94560)
- [x] shturman.ai brand tokens (border #00000014, accent #fef2f2)
- [x] Collapsible sidebar (icon+label, 300ms transition)
- [x] Frosted header (backdrop-blur-md, h-14)
- [x] 8 секций навигации
- [x] "Самодиагностика" в footer сайдбара
- [ ] **Bento-grid dashboard** вместо плоского фида
- [x] **Motion animations** (framer-motion или CSS transitions) — **Готово** (B15)
- [ ] **Confidence signaling** на AI-предложениях — **Не сделано** (B14)
- [ ] **Progressive delegation + override** UI

---

## 3. АРХИТЕКТУРА

### 3.1. Backend (Rust, 27 модулей, ~10K LOC)
```
auth.rs          — credential pool (auth.json sync)
chat.rs          — send_message (SSE streaming via gateway API)
config.rs        — resolve_hermes_home, read/write .env, config.yaml
discovery.rs     — detect_local_instances, primary_instance
gateway.rs       — start/stop gateway, health check, CREATE_NO_WINDOW
install.rs       — install_hermes_cmd (powershell/bash installer)
media.rs         — save_media_blob, read_as_data_url
memory.rs        — read/write soul.md, memory stats
productivity.rs  — tasks/goals/projects/protocols/self_checks (kanban-desktop.db)
sessions.rs      — list_sessions, get_messages, list_feed, search
skills.rs        — list_installed_skills (filtered)
ssh.rs           — SSH tunnel + remote exec
stt.rs           — transcribe_audio (Groq/OpenAI via credential pool)
```

### 3.2. Frontend (React 19, ~10K LOC)
```
App.tsx                    — SPA router (8 views + drill-down state)
views/FeedView.tsx         — главный экран (колонки + брифинги)
views/TasksView.tsx        — CRUD + project picker + drill-down
views/GoalsView.tsx        — CRUD + progress + drill-down
views/ProjectsView.tsx     — CRUD + goal picker + drill-down
views/ProtocolsView.tsx    — upload + agent processing
views/StatsView.tsx        — self-checks + sparkline
chat/ChatView.tsx          — streaming + slash + history
chat/ChatInput.tsx         — slash-commands + voice + attachments
chat/MessageList.tsx       — virtualizer + measureElement
settings/SettingsPanel.tsx — 14 tabs (general/sources/soul/models/credentials/...)
layout/Sidebar.tsx         — 8 nav items + collapse
layout/Header.tsx          — search + date + tokens + connection
```

### 3.3. Data Layer
```
state.db (Hermes)           — sessions, messages, sessions_fts
kanban-desktop.db (desktop) — tasks, goals, projects, protocols, self_checks
auth.json (Hermes)          — credential_pool (synced)
config.yaml (Hermes)        — model, personalities, display, agent, tts, ...
.env (Hermes)               — API keys, TELEGRAM_BOT_TOKEN, EMAIL_*
soul.md (Hermes)            — agent persona text
```

---

## 4. ПРОПУЩЕННЫЕ ТРЕБОВАНИЯ (анализ пробелов)

### 4.1. Критические пробелы

**A. Множественные источники** — ✅ **РЕАЛИЗОВАНО (B4)**: CRUD-таблица источников любого типа + apply_sources_to_env_cmd.

**B. Дистрибутив** — ✅ **РЕАЛИЗОВАНО (B3)**: CI/CD (release.yml) → auto-upload to Releases.

**C. Assignee** — ✅ **РЕАЛИЗОВАНО (B1)**: поле assignee в tasks + UI picker.

**D. Jira-синк** — ❌ **НЕ РЕАЛИЗОВАНО (B8)**: двусторонняя синхронизация статусов задач.

**E. Делегирование** — ✅ **РЕАЛИЗОВАНО (B6)**: кнопка "Делегировать" → create_task с assignee.

### 4.2. UX пробелы

**F. Generative-UI карточки** — ✅ **РЕАЛИЗОВАНО (B2)**: карточки с inline-действиями (Ответить/Делегировать/Резюме).

**G. Авто-брифинг** — ✅ **РЕАЛИЗОВАНО (B7)**: авто-генерация при запуске.

**H. Confidence signaling** — ❌ **НЕ РЕАЛИЗОВАНО (B14)**: нет индикатора уверенности AI.

**I. Kanban-доска** — ✅ **РЕАЛИЗОВАНО (B5)**: перетаскиваемые колонки todo/in-progress/done.

**J. Self-diagnosis modal не работает** — ✅ **РЕАЛИЗОВАНО**: подключён к add_self_check_cmd.

### 4.3. Технические пробелы

**K. Профили подключения** — ✅ **РЕАЛИЗОВАНО (B11)**: profiles.json + UI list.

**L. Skills/cron management** — ✅ **РЕАЛИЗОВАНО (B12)**: list/enable/disable skills + cron CRUD.

**M. MCP servers** — ✅ **РЕАЛИЗОВАНО (B13)**: list/add/remove MCP servers.

---

## 5. ПЛАН ДОРАБОТОК (приоритизированный)

### Приоритет 1 — Критично для MVP

| # | Задача | Объём | Зависимости |
|---|---|---|---|
| P1.1 | **Assignee в задачах** + UI picker | малый | ✅ **Готово (B1)** |
| P1.2 | **Generative-UI действия** на карточках фида | средний | ✅ **Готово (B2)** |
| P1.3 | **Self-diagnosis modal** → add_self_check_cmd | малый | ✅ **Готово** |
| P1.4 | **Дистрибутив** — GitHub Release auto-upload | малый | ✅ **Готово (B3)** |

### Приоритет 2 — Важно для полноценного продукта

| # | Задача | Объём | Зависимости |
|---|---|---|---|
| P2.1 | **Множественные источники** (CRUD таблица + skills) | большой | ✅ **Готово (B4)** |
| P2.2 | **Kanban-доска** (drag-and-drop статусы) | средний | ✅ **Готово (B5)** |
| P2.3 | **Делегирование** из карточки фида | средний | ✅ **Готово (B6)** |
| P2.4 | **Авто-брифинг** при запуске | малый | ✅ **Готово (B7)** |
| P2.5 | **Jira-синк** (двусторонний через Hermes tool) | большой | ❌ **Не сделано (B8)** |

### Приоритет 3 — Полировка и рост

| # | Задача | Объём |
|---|---|---|
| P3.1 | **Sections/sub-projects** (как Todoist) | средний | ✅ **Готово (B9)** |
| P3.2 | **Labels/теги** кросс-проектные | средний | ✅ **Готово (B10)** |
| P3.3 | **Профили подключения** (несколько серверов) | малый | ✅ **Готово (B11)** |
| P3.4 | **Skills/cron management UI** | средний | ✅ **Готово (B12)** |
| P3.5 | **MCP servers UI** | средний | ✅ **Готово (B13)** |
| P3.6 | **Confidence signaling** на AI | малый | ❌ **Не сделано (B14)** |
| P3.7 | **Motion animations** | малый | ✅ **Готово (B15)** |
| P3.8 | **RSS/YouTube** через skills | средний | ❌ **Не сделано (B16)** |
| P3.9 | **Telegram channels по теме** | средний | ❌ **Не сделано (B17)** |

---

## 6. МЕТРИКИ ПРОЕКТА

| Метрика | Значение |
|---|---|
| Версия | 3.2.0 |
| Rust LOC | ~9,900 |
| TypeScript LOC | ~9,800 |
| CSS LOC | ~490 |
| Tauri команд | ~120 |
| React views | 6 (+ SettingsPanel с 14 tabs) |
| Zustand stores | 4 |
| Rust модулей | 27 |
| i18n ключей | ~200 |
| Git коммитов | 30+ |
| SQLite таблиц (desktop) | 5 |
| Источников данных | state.db (Hermes) + kanban-desktop.db |

---

## 7. ТЕХНИЧЕСКИЙ ДОЛГ (актуально на 2026-07-12)

> ⚠️ **ВНИМАНИЕ (обновление 2026-07-15).** Инспекция живой установки Hermes
> показала, что **транспортная часть ADR-002 и ADR-003 устарела**. Реальный
> бэкенд поднимается командой `hermes serve` (WebSocket `/api/ws`, сервер #1),
> а НЕ HTTP API Server #2 (`/v1/chat/completions`, порт 8642), которого в
> актуальной установке **нет**. Следовательно:
> - пункты §7.1 #1, #2 (session_id header) и §7.2 #6 (Runs transport), #9
>   (Content-Length) описывают endpoint'ы, **которых не существует**;
> - весь чат-движок `chat.rs` (HTTP-путь) не может работать с реальным Hermes.
>
> Актуальный транспортный контракт зафиксирован в **ADR-004** (`hermes serve` +
> WS `/api/ws`). Разделы ниже сохранены как историческая запись аудита; при
> планировании работы сверяйтесь с ADR-004. Не валидные для WS-транспорта
> пункты помечены ниже знаком 🚫.
>
> **Полная ревизия.** Предыдущие §7.1/7.2 устарели. Этот раздел — результат
> полного аудита порта против ground truth из исходников `fathah/hermes-desktop`
> (оригинал) и `NousResearch/hermes-agent` (upstream). См. **ADR-002** и
> **ADR-003** для зафиксированных контрактов. Все 21 расхождение ниже найдены
> сверкой кода порта с исходниками, а не брутфорсом.

### 7.0. Источники ground truth

| Источник | Что покрыто |
|----------|-------------|
| `fathah/hermes-desktop` (оригинал Electron) | Chat transport (3 варианта), session ID, auth, gateway spawn, IPC, SSE |
| `NousResearch/hermes-agent` (upstream) | API endpoints, auth, config.yaml schema, MCP env whitelist, proxy chain |
| Локальная установка `AppData/Local/hermes` | Реальная конфигурация, .env, logs ошибок, state.db |

### 7.1. КРИТИЧЕСКИЕ расхождения (ломают core-функционал)

| # | Зона | Оригинал | Наш порт | Файл:строка | Статус (2026-07-15) |
|---|------|----------|----------|-------------|---------------------|
| 1 | **Session ID в body** 🚫 | `session_id` body-field при resume, формат `desk-<ts>-<uuid4>` | Вычисляется `_sid`, **выбрасывается** (underscore=unused) | `chat.rs:231` | SUPERSEDED — WS создаёт сессию через `session.create`; формат `desk-<ts>-<uuid>` переиспользуется в `ws_transport.rs`. Удалить с HTTP в P2.1 |
| 2 | **X-Hermes-Session-Id header** 🚫 | На каждом authed-запросе | **Никогда не отправляется** (grep: 0) | `chat.rs:263-270` | INVALID — header есть (`chat.rs:291-296`), но endpoint `/v1/chat/completions` (8642) не существует в живом Hermes. WS auth через `?token=` |
| 3 | **API_SERVER_ENABLED** 🚫 | `=true` в env gateway | **Не устанавливается** | `gateway.rs:183-188` | INVALID → VALID для spawn: `gateway.rs:180` зовёт `gateway` не `serve`; `gateway.rs:215` шлёт `API_SERVER_ENABLED` (бэкенд игнорирует). → P1.1 |
| 4 | **.env bridge** | Все ключи `.env` → в process env gateway | Только HERMES_HOME + PORT | `gateway.rs:180-215` | CLOSED — `gateway.rs:225-231` bridges все ключи read_env; process.env наследуется полностью |
| 5 | **reasoning_effort формат** 🚫 | Top-level строка `"reasoning_effort":"medium"` | Вложенный объект `reasoning:{effort,context}` | `chat.rs:244-249` | CLOSED — `chat.rs:259-263` top-level строка; закрыто в `6ce9fb4` |

**Эффект:** чат либо не работает (403 от upstream без прокси), либо теряет
контекст между сообщениями (session_id потерян).

### 7.2. ВЫСОКИЕ расхождения (значительная дивергенция)

| # | Зона | Оригинал | Наш порт | Файл:строка | Статус (2026-07-15) |
|---|------|----------|----------|-------------|---------------------|
| 6 | **Runs transport** 🚫 | `POST /v1/runs` + SSE (PREFERRED, capability-detected) | Только `/v1/chat/completions` | `chat.rs:177-322` | SUPERSEDED — функции есть (`chat.rs:565,598,740`), но `/v1/runs` нет в живом Hermes. WS даёт tool events напрямую. Удалить в P2.1 |
| 7 | **Профиль** | `--profile <name>` CLI flag | `HERMES_PROFILE_HOME` env (недокументирован) | `gateway.rs:191-194` | CLOSED — `gateway.rs:191` `cmd.arg("--profile")`, не env var |
| 8 | **Health check** 🚫 | Poll HTTP `/health` | TCP connect только | `gateway.rs:260-281` | PARTIAL — `gateway.rs:300-335` делает TCP+HTTP/1.0 GET `/health`, но `/health` → 404 на `hermes serve`. Readiness нужен через WS `gateway.ready`. → P1.4 |
| 9 | **Content-Length** 🚫 | Явный header (middleware требует) | Не отправляется | `chat.rs:263-266` | INVALID — header есть (`chat.rs:289`), но endpoint'а `/v1/chat/completions` нет. WS не использует Content-Length |
| 10 | **ALL_PROXY** | Проверяет HTTPS_PROXY→HTTP_PROXY→**ALL_PROXY** | ALL_PROXY **не проверяется** | `config.rs:494-514` | CLOSED — `config.rs:471-481` проверяет все три (+ lowercase) |

**Эффект:** теряются tool events и reasoning stream; профили могут не
активироваться; gateway может быть "half-started" при запросе.

### 7.3. СРЕДНИЕ расхождения (неправильное место/формат)

| # | Зона | Оригинал | Наш порт | Файл:строка | Статус (2026-07-15) |
|---|------|----------|----------|-------------|---------------------|
| 11 | **MCP store** | config.yaml `mcp_servers` блок | `servers.json` (JSON, другой путь) | `mcp.rs:75-80` | VALID — `mcp.rs:79` всё ещё `servers.json`. → P2.3 |
| 12 | **MCP креды** | config.yaml `mcp_servers.<name>.env:` блок | `.env` (вырезается `_build_safe_env` whitelist) | `sources.rs:223-278` | PARTIAL — `mcp.rs:330-352 sync_mcp_env_blocks` пишет env-блоки, но через line-based editor. → P2.3 (serde_yaml round-trip) |
| 13 | **Proxy config keys** | Нет в upstream (`network:{force_ipv4}` без proxy) | `500-network.proxy`, top-level `proxy:`, `model.proxy` | `config.rs:468-493` | CLOSED — fork-ключи отсутствуют (`config.rs:457` только комментарий); прокси через env |
| 14 | **cwd gateway** | `HERMES_REPO` (source checkout) | Не установлен | `gateway.rs:165` | CLOSED — `gateway.rs:197-198` `cmd.current_dir(repo)` |
| 15 | **pythonw.exe** | `pythonw.exe` (no console) на Windows | Bare `python` + CREATE_NO_WINDOW | `gateway.rs:171` | CLOSED — `gateway.rs:166-170` prefer pythonw.exe |
| 16 | **Briefing env** | Стандартный MCP `env:` блок | Hand-curated список (хак) | `briefing.rs:178-200` | CLOSED — `briefing.rs:165-178` env-fixing реализован |

**Эффект:** MCP-серверы не получают креды → брифинг показывает 0; proxy может
не найтись (fork-ключи не существуют в upstream); относительные пути gateway
резолвятся от неправильного cwd.

### 7.4. НИЗКИЕ расхождения (косметика/минорное)

| # | Зона | Оригинал | Наш порт | Файл:строка | Статус (2026-07-15) |
|---|------|----------|----------|-------------|---------------------|
| 17 | **SID format** | `desk-<timestamp>-<uuid4>` | `desk-<uuid4>` (без timestamp) | `chat.rs:233` | CLOSED — `chat.rs:241,626` `format!("desk-{}-{}", ts, uuid::Uuid::new_v4())` |
| 18 | **Proxy injection** | Inherits process.env (явной инъекции нет) | Явная инъекция HTTP_PROXY/HTTPS_PROXY | `gateway.rs:196-215` | CLOSED — process.env наследуется (нет env_clear); явной инъекции нет |
| 19 | **Config write** | `hermes config set` CLI | Line-based YAML edit (хрупкий) | `config.rs:857-942` | VALID — `mcp.rs:349` вызывает `set_yaml_block_scalars` (line-based, fragile). → P2.3 |
| 20 | **Gateway stop** | (N/A) | `child.kill()` без graceful shutdown | `gateway.rs:354-358` | CLOSED — `gateway.rs:378-407` SIGTERM→wait→kill (graceful уже есть) |
| 21 | **config_health.rs** | (N/A — desktop-only) | Stale: проверяет `sessions.db` (реально `state.db`), порт 8000 | `config_health.rs:77-85,136-144` | VALID — DB уже `state.db` (строка 92), но порт `8642` хардкожен (строка 148); модуль генерит false-positive. → P2.3 (удалить) |

### 7.5. Dead/stub code

| Компонент | Файл | Статус | Проверка 2026-07-15 |
|-----------|------|--------|---------------------|
| `send_message_via_gateway` (WS transport) | `chat.rs:326-379` | Никогда не вызывается | VALID dead — grep: 0 вызовов; заменён на `ws_transport.rs` в Phase 0. → P2.3 |
| `check_gateway_health` (HTTP /health) | `gateway.rs:393` | Определена, не используется в startup | CLOSED (не dead) — вызывается в `gateway.rs:474` `is_api_server_ready`. Аудит ошибался |
| `test_mcp_server`, `list_mcp_catalog` | `mcp.rs:233-275` | Stubs с hardcoded данными | CLOSED — не найдены в текущем mcp.rs (grep 0); уже удалены |
| `python/tcp_server.py` | `src-tauri/python/` | — | VALID dead — файл присутствует (7959 bytes), не вызывается из Rust. → P2.3 |

### 7.6. Мёртвый/устаревший config_health.rs

`config_health.rs` проверяет `sessions.db` (реальное имя `state.db`),
хардкодит порт 8000 (реальный 8642), проверяет `venv/` (реальный путь
`hermes-agent/venv/`). Модуль — leftover от ранней итерации порта и
генерирует false-positive ошибки на реальных установках.

> **Проверка 2026-07-15:** DB-имя уже исправлено на `state.db`
> (`config_health.rs:92`), но порт `8642` всё ещё хардкожен в default-конфиге
> (`config_health.rs:148`). Сам модуль проверяет endpoint'ы (`/v1/*`),
> которых нет в `hermes serve`. **Вердикт: VALID** — удалить весь модуль в
> P2.3. Модуль подключён (`lib.rs:8`).

### 7.7. Ранее закрытые пункты (из §7.1 редакции 2026-07-08)

- `connectionStore.fetchGatewayStatus` — корректно `bool` ✅
- `groupMessages` — удалён ✅
- `detect_instances` — существует, используется ✅
- `DashboardView.tsx` — удалён, висячих импортов нет ✅
- CSS `@import` — порядок корректен ✅
- Self-diagnosis modal — подключён ✅

### 7.8. План устранения (приоритизированный, актуализирован 2026-07-15)

> ⚠️ **Прежний план (P0=#1-5, P1=#6-10, ...) устарел** — он исходил из
> аудита 2026-07-12, не знавшего о WS-миграции (ADR-004). Из 21 пункта
> реальных дефектов осталось 5 VALID + 2 PARTIAL. См. `docs/plans/P-AUDIT-report.md`.

| Фаза | Пункты | Что делается | Зависимости |
|------|--------|--------------|-------------|
| **Phase 0** ✅ | — | WS-парсер `parse_ws_message` + клиент `ws_transport.rs` + флаг `STEERSMAN_TRANSPORT`. Доказано e2e на живом Hermes | выполнено |
| **P-AUDIT** ✅ | все 21 | Перепроверка против HEAD; этот отчёт | выполнено |
| **P1** | #3, #8 | Spawn `serve --port 0` (вместо `gateway`), генерация `HERMES_DASHBOARD_SESSION_TOKEN`, readiness через WS `gateway.ready`, WS как дефолт | `docs/plans/tasks/P1-ws-wiring.md` |
| **P2** | #1,#2,#6,#9,#11,#12,#19,#21 + §7.5 + tcp_server.py | Удаление ~600 строк HTTP dead code, долгоживущее WS, MCP servers.json→config.yaml, serde_yaml round-trip, удаление config_health.rs | P1 стабилен |
| **P3** | (новое) | Remote/SSH на WS (ADR-005), thiserror типизированные ошибки, tracing structured logging | P2 завершён |

---

## 8. РЕКОМЕНДАЦИИ ПО АРХИТЕКТУРЕ (2026 best practices)

1. **Generative-UI protocol** — агент должен возвращать не только текст, но и
   structured actions (JSON), которые десктоп рендерит как кнопки/формы.
2. **Proactive briefings** — cron-job генерирует брифинг каждое утро.
3. **Override architecture** — каждое AI-действие с human-override.
4. **Connector abstraction** — единый интерфейс для всех источников (IMAP/TG/RSS).
5. **Bidirectional sync** — изменения в десктопе → Hermes и обратно.
6. **MCP env-block contract** — креды в `config.yaml mcp_servers.<name>.env:`,
   не в `.env` (см. ADR-002, `_build_safe_env` whitelist).
7. **Capability detection** — `GET /v1/capabilities` перед выбором transport
   (Runs vs Chat Completions).
