# ТЗ: Связывание задач, сессий, проектов и целей (Task ↔ Session Linking)

> **Дата:** 2026-07-19
> **Тип:** Feature specification (architect design)
> **Связан:** ARCHITECTURE-hermes-vs-steersman.md, ADR-008

## Контекст и проблема

Сейчас в Steersman задачи, сессии, проекты и цели — **изолированные сущности**:
- `tasks` (productivity.db) имеют `project_id`, но не связаны с чат-сессиями
- `sessions` (state.db) не имеют привязки к задачам/проектам
- Jira задачи видны в Ленте, но нельзя превратить во внутреннюю задачу
- Нет способа "открыв задачу — увидеть какие сессии и работы в рамках неё были"

Пользователь хочет:
1. Превращать Jira задачу во внутреннюю + привязывать к проекту/цели
2. Привязывать чат-сессии к задачам/проектам/целям (как теги)
3. Открыв задачу → видеть связанные сессии
4. В карточке Ленты (Jira/email) — стрелочка "создать/привязать к проекту"
5. В сессии — действие "привязать к задаче" (рядом с удалением)

## Доменная модель (principle-model-the-domain)

### Сущности (существуют)
- **Task** (productivity.db) — внутренняя задача, `id, title, status, project_id, goal_id`
- **Project** (productivity.db) — `id, title, goal_id`
- **Goal** (productivity.db) — `id, title, target_date`
- **Session** (state.db) — `id, source, started_at, title, ...`
- **JiraIssue** (внешняя) — `key, summary, status` (живёт в Jira, не у нас)

### Новая сущность: `external_refs` (источник истины для внешних ссылок)
Jira/email/Confluence сущности — внешние. Чтобы привязать их к внутренним задачам, нужен реестр внешних ссылок:

```sql
CREATE TABLE external_refs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Что это (внешний источник)
    source TEXT NOT NULL,          -- 'jira', 'email', 'confluence'
    external_id TEXT NOT NULL,     -- 'DEVOS-3', email Message-ID, Confluence page id
    external_url TEXT,             -- прямая ссылка (опционально)
    title TEXT,                    -- кешированный заголовок (для отображения)
    -- К чему привязано внутри
    task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
    project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL,
    -- Когда создано
    created_at INTEGER,
    UNIQUE(source, external_id)
);
```

**Почему отдельная таблица:** одна Jira задача может породить несколько внутренних подзадач (INT-6515 → "настроить MCP" + "научить агента искать"). `external_refs` хранит ссылку Jira→Task, а подзадачи — отдельные Task с общим `project_id`.

### Новая сущность: `session_links` (связь сессий с задачами)
```sql
CREATE TABLE session_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,      -- state.db sessions.id (внешняя БД!)
    task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
    project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
    goal_id INTEGER REFERENCES goals(id) ON DELETE CASCADE,
    linked_at INTEGER,
    linked_by TEXT,                -- 'manual' (user) | 'agent' (LLM)
    note TEXT,                     -- опциональная заметка
    -- Минимум одна привязка, можно несколько (сессия к задаче И проекту)
    CHECK (task_id IS NOT NULL OR project_id IS NOT NULL OR goal_id IS NOT NULL)
);
```

**Проблема двух БД:** `sessions` в state.db (владение Hermes backend), `tasks` в kanban-desktop.db (владение Steersman). `session_links` живёт в **kanban-desktop.db** (Steersman владеет). `session_id` — просто строка (внешний ключ, без FK constraint, т.к. другая БД).

## Пользовательские сценарии (use cases)

### UC-1: Jira → внутренняя задача + проект
**Триггер:** в Ленте, на карточке Jira → кнопка "↗ В задачу" (стрелочка).
**Flow:**
1. Открывается модал: поля Title (предзаполнено из Jira summary), Project (dropdown), Goal (dropdown), Priority
2. На submit:
   - `productivity::create_task(title, project_id, ...)`
   - `external_refs` insert: `(source='jira', external_id='DEVOS-3', task_id=<new>, title=<jira summary>)`
3. Карточка показывает бейдж "→ Task #N" (привязано)

### UC-2: Сессия → задача/проект (привязка)
**Триггер:** в списке сессий, на сессии → действие "Привязать" (рядом с "Удалить").
**Flow:**
1. Открывается модал: выбор Task / Project / Goal (autocomplete или dropdown)
2. На submit: `session_links` insert: `(session_id, task_id, linked_by='manual')`
3. Сессия показывает бейдж привязки

