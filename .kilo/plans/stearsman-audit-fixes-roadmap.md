# План: Steersman/Штурман Desktop — багфиксы, техдолг и фичи (SDD)

**Каноничное название:** Steersman / «Штурман Desktop» (решено пользователем).
Деплой по ссылке брендирован как «Autolycus Desktop» + «Built with Next.js» —
это ошибочные/чужие артефакты, исправляем только их, сам бренд не меняем.

**Текущая версия:** 3.2.0 (синхронизирована: `package.json`, `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`). `SDD.md` устарел (пишет 3.1.0).

---

## Контекст (верифицированный аудит)

| Что проверено | Результат |
|---|---|
| `connectionStore.fetchGatewayStatus` возвращает bool, а не объект | **Уже исправлено** (connectionStore.ts:122-138 корректно) |
| `detect_instances` legacy-команда | **Существует и используется** (ConnectScreen.tsx:96, lib.rs:185) |
| `DashboardView.tsx` удалён, ссылки остались | **Чисто**, импортов нет |
| CSS `@import` порядок (fonts перед tailwindcss) | **Корректно** (globals.css:1-2) |
| `groupMessages` — no-op passthrough | **Подтверждён мёртвый код** (MessageList.tsx:8-11) |
| Self-diagnosis modal не подключён | **Уже подключён** (App.tsx:357, StatsView.tsx:31 → add_self_check_cmd) |
| `SDD.md` §7 «Технический долг» (6 пунктов) | **Полностью устарел** — все 6 пунктов уже решены или не актуальны |
| README «Next.js + shadcn/ui» | В репо README нет; это на **деплое** по ссылке (вводит в заблуждение) |
| Бренд деплоя «Autolycus Desktop» | Конфликтует с репо; оставляем Steersman, правим только внешнее |

**Вывод:** реальных критичных багов в коде мало; основной «техдолг» — это
устаревшая документация (SDD/landing) и мелкий мёртвый код. Большой объём работ —
это roadmap фич из SDD §4 (пропущенные требования), который актуален.

---

## Часть A — Багфиксы и техдолг (верифицировано)

### A1. Реконсиляция документации
- [ ] В `SDD.md`: поднять версию 3.1.0 → 3.2.0; переписать §7 «Технический долг»
      (отметить 6 старых пунктов как решённые; оставить только актуальное).
- [ ] В `SDD.md` §2: снять `[ ]` у уже реализованного (Self-diag modal J,
      connection profiles K есть в `ProfilesScreen.tsx`, detect_instances есть).
- [ ] Исправить вводящий в заблуждение landing по ссылке: заменить
      «Built with Next.js + shadcn/ui» на «Tauri 2 + React 19 + Vite» и
      «Autolycus Desktop» → «Steersman / Штурман Desktop» (если есть доступ к
      источнику деплоя; иначе задокументировать как внешняя ошибка).

### A2. Удаление мёртвого кода
- [ ] `src/components/chat/MessageList.tsx:8-11` — убрать `groupMessages` и
      `useMemo(() => groupMessages(messages), ...)`; использовать `messages` напрямую.

### A3. Проверка сборки и чистка warnings
- [ ] Запустить `cargo check` (src-tauri) и `npm run build`; зафиксировать реальное
      число warnings (SDD утверждает «40 cargo warnings» — проверить).
- [ ] Добавить прогон `cargo clippy` и устранить предупреждения
      (unused functions/structs) в модулях Rust.
- [ ] Прогнать `tsc --noEmit` (часть `npm run build`) и убрать неиспользуемые
      импорты в `src/` (если есть по результатам сборки).

### A4. Целостность Tauri-команд
- [ ] Сверить список `#[tauri::command]` в `src-tauri/src/lib.rs` (регистрация ~2211+)
      с вызовами `invoke(...)` во фронте; найти неиспользуемые и непокрытые команды.
- [ ] Убедиться, что все `invoke` имеют типы в `src/lib/types.ts` (sync интерфейсов).

### A5. Брендинг/консистентность
- [ ] Проверить, что во всех строках UI, README и метаданных используется
      «Steersman» / «Штурман Desktop» (без упоминания Autolycus) — вынести в отдельный
      grep-проход; править только при реальном расхождении в репо.

---

## Часть B — Roadmap фич из SDD §4 (пропущенные требования)

Приоритет 1 — критично (MVP-щели пользователя):
- [x] **B1. Assignee в задачах** (SDD P1.1): ✅ Готово 2026-07-07. Схема и UI уже
      были; баг — `create_task`/`create_task_cmd` не принимали assignee (тихо
      терялся). Исправлено: `productivity.rs::create_task` + `lib.rs::create_task_cmd`
      теперь принимают `assignee: Option<String>` и пишут в БД. `tsc` зелёный.
- [ ] **B2. Generative-UI действия на карточках фида** (SDD P1.2): inline-кнопки
      Ответить/Делегировать/Резюме в `FeedView.tsx`/`FeedCard`.
- [ ] **B3. Дистрибутив / Release** (SDD P1.4): проверить `.github/workflows/release.yml`,
      auto-upload артефактов в GitHub Releases (AppImage/deb/MSI).

Приоритет 2 — важно:
- [ ] **B4. Множественные источники** (SDD P2.1): CRUD-таблица источников любого типа
      (TG/Email/RSS/YouTube) + управление skills/cron Hermes.
- [ ] **B5. Kanban-доска drag-and-drop** (SDD P2.2): статусы triage/todo/running/blocked/done
      через `@dnd-kit` (уже в зависимостях) — подключить к `KanbanBoard.tsx`.
- [ ] **B6. Делегирование из карточки фида** (SDD P2.3): зависит от B1 (assignee).
- [ ] **B7. Авто-брифинг при запуске** (SDD P2.4): `useEffect` в `FeedView` генерит брифинг
      при старте (не только по клику).
- [ ] **B8. Jira-синк** (SDD P2.5): двусторонняя синхронизация через Hermes MCP/REST.

Приоритет 3 — полировка/рост:
- [ ] **B9. Sections/sub-projects** (P3.1) · **B10. Labels/теги** (P3.2) ·
      **B11. Профили подключения UI** (P3.3, backend `profiles.rs` уже есть) ·
      **B12. Skills/cron management UI** (P3.4) · **B13. MCP servers UI** (P3.5) ·
      **B14. Confidence signaling** (P3.6) · **B15. Motion animations** (P3.7) ·
      **B16. RSS/YouTube через skills** (P3.8) · **B17. Telegram-каналы по теме** (P3.9).

---

## Риски
- B1/B6 требуют миграции SQLite (`kanban-desktop.db`) — нужен механизм миграций/upgrade.
- B4/B8/B12/B13 зависят от изучения Hermes Agent API/skills/MCP (вне репо).
- A3 `cargo check`/`build` могут превысить 2 мин на холодном билде — запускать осторожно.

## Валидация
- `cargo check` (src-tauri) без ошибок; `clippy` без warnings.
- `npm run build` (tsc + vite) без ошибок/неиспользуемых импортов.
- Все `invoke` покрыты типами; нет мёртвого кода (`groupMessages` удалён).
- SDD актуален (версия 3.2.0, §7 переписан).
- Для фич B1–B3: ручная проверка в Tauri-dev (`cargo tauri dev`) на создании задачи
  с assignee и генерации брифинга.

## Открытые вопросы
- Доступен ли исходник деплоя по ссылке для правки landing (A1)? Если нет — только документируем.
- Нужны ли миграции БД для B1 или достаточно `ALTER TABLE` с дефолтом?
