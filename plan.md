# autolycus-desktop fixes — multi-agent plan

## Context
Repo: C:\Users\n.gusev\ZCodeProject\autolycus-desktop (Tauri 2: Rust + React/Vite).
Цель: добавить per-connector SOCKS5 прокси (настройки desktop↔Hermes), устранить рекурсию брифинга и игнор задач/источников, собрать MSI.
Ключевые факты от пользователя:
- Прокси = SOCKS5, адрес:порт В НАСТРОЙКАХ (дефолт socks5://127.0.0.1:12334), резерв env HTTP_PROXY/HTTPS_PROXY.
- Галочка «с прокси/без прокси» для ЛЮБОГО коннектора (ТГ/почта/Jira/LLM), default вкл.
- Two-way sync: desktop пишет прокси в config.yaml Hermes и подхватывает обратно.
- Local mode (localhost gateway) — БЕЗ прокси (split вручную).
- Баг А: брифинг плодит сессии (session_id:null) → попадают в list_feed → рекурсия.
- Баг Б: брифинг тянет только items (голые сессии), игнорирует задачи/проекты/цели/источники.

### Task 1: deps-1 — Включить proxy+socks в reqwest
Файл: src-tauri/Cargo.toml строка 28.
Действие: заменить `features = ["stream", "json", "rustls-tls"]` на `features = ["stream", "json", "rustls-tls", "proxy", "socks"]`.
Проверка: `cd src-tauri && cargo check` — нет ошибки `Proxy`/`socks5` unresolved.

### Task 2: proxy-1 — Структура ProxySettings в config.rs
Файл: src-tauri/src/config.rs (рядом с ModelConfig на строке 393).
Добавить:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxySettings {
    pub enabled: bool,
    pub url: String, // socks5://host:port, дефолт socks5://127.0.0.1:12334
}
```
И в `ModelConfig` (строка 393-397) добавить поле `pub proxy: ProxySettings`.
В `get_model_config` (строка 399): при чтении yaml блока `model` считать `model.proxy.enabled` и `model.proxy.url`; если пусто — fallback на env HTTP_PROXY/HTTPS_PROXY, иначе дефолт `socks5://127.0.0.1:12334`.

### Task 3: proxy-2 — use_proxy в SourcesConfig
Файл: src-tauri/src/sources.rs строки 12-43.
Добавить `pub use_proxy: bool` в `TelegramSource`, `EmailSource`, `JiraSource` (default = true для всех). Учесть serde default (добавить `#[serde(default)]` или Default derive, чтобы старые конфиги грузились).
Проверка: `SourcesConfig::load` не падает на старом JSON без use_proxy.

### Task 4: proxy-3 — Telegram OUTBOUND через прокси
Файл: src-tauri/src/telegram.rs функции send_message (строка 44) и validate_bot_token (строка 92).
Изменить: принимать параметр `use_proxy: bool` и `proxy_url: &str`. Если use_proxy → `Client::builder().proxy(reqwest::Proxy::all(proxy_url).unwrap_or_else(|_| reqwest::Proxy::from_env()))`; иначе `Client::new()`.
Вызывающие (в lib.rs где telegram::send_message) должны передавать use_proxy из TelegramSource и proxy_url из ModelConfig.proxy.url.
Проверка: бот отправляет через socks5://127.0.0.1:12334.

### Task 5: proxy-4 — LLM прокси в chat.rs
Файл: src-tauri/src/chat.rs.
- `send_message_via_api` (строка 186): добавить параметры `use_proxy: bool`, `proxy_url: &str`. При use_proxy → `Client::builder().proxy(reqwest::Proxy::all(proxy_url).unwrap_or_else(|_| reqwest::Proxy::from_env()))`; иначе `Client::new()` (Local mode localhost — без прокси).
- `send_message` (строка 442): вычислить `let use_proxy = matches!(connection_mode, ConnectionMode::Remote | ConnectionMode::Ssh);` и `let proxy_url = &model_config.proxy.url;` и передать в send_message_via_api (в Local, Remote, Ssh ветках).
Проверка: Remote LLM идёт через socks5, localhost — напрямую.

### Task 6: proxy-5 — model_discovery прокси
Файл: src-tauri/src/model_discovery.rs строка 78.
Изменить Client::builder(): если base_url внешний (Remote) → `.proxy(reqwest::Proxy::all(proxy_url)...)`; если локальный gateway (127.0.0.1) → `Client::new()`. proxy_url брать из ModelConfig.proxy.url (передать в функцию или прочитать через get_model_config).
Проверка: discovery Remote работает через SOCKS5.

### Task 7: proxy-6 — two-way sync в config.rs
Файл: src-tauri/src/config.rs функция `set_model_config` (строка 426).
Дописать запись `model.proxy.enabled` и `model.proxy.url` в yaml-блок `model` (писать в config.yaml Hermes-home). Убедиться, что `get_model_config` (строка 399) их читает (см. Task 2).
Проверка: после set_model_config в config.yaml появляется model.proxy.url, get_model_config его возвращает.

### Task 8: proxy-7 — UI настройки прокси (SettingsPanel)
Файл: src/components/settings/SettingsPanel.tsx.
Добавить:
- Поле ввода `proxy_url` (default socks5://127.0.0.1:12334) с привязкой к состоянию.
- Галочку `use_proxy` для каждого источника (ТГ/почта/Jira) и для LLM.
- Кнопку/эффект сохранения: вызвать invoke("set_model_config", { proxy: {enabled, url} }) для LLM и invoke("save_source"... ) для источников (писать use_proxy в SourcesConfig → Hermes-home).
Проверка: настройки сохраняются, подхватываются при рестарте (load из config.yaml/SourcesConfig).

### Task 9: proxy-8 — Индикация прокси (ConnectScreen)
Файл: src/components/ConnectScreen.tsx (или settings).
Показать proxy_url и какие источники идут через прокси (считать из ModelConfig.proxy + SourcesConfig).
Проверка: UI отражает состояние.

### Task 10: bugA-1 — Рекурсия брифинга (session_id)
Файл: src/components/views/FeedView.tsx строка 138.
Изменить `session_id: null` на `session_id: \`briefing:${key}\`` (key = source || "unified").
Проверка: брифинг не плодит новые сессии desk-*, перезаписывает одну.

### Task 11: bugA-2 — Исключить брифинг-сессии из контекста
Файл: src/components/views/FeedView.tsx строки 104-106 (sourceItems filter).
Добавить в filter: `&& !i.session_id.startsWith("briefing:")`.
Проверка: повторный брифинг не содержит текст предыдущего брифинга.

### Task 12: bugB-1 — Агрегация задач/источников в брифинг
Файл: src/components/views/FeedView.tsx функция generateBriefing (строка 97).
Параллельно с получением items вызвать invoke list_tasks_cmd, list_goals_cmd, list_projects_cmd, list_sources_cmd (все есть в lib.rs:1932/1957/1977/1284). Собрать контекст: задачи (созданные по прошлым брифингам — помечать тегом), проекты, цели, полнотекст источников (ТГ-бот/юзербот, почта, Jira), новые комментарии.
Проверка: промпт брифинга содержит данные задач/проектов.

### Task 13: bugB-2 — Переписать промпт брифинга
Файл: src/components/views/FeedView.tsx строки 118-135.
Промпт: «Учти задачи (особенно созданные по прошлым брифингам), проекты, цели, новые комментарии в источниках (ТГ, почта, Jira). Не ограничивайся N последними сессиями. Группируй по источникам.»
Проверка: брифинг структурирован по задачам/источникам, не рандомный.

### Task 14: build-1 — MSI инсталлятор
Действие: починить tauri-WiX кэш. Проверить AppData\Local\tauri\WixTools314 — light.exe падает (нужен VC++ runtime). Установить vcredist либо заменить кэш на корректный WiX 3.11 (уже скачан в C:\Users\n.gusev\tools\wix). Затем `cd src-tauri && cargo tauri build --bundles msi` с PATH содержащим C:\Users\n.gusev\tools\wix и C:\Users\n.gusev\tools\nsis\nsis-3.11.
Проверка: bundle/msi/Штурман Desktop_3.2.0_x64_en-US.msi создан.

### Task 15: build-2 — Rebuild NSIS + журнал
Действие: `cd src-tauri && cargo tauri build --bundles nsis` (PATH с бандлерами). Обновить BUILD_STAGES.md — добавить комментарии по стадиям 0-5 (№|Действие|Шаги|Результат|Статус|Комментарий).
Проверка: setup.exe обновлён, BUILD_STAGES.md дополнен.

## Notes
- Стек смешанный (Rust + TSX). Каждый subagent получает свой файл(ы) и точные строки.
- Не коммитить без проверки cargo check / npm run build для своего файла.
- После всех правок — итоговая сборка (build-1, build-2).
