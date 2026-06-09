# План портинга fathah/hermes-desktop → autolycus-desktop (Tauri+Rust)

## Обзор

**Оригинал:** fathah/hermes-desktop — Electron+TypeScript, ~20,800 строк (+ 20,300 строк экранов)
**Текущее состояние:** autolycus-desktop v0.5.0 — Tauri+Rust, бэкенд ~90% готов, фронтенд ~15% готов

## Текущее состояние (что уже портировано в Rust)

### ✅ Rust backend (src-tauri/src/) — 9 из 50+ модулей оригинала:
| Модуль | Строк | Оригинал |
|--------|-------|----------|
| config.rs (YAML/JSON/envconfig) | 23,318 | config.ts (1,679) |
| chat.rs (SSE streaming) | 16,615 | hermes.ts (partial) |
| sessions.rs (SQLite) | 9,265 | sessions.ts (696) |
| gateway.rs (процесс-менеджмент) | 11,998 | gateway+installer partial |
| kanban.rs | 17,286 | kanban.ts (425) |
| skills.rs | 6,538 | skills.ts (447) |
| models.rs + model_discovery.rs | 10,029 | models.ts (230) + model-discovery.ts (680) |
| mcp.rs | 9,421 | mcp-servers.ts (851) + tools.ts (371) |
| profiles.rs | 7,596 | profiles.ts (295) |
| provider_registry.rs + registry.rs | 8,773 | provider-registry.ts (47) + registry.ts (516) |
| cronjobs.rs | 8,753 | cronjobs.ts (329) |
| memory.rs | 7,058 | memory.ts (206) |
| telegram.rs | 4,538 | messaging-platforms.ts (199) |
| ssh.rs | 8,083 | ssh-tunnel.ts (258) + ssh-remote.ts (2,222) |
| terminal.rs | 3,541 | terminal-launcher.ts (621) |
| media.rs | 2,845 | media.ts (192) |

### ❌ Критичные пробелы в Rust:
| Модуль оригинала | Строк | Статус |
|-----------------|-------|--------|
| hermes.ts (WebSocket RPC, профили, send/recv) | 3,501 | ❌ НЕТ |
| ssh-remote.ts (SSH proxy для всех операций) | 2,222 | ❌ НЕТ |
| installer.ts (установка/обновление/бэкап) | 1,480 | ❌ НЕТ |
| config-health.ts (аудит конфигурации) | 1,049 | ❌ НЕТ |
| claw3d.ts (3D office) | 1,046 | ❌ НЕТ |
| hermes-auth.ts (OAuth login) | 173 | ❌ НЕТ |
| utils.ts / profiles.ts / validation.ts | 591 | тяжелые части |
| session-cache.ts / attachment-store.ts | 468 | частично |

### ❌ Фронтенд (React/TSX) — ~20,300 строк, из них портировано ~2,000 (~10%)

---

## Фазы портинга

### Фаза 1: Ядро чата + настройки + багфикс
**Цель:** Чат работаег через SSH, настройки не пустые, версия корректна
**Оценка:** ~500 строк

- [x] Исправить SSH host (147.90.10.50 → 153.80.251.34)
- [x] Включить API сервер на сервере (8642)
- [x] UFW 8642
- [x] Публичные ключи DESKTOP в authorized_keys
- [ ] Settings: показать текущий профиль, версию, connection mode
- [ ] Settings: рабочие вкладки General / Connection / Version
- [ ] Исправить версию (0.4.0 → 0.5.0 везде: lib.rs, App.tsx, package.json)
- [ ] Welcome/Setup экраны: заглушки с кнопкой "Connect"
- [ ] Навигация: Session List → реальные данные из SQLite
- [ ] Навигация: Chat → реальный чат через SSH
- [ ] Тест: собрать, подключиться, отправить сообщение

### Фаза 2: Экраны конфигурации + провайдеры + модели
**Цель:** Настройки и управление ИИ работают полноценно
**Оценка:** ~3,000 строк

##### Провайдеры (команды Tauri)
- [ ] getProviderBaseUrl — GET
- [ ] providers: список провайдеров, редактирование ключей, базовых URL
- [ ] Провайдер-реестр: 14+ провайдеров с автодополнением

#### Модели
- [ ] listModels — реализовать поиск (локальный + remote)
- [ ] addModel / removeModel — CRUD
- [ ] updateModel — редактирование
- [ ] Auxiliary tasks (vision, compression, web_extract)
- [ ] Модели профиля: переключение через UI

#### Профили
- [ ] listProfiles — список
- [ ] createProfile / deleteProfile
- [ ] setActiveProfile

#### Сессии
- [ ] Список сессий с поиском
- [ ] Переименование, удаление
- [ ] Возобновление сессии (загрузка истории)

**Тестирование:** `tauri dev` и `tauri build`, проверка каждого экрана

### Фаза 3: Инструменты + навыки + календарь
**Цель:** Полное управление toolsets + skills + cron
**Оценка:** ~2,500 строк

#### Toolsets
- [ ] getToolsets / getPlatformToolsets
- [ ] setToolsetEnabled (включая messaging platforms)
- [ ] tools-экран: все платформенные тулсеты с переключателям

#### Навыки
- [ ] listInstalledSkills / listBundledSkills
- [ ] installSkill / uninstallSkill
- [ ] getSkillContent (чтение SKILL.md)
- [ ] searchSkills (реестр)
- [ ] Skills экран с установленными + каталогом