### UC-3: Открыть задачу → увидеть сессии
**Триггер:** клик на задачу в "Мои задачи" или Work view.
**Flow:**
1. Открывается detail view задачи
2. Запрос: `SELECT session_id FROM session_links WHERE task_id = ?`
3. Для каждой session_id: `sessions::get_session_messages` → превью
4. Сессии показаны списком с превью первого сообщения

### UC-4: Email → задача
**Триггер:** в Ленте, на email карточке → "↗ В задачу".
**Flow:** как UC-1, но `external_refs.source='email'`, `external_id=<Message-ID>`.

### UC-5: Авто-привязка агентом (future, v2)
Агент в чате обсуждает задачу INT-6515 → через `steersman_link_session` MCP tool → создаёт session_link с `linked_by='agent'`.

## Подзадачи (sequence-verifiable-units)

### L1: Schema migration — `external_refs` + `session_links`
**Файл:** `productivity.rs` (additive ALTER / CREATE IF NOT EXISTS)
**Тесты:** таблицы создаются, FK работают, UNIQUE(source, external_id) предотвращает дубли.

### L2: CRUD commands для external_refs
- `link_external_to_task_cmd(source, external_id, task_id, title)` → insert external_refs
- `create_task_from_external_cmd(source, external_id, title, project_id, goal_id, priority)` → create_task + external_refs atomically
- `get_external_ref_cmd(source, external_id)` → есть ли привязка

### L3: CRUD commands для session_links
- `link_session_cmd(session_id, task_id?, project_id?, goal_id?, note?)` → insert session_links
- `unlink_session_cmd(link_id)`
- `get_session_links_cmd(session_id)` → все привязки сессии
- `get_links_for_task_cmd(task_id)` → все сессии задачи (для UC-3)

### L4: UI — кнопка "↗ В задачу" на карточках Ленты
- Email карточка: кнопка (рядом с mark_read)
- Jira карточка: кнопка (рядом с transition)
- Модал: Title + Project dropdown + Goal dropdown + Priority
- На submit → `create_task_from_external_cmd`

### L5: UI — привязка сессии
- В списке сессий: действие "Привязать" (рядом с "Удалить")
- Модал: выбор Task/Project/Goal
- Бейдж на сессии

### L6: UI — detail view задачи со связанными сессиями
- При открытии задачи → панель "Связанные сессии"
- `get_links_for_task_cmd` → список с превью

### L7: MCP tool для агента (future)
- `steersman_link_session(session_id, task_id)` в mcp_server.rs
- `steersman_create_task_from_jira(jira_key, title, project_id)` — UC-1 через чат

## MCP tools (для агента, Фаза future)
```
steersman_link_session(session_id, task_id?, project_id?, goal_id?, note?)
steersman_create_task_from_external(source, external_id, title, project_id?, goal_id?)
steersman_get_task_sessions(task_id)  → [{session_id, title, started_at}]
```

## Архитектурные решения

### Две БД — нет cross-DB FK
`session_links` в kanban-desktop.db. `session_id` — строка без FK. При удалении сессии в state.db → session_links остаётся "висячая" (orphan). Митигация: при рендере detail view фильтровать — если session_id не найден в state.db, не показывать (мягкое удаление).

### Steersman MCP server — обратный канал для авто-привязки
Агент в чате: "обсудили INT-6515, привяжи эту сессию к задаче" → `steersman_link_session` → session_links insert. `linked_by='agent'`.

### external_refs — кеширование title
Jira summary может меняться. `external_refs.title` — кеш на момент привязки. Refresh — future (опционально, через MCP jira_get_issue).

## Границы (что НЕ входит)
- Двусторонняя синхронизация с Jira (status sync) — future
- Авто-определение "эта сессия про задачу X" (NLP) — future
- Bulk-привязка сессий — future

## Порядок реализации
1. **L1** (schema) → тесты
2. **L2** + **L3** (CRUD) → тесты
3. **L4** (UI кнопки на карточках) → самое ценное для пользователя
4. **L5** (привязка сессий)
5. **L6** (detail view)
6. **L7** (MCP tools для агента)

## Риски
| Риск | Митигация |
|---|---|
| Orphan session_links (сессия удалена) | Мягкое удаление при рендере |
| Внешний ID меняется (Jira re-key) | external_refs по source+external_id; миграция — future |
| Дубликаты привязок | UNIQUE(session_id, task_id) — один линк на пару |

---

## Известные баги (фиксы, найденные в ходе L1–L7)

