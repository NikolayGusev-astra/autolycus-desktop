# Штурман Desktop — Спецификация разработки (SDD)
## Единый документ: требования, архитектура, состояние, план доработок

**Версия документа:** 1.2  
**Версия приложения:** 3.2.0  
**Дата:** 2026-07-08

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

## 7. ТЕХНИЧЕСКИЙ ДОЛГ (актуально на 2026-07-08)

> Предыдущая редакция §7 (6 пунктов) устарела — все перечисленные пункты уже
> решены или не актуальны. Ниже — актуальное состояние по результатам аудита кода.

### 7.1. Закрытые (проверено в коде)
- `connectionStore.fetchGatewayStatus` — корректно использует `bool`
  (connectionStore.ts:122-138), не объект. ✅
- `groupMessages` — был no-op passthrough в MessageList; **удалён** в аудите. ✅
- `detect_instances` — существует и используется (ConnectScreen.tsx:96,
  lib.rs:185). Не legacy. ✅
- `DashboardView.tsx` — удалён, висячих импортов нет (grep чист). ✅
- CSS `@import` — порядок корректен (globals.css: fonts перед tailwindcss). ✅
- Self-diagnosis modal — подключён (App.tsx:357, StatsView → add_self_check_cmd). ✅

### 7.2. Открытые (требуют работы)
1. **Документация/бренд рассинхрон.** Репозиторий GitHub — `autolycus-desktop`,
   но продукт внутри — «Steersman / Штурман Desktop». Деплой по ссылке брендирован
   «Autolycus Desktop» + «Built with Next.js + shadcn/ui» (фактический стек —
   Tauri 2 + React 19 + Vite). Каноничное имя (решено): **Steersman / Штурман**;
   внешний landing править отдельно при наличии доступа.
2. **cargo warnings** — число не подтверждено (сборка Rust недоступна в CI-среде
   аудита). Требуется `cargo check` + `clippy` и чистка unused-предупреждений.
3. **Целостность Tauri-команд** — сверить `#[tauri::command]` (lib.rs) с вызовами
   `invoke(...)` во фронте; убедиться, что все `invoke` типизированы в
   `src/lib/types.ts`.
4. **Миграции БД** — добавление полей (напр. `assignee` в tasks, см. §4 P1.1)
   требует механизма миграций `kanban-desktop.db`.

---

## 8. РЕКОМЕНДАЦИИ ПО АРХИТЕКТУРЕ (2026 best practices)

1. **Generative-UI protocol** — агент должен возвращать не только текст, но и
   structured actions (JSON), которые десктоп рендерит как кнопки/формы.
2. **Proactive briefings** — cron-job генерирует брифинг каждое утро.
3. **Override architecture** — каждое AI-действие с human-override.
4. **Connector abstraction** — единый интерфейс для всех источников (IMAP/TG/RSS).
5. **Bidirectional sync** — изменения в десктопе → Hermes и обратно.
