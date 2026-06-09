# План портинга Autolycus Desktop v0.5 → v1.0

## Призм-анализ: ГДЕ МЫ СЕЙЧАС

### Что РАБОТАЕТ (проверено в коде)

| Компонент | Файл | Строк | Статус |
|-----------|------|-------|--------|
| Chat (send/recv/streaming) | chat/ChatView.tsx + chat.rs | 136+518 | ✅ |
| ChatInput (ввод + отправка) | chat/ChatInput.tsx | 67 | ✅ |
| MessageList + MessageBubble | chat/ | 58+101 | ✅ |
| ThinkingBlock + ToolResult + StreamingText | chat/ | ~150 | ✅ |
| ApprovalCard + ApprovalModal | chat/ | ~120 | ✅ |
| ConnectionScreen (local/remote/ssh) | ConnectionScreen.tsx | 373 | ✅ |
| SessionList (из SQLite) | sessions/SessionList.tsx | 189 | ✅ |
| KanbanBoard | kanban/KanbanBoard.tsx | 301 | ✅ |
| SettingsPanel (5 вкладок) | settings/SettingsPanel.tsx | 409 | ✅ |
| Sidebar + Header + StatusBar | layout/ | 52+94+? | ✅ |
| Rust: config.rs (YAML/JSON/env) | config.rs | 23,318 | ✅ |
| Rust: gateway.rs | gateway.rs | 11,998 | ✅ |
| Rust: chat.rs (SSE parser) | chat.rs | 16,615 | ✅ |
| Rust: sessions.rs (SQLite) | sessions.rs | 9,265 | ✅ |
| Rust: profiles.rs | profiles.rs | 7,596 | ✅ |
| Rust: models.rs | models.rs | 4,501 | ✅ |
| Rust: skills.rs | skills.rs | 6,538 | ✅ |
| Rust: ssh.rs | ssh.rs | 8,083 | ✅ |
| Rust: memory.rs | memory.rs | 7,058 | ✅ |
| Rust: cronjobs.rs | cronjobs.rs | 8,753 | ✅ |
| Rust: mcp.rs | mcp.rs | 9,421 | ✅ |
| Rust: kanban.rs | kanban.rs | 17,286 | ✅ |
| Rust: telegram.rs | telegram.rs | 4,538 | ✅ |
| Rust: terminal.rs | terminal.rs | 3,541 | ✅ |
| Rust: media.rs | media.rs | 2,845 | ✅ |
| Rust: validation.rs | validation.rs | 2,873 | ✅ |
| Rust: registry.rs | registry.rs | 5,829 | ✅ |
| Rust: provider_registry.rs | provider_registry.rs | 2,944 | ✅ |
| Rust: model_discovery.rs | model_discovery.rs | 5,528 | ✅ |
| CI/CD (Linux AppImage/deb + Windows MSI) | .github/workflows | ~80 | ✅ |

### ЧТО НЕ РАБОТАЕТ / ПУСТОЕ

| Компонент | Проблема |
|-----------|----------|
| ConnectionScreen SSH host | хардкод 153.80.251.34 вместо автодетекта instances |
| Settings: General | пусто — нет темы/языка/версия |
| Settings: Models | не подключён к Rust (нет invoke) |
| Settings: Terminal | нет терминала |
| Chat через SSH | туннель поднимается но API на сервере не включён был |
| Автодетект instances | нет кода для поиска hermes/autolycus на сервере/локально |
| Version display | показывает v0.4.0 хотя Cargo.toml v0.5.0 |
| Sidebar | только 4 вкладки: chat, kanban, sessions, settings |
| Launch splash | нет splash screen при запуске |
| Обновление | нет Tauri updater |
| Keyboard shortcuts | нет |
| i18n | всё захардкодено на русском |
| Welcome экран | нет |
| ErrorBoundary | нет |

---

## ПЛАН ПОРТИНГА (6 фаз)

### Фаза 0: Подготовка + багфикс (1 день)
**Цель:** Исправить критичные баги, синхронизировать версии, включить API на сервере

- [ ] Исправить SSH host: убрать хардкод, добавить поле ввода + авто_detект
- [ ] Синхронизировать версию: Cargo.toml 0.5.0, package.json 0.5.0, lib.rs v0.5.0, App.tsx "v0.5.0"
- [ ] Убрать неиспользуемые импорты и мёртвый код если есть
- [ ] **Тест:** `cargo check` + `npm run build`

### Фаза 1: Автодетект + Welcome + Структура (2 дня)
**Цель:** Автоматическое обнаружение instances hermes/autolycus, Welcome экран

#### Автодетект instances
- [ ] Rust: added `detect_instances_cmd` — сканирует:
  - Локально: `~/.autolycus/venv/bin/python`, `~/.hermes/hermes-agent/venv/bin/python`, `~/autolycus/venv/bin/python`
  - Удалённо (через SSH): `which hermes`, `which autolycus`, проверка `hermes gateway status`
- [ ] ConnectionScreen: выпадающий список instances + кнопка "Авто检测" вместо хардкода IP
- [ ] Сохранение instance в desktop.json

#### Welcome экран
- [ ] Splash screen: логотип + проверка подключения 2 сек
- [ ] WelcomeScreen: если нет сохранённого instance → показать ConnectionScreen
- [ ] Навигация: Welcome → (авто-detект OK) → Main layout