### BUG-1: `create_task_from_external_cmd` падает с "missing required key externalId"
**Симптом:** При попытке перевести Jira-задачу в内部的 (`↗ В задачу`) — ошибка:
`invalid args externalId for command create_task_from_external_cmd: command missing required key externalId`.

**Корень:** Tauri конвертирует snake_case имена аргументов Rust-команд в camelCase для
JS-моста. Rust-параметр `external_id` становится `externalId` на стороне `invoke()`.
В `FeedView.tsx` (строка ~902) передаётся `external_id: createTaskModal.externalId`
(snake_case), а надо `externalId: createTaskModal.externalId`.

**Что чинить:**
- `FeedView.tsx` в вызове `invoke("create_task_from_external_cmd", {...})`:
  заменить `external_id:` на `externalId:`.
- Затронет и Email-карточку (UC-4), и Jira-карточку (UC-1).
- Контрактно: все остальные команды уже используют camelCase (`dueDate`, `projectId`,
  `goalId`, `sectionId`) — этот баг единичный, только в `create_task_from_external_cmd`.

**Тест (RED→GREEN):** добавить в `FeedView` smoke-тест или в Rust тест, что
`create_task_from_external` вызывается с аргументом `externalId` (а не `external_id`),
например мок Tauri `invoke` проверяет ключ.

### BUG-2: Сессии "теряются" при открытии из Ленты в чат
**Симптом:** Клик по сессии в каналах Ленты → открывается чат, но сообщения сессии
не показываются (пусто) или показываются не те.

**Корень (предположение, требует подтверждения):** В `App.tsx` `onOpenSession` делает
`invoke("get_session_messages_cmd", {sessionId: sid})`, маппит в `messages` и ставит
`currentSessionId`. Но `ChatView` на `currentSessionId` не перезагружает историю — он
полагается на то, что `messages` уже заполнены из стора. Если `ChatView` смонтирован
раньше (или `currentSessionId` обновляется после рендера), история не подхватывается.
Плюс: при `handleNewSession` стор сбрасывается в `[]`, и если клик пришёл до завершения
загрузки `load()`, сообщения могут затереться.

**Что чинить (план):**
- В `ChatView` добавить `useEffect` на `currentSessionId`: при смене ID — загрузить
  историю через `get_session_messages_cmd` (источник истины), а не только из стора.
- В `App.tsx` `onOpenSession` — гарантировать, что `setActiveView("chat")` вызывается
  **после** успешной загрузки сообщений (уже так, но нужна обработка ошибки без сброса).
- Убедиться, что `get_session_messages_cmd` возвращает роли `user`/`assistant` (сейчас
  фильтруется — системные/tool сообщения теряются, но это ок для превью).

**Верификация:** e2e — открыть сессию из Ленты → видим ≥1 сообщение; открыть другую →
видим её сообщения (не предыдущей).

### BUG-3: "Отметить прочитанным" в Почте — письмо исчезает и возвращается непрочитанным
**Симптом:** Клик ✓ на email карточке → письмо пропадает из Ленты, затем через ~5 мин
(или при `load()`) возвращается как непрочитанное.

**Корень (два кандидата):**
1. `mark_email_read_cmd` вызывает email MCP `mark_read`, но MCP-сервер не сохраняет флаг
   (или IMAP-флаг не выставляется — нужен `STORE +FLAGS \Seen`). Тогда `list_email_unread`
   перечитывает письмо как unread → `load()` возвращает его.
2. Фоновый `setInterval(load, 5*60*1000)` (ADR-007 hybrid refresh) перезапрашивает
   `list_email_unread_cmd` каждые 5 мин, и если флаг реально не выставлен — письмо
   "воскресает".

**Что чинить (план):**
- Проверить email MCP `mark_read`: должен делать IMAP `STORE <uid> +FLAGS (\Seen)` и
  возвращать успех только если флаг реально проставлен.
- В `FeedView` после `mark_email_read_cmd` — фильтровать `emailItems` локально (удалить
  `uid` из списка) сразу, **не дожидаясь** `load()`, чтобы не было мигания. А `load()`
  через 5 мин должен подтвердить, что письмо ушло (если MCP работает корректно).
- Добавить optimistic-update + rollback при ошибке MCP.

**Верификация:** отметить прочитанным → письмо уходит из Ленты и **не возвращается**
через 5 мин (при корректном MCP). Если MCP недоступен — показываем ошибку, не удаляем.

---

## Новая фича: Встречи → задачи + брифинг перед встречей (L8)

