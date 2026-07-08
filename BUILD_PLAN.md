# План фиксов autolycus-desktop (Штурман Desktop) — прокси per-connector, баги брифинга, MSI

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task (zero-dollar kilo stack).

**Goal:** Добавить per-connector прокси (ТГ/LLM через прокси, Jira/почта — по галочке), two-way sync настроек desktop↔Hermes, устранить рекурсию брифинга и игнор задач/источников, собрать MSI.

**Architecture:**
- Прокси = настройка НА УРОВНЕ КОННЕКТОРА (галочка «с прокси / без прокси» в UI подключения источника).
- LLM (Remote) — прокси вкл по умолчанию (РФ-блок). Telegram — прокси вкл по умолчанию (api.telegram.org заблокирован). Jira/Email — прокси ВЫКЛ по умолчанию, с галочкой (можно вкл).
- Two-way sync: `SourcesConfig` и `ModelConfig` пишутся в Hermes-home (sources.json / config.yaml) и подхватываются обратно (load). Входящий ТГ/Jira (через Hermes-backend) тоже получают прокси из этих же настроек.
- Local mode (localhost gateway) — БЕЗ прокси (split: только внешние endpoint'ы).

**Tech Stack:** Rust (Tauri 2), React/Vite/TypeScript, reqwest 0.12 (нужен feature `proxy`), rusqlite, Hermes backend.

---

## Этап 0. Зависимости (infra first)

| № | Действие | Файл | Шаги | Ожидаемый результат | Проверка |
|---|----------|------|------|---------------------|----------|
| 0.1 | Включить feature `proxy` + `socks` в reqwest | `src-tauri/Cargo.toml:28` | `features = ["stream", "json", "rustls-tls", "proxy", "socks"]` | `reqwest::Proxy` + SOCKS5 схема доступны | `cargo check` без `Proxy`/`socks5` unresolved |

## Этап 1. Per-connector прокси (desktop ↔ Hermes)

**Ключевые факты (от пользователя):**
- Прокси = SOCKS5, адрес и порт задаются В НАСТРОЙКАХ (не хардкод). Дефолт: `socks5://127.0.0.1:12334`.
- Резерв: env `HTTP_PROXY`/`HTTPS_PROXY` (тоже socks5).
- Галочка «с прокси / без прокси» для ЛЮБОГО коннектора (ТГ/почта/Jira/LLM).
- Two-way sync: desktop пишет прокси в config.yaml Hermes и подхватывает обратно.

| № | Действие | Файл | Шаги | Ожидаемый результат | Проверка |
|---|----------|------|------|---------------------|----------|
| 1.0 | Структура прокси-настроек | `src-tauri/src/config.rs` (новый блок) | `pub struct ProxySettings { pub enabled: bool, pub url: String }` (url = `socks5://host:port`, дефолт `socks5://127.0.0.1:12334`); + `pub proxy: ProxySettings` в `ModelConfig` | централизованные настройки прокси | `get_model_config().proxy.url` = дефолт при пустом config |
| 1.1 | Поле `use_proxy` в структуры источников | `src-tauri/src/sources.rs:12-43` | добавить `pub use_proxy: bool` в `TelegramSource`, `EmailSource`, `JiraSource` (default true для всех — галочка вкл по умолчанию для любого коннектора, как просил юзер) | каждый коннектор хранит флаг | `SourcesConfig::load` читает флаг |
| 1.2 | Применить прокси в Telegram OUTBOUND | `src-tauri/src/telegram.rs:44,92` | если `use_proxy` → `Client::builder().proxy(proxy_from_settings_or_env())` (явный `socks5://host:port` из настроек, fallback `Proxy::from_env()`); иначе `Client::new()` | ТГ бот шлёт через прокси при включённой галочке | бот отправляет через `socks5://127.0.0.1:12334` |
| 1.3 | Прокси для LLM (Remote) | `src-tauri/src/chat.rs:186` | `send_message_via_api` принимает `use_proxy: bool` + `proxy_url: &str`; вкл → `Client::builder().proxy(reqwest::Proxy::all(proxy_url).unwrap_or_else(\|_\| reqwest::Proxy::from_env()))` (SOCKS5); Local/выкл → `Client::new()` | LLM через прокси для Remote (SOCKS5 из настроек/env), localhost — нет | Remote LLM работает через `socks5://127.0.0.1:12334` |
| 1.4 | Проброс флага режима + url | `src-tauri/src/chat.rs:442 send_message` | `use_proxy = (mode == Remote \|\| mode == Ssh)`, `proxy_url = model_config.proxy.url` | split корректен, url передаётся | см. 1.3 |
| 1.5 | Прокси в model_discovery | `src-tauri/src/model_discovery.rs:78` | Remote base_url → `Client::builder().proxy(reqwest::Proxy::all(proxy_url)...)`; локальный gateway → `Client::new()` | discovery моделей через прокси для Remote | discovery Remote работает через SOCKS5 |
| 1.6 | Поле прокси в ModelConfig + two-way sync | `src-tauri/src/config.rs:393,426` | `set_model_config` пишет `model.proxy.enabled` + `model.proxy.url` в config.yaml Hermes; `get_model_config` читает (fallback на env `HTTP_PROXY`/`HTTPS_PROXY`, дефолт `socks5://127.0.0.1:12334`) | настройки LLM-прокси desktop↔Hermes | config.yaml содержит `model.proxy.url`, desktop подхватывает SOCKS5 |
| 1.7 | UI: адрес/порт прокси + галочка per-connector | `src/components/settings/SettingsPanel.tsx` | поле ввода `proxy_url` (scheme://host:port, дефолт `socks5://127.0.0.1:12334`) + галочка `use_proxy` для каждого источника (ТГ/почта/Jira) и для LLM; биндинг на `save_proxy`/`save_source` (пишет в config.yaml Hermes / SourcesConfig → Hermes-home) | юзер задаёт адрес:порт и вкл/выкл прокси для любого коннектора | настройки сохраняются и подхватываются при рестарте |
| 1.8 | Индикация статуса прокси | `src/components/ConnectScreen.tsx` | показать proxy_url + какие источники идут через прокси (считано из SourcesConfig/Hermes) | юзер видит синхронизацию | UI отражает состояние |

## Этап 2. Баг А — рекурсия брифинга

| № | Действие | Файл | Шаги | Ожидаемый результат | Проверка |
|---|----------|------|------|---------------------|----------|
| 2.1 | Детерминированный session_id | `src/components/views/FeedView.tsx:138` | `session_id: null` → `session_id: \`briefing:${key}\`` | брифинг не плодит сессии | нет новых `desk-*` от брифинга |
| 2.2 | Исключить брифинг-сессии из контекста | `src/components/views/FeedView.tsx:104-106` | `&& !i.session_id.startsWith("briefing:")` в filter | брифинг не самокормится | повторный брифинг не содержит предыдущий |

## Этап 3. Баг Б — брифинг игнорирует задачи/источники

| № | Действие | Файл | Шаги | Ожидаемый результат | Проверка |
|---|----------|------|------|---------------------|----------|
| 3.1 | Дотянуть задачи/цели/проекты/источники | `src/components/views/FeedView.tsx:97` | параллельно `list_tasks_cmd`, `list_goals_cmd`, `list_projects_cmd`, `list_sources_cmd` | контекст обогащён | промпт содержит задачи/проекты |
| 3.2 | Агрегировать контекст | `src/components/views/FeedView.tsx:113-116` | блоки: задачи (по прошлым брифингам — тег/ссылка), проекты, цели, полнотекст источников (ТГ-бот/юзербот, почта, Jira), новые комментарии | брифинг учитывает все источники | брифинг упоминает задачи и комментарии Jira/ТГ |
| 3.3 | Переписать промпт | `src/components/views/FeedView.tsx:118-135` | «Учти задачи (созданные по прошлым брифингам), проекты, цели, новые комментарии в источниках. Не ограничивайся N последними сессиями.» | конец «рандомным N беседам» | брифинг структурирован по задачам/источникам |

## Этап 4. MSI-инсталлятор

| № | Действие | Файл | Шаги | Ожидаемый результат | Проверка |
|---|----------|------|------|---------------------|----------|
| 4.1 | Починить tauri-WiX кэш | `AppData\Local\tauri\WixTools314` | VC++ runtime для light.exe ИЛИ заменить кэш на корректный WiX 3.11 | `light.exe` не падает | `cargo tauri build --bundles msi` OK |
| 4.2 | Собрать MSI | `src-tauri` | `cargo tauri build --bundles msi` (PATH: NSIS+WiX) | `bundle/msi/...msi` | файл существует |

## Этап 5. Сборка и проверка

| № | Действие | Файл | Шаги | Ожидаемый результат | Проверка |
|---|----------|------|------|---------------------|----------|
| 5.1 | Rebuild NSIS | `src-tauri` | `cargo tauri build --bundles nsis` | обновлённый setup.exe | установка + брифинг без рекурсии |
| 5.2 | Обновить BUILD_STAGES.md | `BUILD_STAGES.md` | комментарии по стадиям 0-5 | журнал дополнен | файл обновлён |

---

**Порядок:** 0 → 1 → 2 → 3 → 4 → 5.

**Риски:**
- `reqwest::Proxy::from_env()` требует feature `proxy` (Этап 0 обязателен).
- Local mode (localhost gateway) — прокси НЕ применяем (split вручную), чтобы gateway-трафик шёл напрямую.
- Входящий ТГ/Jira через Hermes-backend: прокси берётся из тех же SourcesConfig/ModelConfig (Hermes читает свой config.yaml/sources.json). Desktop пишет туда настройки → two-way sync.
- UI-галочка пишет через `save_source` в SourcesConfig (уже есть `save()` в sources.rs:70).

**Коммиты:** после каждого этапа.