#### Frontend структура недостающих компонентов
- [ ] Settings: General tab — тема, язык, версия hermes
- [ ] Settings: Terminal tab — открыть терминал
- [ ] ErrorBoundary — обёртка для всех views
- [ ] Keyboard shortcuts (Ctrl+N new chat, Ctrl+K command palette)

### Фаза 2: Настройки + модели + провайдеры (3 дня)

#### Settings General
- [ ] Тёмная/светлая тема (CSS variables, store в uiStore)
- [ ] Язык: English / Русский (простая реализация через useState + translations map)
- [ ] Показать версию Hermes (gateway_status_cmd) + версию Desktop

#### Models UI
- [ ] Подключить SettingsPanel Models tab к Rust: list_models_cmd / add_model_cmd / remove_model_cmd
- [ ] Список моделей из models.json
- [ ] CRUD: добавить/удалить/редактировать модель
- [ ] Переключение активной модели (set_model_config_cmd)

#### Providers UI
- [ ] Список провайдеров: OpenRouter, Anthropic, OpenAI, DeepSeek, Ollama, vLLM (захардкод списка)
- [ ] Ввод API key для каждого провайдера (set_env_cmd)
- [ ] Ввод base URL (set_model_config_cmd)
- [ ] Индикатор: ключ сохранён / не сохранён
- [ ] validate_chat_readiness_cmd — проверка перед чатом

#### Авто-detект через SSH
- [ ] UI: форма SSH (host, port, user, key_path) + "Scan" button
- [ ] После SSH connect → запускает `hermes status` на удалённом хосте
- [ ] Находит: hermes version, installed path, active profile, running gateway port
- [ ] Сохраняет в desktop.json > instances

### Фаза 3: Сессия + чат расширения (2 дня)

#### SessionList доработка
- [ ] Группировка по дате (сегодня / вчера / старые)
- [ ] Возобновление сессии (load history → в ChatView)
- [ ] Переименование сессии

#### ChatView расширение
- [ ] Отображение model name в header
- [ ] Контекстное окно indicator (context gauge)
- [ ] Tool events визуализация (tool_start / tool_complete в реальном времени)
- [ ] Reasoning display (thinking block) из streaming
- [ ] Slash commands: /model, /clear, /new, /reasoning

#### Approval flow
- [ ] ApprovalCard в чате (вместо отдельного модального окна)
- [ ] Выбор: approve / deny / approve_always
- [ ] Отправка через send_message_cmd с approval_decision

### Фаза 4: Чат через SSH + тестирование (2 дня)

#### SSH-туннель интеграция
- [ ] Убрать хардкод порта 8642 — читать из gateway config
- [ ] ConnectionScreen: host → SSH connect → detect remote gateway port → туннель local→remote
- [ ] Автоматический reconnect при падении туннеля
- [ ] Индикатор статуса: tunnel up/down, latency

#### API сервер на хосте
- [ ] Автоматический запуск `hermes gateway run` на удалённом хосте если не запущен
- [ ] Health check через SSH: `curl http://127.0.0.1:{port}/health`
- [ ] Определение remote port из `gateway_state.json` или CLI

#### Полный flow тестирование
- [ ] Windows: установить MSI → запустить → авто-detект local instance → чат
- [ ] Windows: подключиться по SSH → туннель → API → чат
- [ ] Linux: AppImage → то же

### Фаза 5: Kanban + Memory (2 дня)

#### Kanban UI доработка
- [ ] Kanban: подключить к Rust backend (list_kanban_boards_cmd etc.)
- [ ] Kanban: create/update/delete tasks
- [ ] Kanban: columns, status flow (triage→todo→running→blocked→done)
- [ ] Kanban: drag-and-drop

#### Memory UI
- [ ] Memory вкладка: чтение/запись MEMORY.md, USER.md
- [ ] Memory entries CRUD
- [ ] Capacity bar

### Фаза 6: Сборка + Release (1 день)

#### Сборка и публикация
- [ ] v0.5.0: обновить версию everywhere
- [ ] v1.0.0: переименовать → Autolycus Desktop v1.0
- [ ] Windows MSI: через GitHub Actions
- [ ] Linux AppImage + deb
- [ ] GitHub release notes
- [ ] Проверить подпись MSI
- [ ] Тест: установить MSI на Windows VM

---

## Оценка трудозатрат

| Фаза | Дней | Строк (оценка) | Ключевые риски |
|------|------|-----------------|----------------|
| 0: Подготовка + багфикс | 1 | ~200 | SSH: API сервер был выключен |
| 1: Автодетект + Welcome | 2 | ~800 | SSH connect timing |
| 2: Настройки + модели | 3 | ~1200 | Per-провайдер validation |
| 3: Сессия + чат расширения | 2 | ~600 | Streaming edge cases |
| 4: Чат через SSH | 2 | ~500 | Tunnel reliability |
| 5: Kanban + Memory | 2 | ~400 | Kanban state sync |
| 6: Сборка + Release | 1 | ~100 | Windows сервер для MSI |

**Итого:** ~13 дней, ~3,800 строк нода + обновления Rust

---

## Definition of Done

Для каждой фазы:
- [ ] `cargo check` без ошибок
- [ ] `npm run build` без ошибок
- [ ] Все Tauri commands типизированы (types.ts)
- [ ] Нет хардкодов в UI
- Тест на реальном Windows/Linux