> **Статус:** проект (design). Интегрируется в существующую модель ADR-009:
> встреча (CalendarEvent) — это тоже «внешний источник», как Jira/email.
> Перевод встречи в задачу = `external_refs(source='calendar', external_id=<event_uid>)`
> + `tasks`. Брифинг = расширение существующего механизма Smart Briefing (ADR-008).

### Контекст
Пользователь хочет из календаря Steersman:
1. Переводить **одиночные и повторяющиеся** встречи во внутренние задачи
   (со сбором контекста встречи: участники, описание, ссылка).
2. Получать **напоминание перед встречей** + **автоматический брифинг**, который
   собирается из:
   - источника встречи (кто прислал приглашение, календарь заказчика/внутренний),
   - участников встречи,
   - задач инженера (мои активные задачи из `tasks`),
   - описания самой встречи.
3. Типовые сценарии брифинга:
   - **Дейли** → бриф по моим задачам + анализ сессий + проделанной работы.
   - **Встреча по заказчику** → в названии поиск и анализ задач, связанных с этой
     встречей/заказчиком (через `external_refs` или теги проекта).

### Доменное расширение

#### `CalendarEvent` (источник истины — calendar MCP)
Текущий интерфейс бедный: `{summary, start, end, location}`. Нужно расширить:
```ts
interface CalendarEvent {
  uid: string;            // stable ID (для external_refs.external_id)
  summary: string;
  description: string;    // тело приглашения
  start: string;          // ISO
  end: string;
  location: string;
  organizer: string;      // кто прислал (источник встречи)
  attendees: string[];    // участники
  recurring: boolean;     // повторяющаяся ли
  recurrence_rule?: string; // RRULE, если есть
}
```

#### `meeting_tasks` (связь встреча → задачи, как external_refs)
Повторяющаяся встреча (Дейли) порождает одну задачу-шаблон + экземпляры по датам,
или одну задачу с привязкой к `external_refs(source='calendar', external_id=<recurrence_id>)`.
Одиночная встреча → одна задача с `external_refs(source='calendar', external_id=<uid>)`.

#### Брифинг (расширение Smart Briefing)
Новый вход: `generate_meeting_briefing_cmd(event_uid, minutes_before=15)`:
1. Берёт встречу из calendar MCP по `uid`.
2. Классифицирует тип (дейли / заказчик / прочее) по `summary` + `organizer`.
3. Собирает контекст:
   - `list_tasks_cmd` (фильтр: мои активные, приоритет) → "что я делаю",
   - `search_sessions_cmd(query=заказчик/проект)` → анализ прошлых сессий,
   - `get_external_ref` для встречи → связанные задачи/проект,
   - `get_links_for_task` → какие сессии уже привязаны к задачам встречи.
4. LLM (Hermes) формирует бриф: что обсудить, статусы задач, открытые вопросы.

### Пользовательские сценарии (UC)

#### UC-6: Встреча → задача (одиночная)
**Триггер:** карточка Встречи в Ленте → кнопка "↗ В задачу" (как у Jira/email).
**Flow:**
1. Модал: Title (предзаполнено `summary`), Project, Goal, Priority, Due (по `start`).
2. Submit → `create_task_from_external_cmd(source='calendar', external_id=<uid>, ...)`.
3. Бейдж "→ Task #N" на карточке встречи.

#### UC-7: Повторяющаяся встреча → задачи
**Триггер:** Дейли/еженедельная встреча → та же кнопка, но с опцией
"создать задачу на каждый экземпляр" или "одну задачу-шаблон".
**Flow:**
- Если `recurring` — предложить: (a) одна задача на всю серию (`external_id=recurrence_id`)
  или (b) задача на ближайший экземпляр (`external_id=<instance_uid>`).
- Для Дейли: задача-шаблон "Дейли статус" + авто-бриф по моим задачам/сессиям.

#### UC-8: Напоминание + брифинг перед встречей
**Триггер:** за N минут до `start` встречи (настраивается, дефолт 15).
**Flow:**
1. Локальный таймер (или cron из Rust `cronjobs.rs`) проверяет календарь.
2. За N минут → генерит бриф через `generate_meeting_briefing_cmd(event_uid, N)`.
3. Показывает карточку-напоминание в Ленте: "Через 15 мин: Дейли — бриф готов"
   с кнопкой "Открыть бриф" (раскрывает текст) и "Открыть задачу/сессию".