#### Cron (Расписания)
- [ ] listCronJobs / createCronJob / removeCronJob
- [ ] pauseCronJob / resumeCronJob / triggerCronJob
- [ ] Schedules экран с билдером расписания

#### Memory
- [ ] чтение/запись MEMORY.md, USER.md
- [ ] Capacity bar (символы использовано / лимит)
- [ ] Провайдеры памяти (Honcho, Mem0, etc.)
- [ ] Memory экран: Entries / Profile / Providers / Soul

**Тестирование:** `tauri dev` и `tauri build`

### Фаза 4: Чат расширенный + Kanban + Discover
**Цель:** Полнофункциональный чат с расширенными фичами + Kanban
**Оценка:** ~4,000 строк

#### Чат
- [ ] Реальная отправка/получение через SSH (model-dependent)
- [ ] MessageList → реальный рендер markdown + tool events + reasoning
- [ ] ChatInput → текстовая область + slash commands + вложения + голос
- [ ] Streaming response tool events (tool_start, tool_complete)
- [ ] Reasoning display
- [ ] ClarifyCard (запрос уточнения от агента)
- [ ] Контекстное окно indicator
- [ ] Быстрые команды (/, модель, reasoning effort)

#### Действия чата
- [ ] копировать, регенерировать
- [ ] вложения (файлы, изображения) → диалог открытия
- [ ] быстрые настройки модели прямо в чате

#### Kanban
- [ ] Kanban-доска: колонки (triage/todo/ready/running/blocked/done)
- [ ] Создание/редактирование/удаление задач и колонок
- [ ] Статусы с drag-and-drop
- [ ] Комментарии задач
- [ ] Runs и результаты

#### Discover
- [ ] Каталог реестра (skills, MCP, agents, workflows)
- [ ] Поиск и фильтр
- [ ] Установка/удаление элементов

**Тестирование:** `tauri dev` и `tauri build`

### Фаза 5: Gateway + Office
**Цель:** Управление шлюзом + 3D офис
**Оценка:** ~3,000 строк

#### Gateway
- [ ] Статус gateway (version, memory, cpu)
- [ ] Старт/стоп/рестарт gateway
- [ ] Платформы: Telegram/Discord/Slack/etc.
- [ ] API-ключи / OAuth
- [ ] Логи gateway (tail/follow)
- [ ] Модель конфигурация

#### Office (3D)
- [ ] Splash screen (заставка при запуске)
- [ ] Loading профиля (профили)
- [ ] Диалог OneChat (модальное окно чата)
- [ ] 3D-рендер с агентами
- [ ] Аватары агентов (hair/skin/clothes на seed-hash)

**Тестирование:** `tauri dev` и `tauri build`

### Фаза 6: i18n + сборка + репозиторий
**Цель:** Мультиязычность + push + release
**Оценка:** ~1,000 строк

#### i18n
- [ ] Английский (дефолт)
- [ ] Русский (перевод)

#### Сборка и релиз
- [ ] `npm run tauri build` → `.msi` / `.nsis`
- [ ] Подпись кода (если нужно)
- [ ] GitHub releases + auto-update
- [ ] README / документация

**Тестирование:** собрать `.msi`, установить на Windows, проверить все фазы

---

## Файлы к созданию (фронтенд, TSX)

**src/components/layout/**
- `Layout.tsx` — главная оболочка с навигацией
- `Header.tsx` — шапка
- `StatusBar.tsx` — строка статуса
- `Sidebar.tsx` — боковая панель

**src/components/settings/**
- `SettingsPanel.tsx` — настройки с вкладками

**src/components/sessions/**
- `SessionList.tsx` — список сессий
- `SessionRow.tsx` — строка сессии

**src/components/models/**
- `ModelList.tsx`

**src/components/providers/**
- `ProviderList.tsx`

**src/components/tools/**
- `ToolsPanel.tsx`

**src/components/memory/**
- `MemoryPanel.tsx`
- `MemoryEntries.tsx`

**src/components/skills/**
- `SkillsPanel.tsx`

**src/components/schedules/**
- `SchedulesPanel.tsx`

**src/components/discover/**
- `DiscoverPanel.tsx`

**src/components/gateway/**
- `GatewayPanel.tsx`

**src/components/kanban/**
- `KanbanBoard.tsx`

**src/components/chat/**
- Существующие: ChatView, ChatInput, MessageList, MessageBubble — доработать

## Ключевые архитектурные решения

1. **SSH-туннель**: поднимаем локально, API-сервер уже запущен на сервере
2. **API-сервер**: OpenAI-совместимый на порту 8642
3. **Локальный кэш**: SQLite для сессий/настроек
4. **Стриминг**: SSE (Server-Sent Events) для чата
5. **Профили**: per-профайл настройки (model, env, hermes_home)
6. **i18n**: через Tauri-состояние + константы (без библиотек)

## Definition of Done для фазы
- [ ] `npm run tauri dev` работает (hot reload)
- [ ] `npm run tauri build` собирается без ошибок
- [ ] Чат → отправка → ответ через SSH
- [ ] Навигация между всеми экранами текущей фазы
- [ ] Данные сохраняются между сессиями
- [ ] cargo clippy без warnings
