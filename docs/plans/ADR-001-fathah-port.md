# ADR-001: Портирование hermes-desktop функциональности в Tauri

> **Статус:** предложено
> **Дата:** 2026-06-09
> **Решение:** Принято

---

## Контекст

autolycus-desktop v0.3.0 — минимальный прототип. Работает только локально, только с одним профилем, без session management, без remote/SSH подключений. Чат использует примитивный JSON-RPC протокол, который не соответствует реальному API hermes gateway.

fathah/hermes-desktop v0.5.8 — полноценный клиент с 3 режимами подключения, gateway management, session management, per-profile изоляцией. 22K+ строк TypeScript в main процессе Electron.

Нам нужно заставить autolycus-desktop работать с удалённым hermes-сервером (NL, 147.90.10.50) и локально.

---

## Варианты

### Вариант A: Полный переход на Electron

**Описание:** Выбросить Tauri, переписать всё на Electron + код fathah.

**Плюсы:**
- Готовый код fathah — копируем и адаптируем
- Больше готовых решений (better-sqlite3, ssh2, etc.)
- Зрелая экосистема

**Минусы:**
- Бандл 150-250 MB (vs 5-15 MB у Tauri)
- RAM 150-400 MB (vs 30-80 MB у Tauri)
- Теряем все преимущества Tauri
- Полный rewrite frontend (другой способ IPC)
- Нужен Node.js вместо Rust toolchain

**Вердикт:** ❌ Отклонено. Потеряем все преимущества Tauri без реальной выгоды.

---

### Вариант B: Портирование логики в Tauri (выбран)

**Описание:** Оставить Tauri 2 + React. Портировать логику fathah в Rust модули. Frontend адаптировать под новый API.

**Плюсы:**
- Сохраняем преимущества Tauri (размер, RAM, безопасность)
- Переиспользуем React frontend (80% кода остаётся)
- Rust backend — типобезопасный, быстрый
- Контроль над архитектурой

**Минусы:**
- Нужно переписать TS → Rust (~3200 строк)
- Некоторые библиотеки менее зрелые (rusqlite vs better-sqlite3)
- Дольше чем вариант A

**Вердикт:** ✅ Принято. Оптимальный баланс усилий и результата.

---

### Вариант C: Гибрид — Electron main + Tauri renderer

**Описание:** Использовать Electron main процесс от fathah + наш Tauri renderer.

**Плюсы:**
- Минимум работы по backend

**Минусы:**
- Два фреймворка — кошмар поддержки
- Несовместимые IPC модели
- Баги на стыке гарантированы

**Вердикт:** ❌ Отклонено. Архитектурный абсурд.

---

### Вариант D: Минимальные патчи v0.3.0

**Описание:** Только добавить remote mode, остальное как есть.

**Плюсы:**
- Быстро (~500 строк)

**Минусы:**
- Не решает проблему gateway management
- Не решает проблему session management
- Технический долг растёт
- Придётся переделывать через релиз

**Вердикт:** ❌ Отклонено. Полумера, которая заблокирует развитие.

---

## Решение

Принимаем **Вариант B**: портируем логику fathah/hermes-desktop в Tauri-архитектуру.

### Ключевые решения:

1. **Tauri 2 остаётся** — не меняем платформу
2. **Rust backend** — пишем аналоги fahah-модулей на Rust
3. **React frontend** — переиспользуем 80%, добавляем новые компоненты
4. **3 режима подключения** — local, remote, SSH (как у fathah)
5. **Единый ChatEvent протокол** — абстрагирует режим от frontend
6. **rusqlite** — для доступа к state.db (вместо better-sqlite3)
7. **ssh2-rs** — для SSH tunnel и remote execution
8. **reqwest** — для HTTP API (remote mode)

### Миграционный план:

**Phase 1: Foundation** (критичный минимум)
- gateway.rs — start/stop/health
- config.rs — desktop.json, .env, config.yaml
- connection.rs — local/remote/ssh modes
- chat.rs — sendMessage с единым ChatEvent
- Frontend: ConnectionScreen, GatewayStatusBar

**Phase 2: Sessions** (базовая функциональность)
- sessions.rs — SQLite access
- profiles.rs — per-profile management
- Frontend: SessionList, ProfileSwitcher

**Phase 3: Full feature parity** (полнота)
- skills.rs, memory.rs, cronjobs.rs, mcp.rs
- Frontend: SkillsPanel, CronPanel, MCPPanel

---

## Последствия

### Положительные
- Работает с удалённым hermes (NL сервер)
- Session management с SQLite
- Per-profile изоляция
- Gateway auto-restart + health monitoring
- Сохранение преимуществ Tauri (размер, RAM)

### Отрицательные
- Рост Rust кода: 682 → ~3200 строк
- Увеличение времени сборки: ~3 мин → ~5-7 мин
- Новые завимости: tokio, reqwest, ssh2, rusqlite
- Breaking changes в IPC API

### Риски
- tui_gateway протокол может быть несовместим → митигация: capability probing
- SSH на Windows → митигация: conditional compilation
- rusqlite FTS5 → митигация: bundled feature

---

## Метрики успеха

- [ ] Подключение к локальному hermes gateway
- [ ] Подключение к удалённому hermes через API
- [ ] Подключение через SSH tunnel
- [ ] Список сессий из SQLite
- [ ] История сообщений с reasoning/tool events
- [ ] Per-profile переключение
- [ ] Gateway auto-restart при падении
- [ ] Бандл < 20 MB
- [ ] RAM < 100 MB