#### UC-9: Типовой брифинг (Дейли)
**Содержание брифа:**
- Мои активные задачи (из `tasks`), отсортированные по приоритету.
- Анализ сессий за последние N дней (из `sessions` через `search_sessions`).
- Проделанная работа: задачи, переведённые в `done` за период.
- Предложение: "обсудить на Дейли: задача X заблокирована, задача Y готова".

#### UC-10: Брифинг по заказчику
**Содержание брифа:**
- В `summary`/`organizer` поиск имени заказчика/проекта.
- Через `external_refs` + `tasks.project_id` → задачи, связанные с этим заказчиком.
- `search_sessions_cmd(query=заказчик)` → какие сессии уже были по теме.
- Статусы задач, открытые вопросы, что подготовить к встрече.

### Подзадачи (sequence-verifiable-units) для L8

#### L8.1: Расширить `CalendarEvent` + calendar MCP
- `list_calendar_today_cmd` и `list_calendar_upcoming_cmd` возвращают
  `uid, description, organizer, attendees, recurring, recurrence_rule`.
- Rust `feed_sources.rs`: парсить эти поля из ответа calendar MCP.
- Тест: fixture ответа MCP → `CalendarEvent` с новыми полями.

#### L8.2: UC-6/UC-7 — кнопка "↗ В задачу" на карточке Встречи
- `FeedView.tsx`: карточка Встречи получает кнопку (рядом с временем).
- Модал: как у Jira (Title, Project, Goal, Priority, Due=start).
- Для `recurring` — доп. переключатель "серия / экземпляр".
- Submit → `create_task_from_external_cmd(source='calendar', ...)`.

#### L8.3: Напоминания (reminder engine)
- `cronjobs.rs` или новый `reminders.rs`: таймер на ближайшие встречи из календаря.
- За N минут → событие в Ленту (toast/карточка) "Встреча через N мин".
- Хранить настройку `reminder_minutes` (дефолт 15) в config.

#### L8.4: `generate_meeting_briefing_cmd`
- Новая Rust-команда: собирает контекст (задачи, сессии, external_refs) + вызывает
  Hermes LLM для брифа.
- Возвращает `{briefing_text, related_tasks: [], related_sessions: []}`.
- Классификация типа (дейли/заказчик) по `summary`+`organizer`.
- Тесты: мок Hermes, проверка что контекст собран (задачи + сессии в prompt).

#### L8.5: UI брифинга в Ленте
- Карточка-напоминание: "Через 15 мин: Дейли" + кнопка "Открыть бриф".
- Раскрывающийся бриф: текст + список связанных задач (клик → TaskDetailView)
  и сессий (клик → чат).
- Привязка брифа к задаче встречи (UC-3/L6 переиспользуется).

#### L8.6: MCP tool `steersman_create_meeting_task`
- Для агента: "создай задачу из завтрашнего Дейли" → `steersman_create_task_from_external`
  с `source='calendar'`.

### MCP tools (расширение для агента)
```
steersman_link_session(session_id, task_id?, project_id?, goal_id?, note?)
steersman_create_task_from_external(source, external_id, title, project_id?, goal_id?)
steersman_get_task_sessions(task_id)  → [{session_id, title, started_at}]
steersman_get_meeting_context(event_uid) → {tasks, sessions, briefing}  # L8 новый
```

### Архитектурные решения (L8)
- **Встреча = внешний источник** (как Jira). `external_refs.source='calendar'`.
  Переиспользуем L2/L4 — не пишем новую таблицу привязок.
- **Брифинг = расширение Smart Briefing** (ADR-008). Не новый LLM-пайплайн, а
  новый «вход» с другим контекстом (встреча вместо глобального дашборда).
- **Напоминания** — локальный таймер в Rust (cronjobs.rs), не зависят от Hermes.
  Если Hermes упал — напоминание всё равно показываем, бриф — "недоступен".

### Границы (что НЕ входит в L8)
- Двусторонняя синхронизация задач → календарь (создал задачу → появилось событие).
- Авто-приглашение участников в чат по встрече.
- Календарь заказчика (только наш календарь через calendar MCP).

### Риски (L8)
| Риск | Митигация |
|---|---|
| Calendar MCP не отдаёт attendees/organizer | Graceful: бриф без них, пометка "нет данных" |
| Повторяющаяся встреча дублирует задачи | `external_refs` UNIQUE(source, external_id); для серии `external_id=recurrence_id` |
| Hermes недоступен при генерации брифа | Показываем напоминание без брифа, кнопка "сгенерить позже" |
| Напоминание спамит | Дедуп по `event_uid` + `reminder_shown` флаг в БД |
