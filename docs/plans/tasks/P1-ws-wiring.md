# Задача: P1 — переключить gateway на `hermes serve` и сделать WS дефолтом

> **Статус задачи:** blocked on P-AUDIT · **Зависимости:** P-AUDIT должен быть завершён
> **Создан:** 2026-07-15

## Роль
Ты — senior Rust-инженер в Steersman Desktop. Переводишь бэкенд-спавн и
транспорт на реальный контракт Hermes (ADR-004). Строго по TDD.
Один подшаг — отдельный коммит.

## Контекст (читай перед работой)
1. `docs/plans/ADR-004-ws-transport-contract.md` — ground truth. Spawn:
   `hermes serve --host 127.0.0.1 --port 0`, токен `HERMES_DASHBOARD_SESSION_TOKEN`.
2. Phase 0 готова: `ws_transport.rs`, `parse_ws_message`, переключатель
   `STEERSMAN_TRANSPORT` в `chat.rs` (Local-ветка, send_via_ws_local).
3. `src-tauri/src/gateway.rs` — текущий spawn (слушай сюда):
   `allocate_port` (101-111, хардкод 8642), `start_gateway` (115-369,
   сейчас шлёт `API_SERVER_ENABLED=true` + `API_SERVER_PORT`), health check
   (300-332, raw TCP + HTTP/1.0 GET /health), `get_api_url` (466-468,
   http://), `GatewayProcess` (47-53: child+port+profile_key).
4. `src-tauri/src/config.rs`: `get_api_server_key` (919-922), `read_env` (245).
5. НЕ сверяйся с ADR-003 по spawn-команде — он устарел (говорит `hermes gateway`,
   реально нужен `hermes serve`). ADR-004 точнее.

## Проблема
Phase 0 доказала, что WS-транспорт работает, но он не подключён к продакшен-флоу:
- `gateway.rs` всё ещё spawn'ит `hermes gateway` (мессенджер-сервис) с
  `API_SERVER_*` env, который бэкенд игнорирует;
- порт хардкожен 8642 (allocate_port), а `hermes serve --port 0` отдаёт
  случайный порт, который надо читать из stdout;
- токен `HERMES_DASHBOARD_SESSION_TOKEN` не генерируется Steersman'ом
  (Phase 0 брала его из env вручную);
- WS-путь доступен только через `STEERSMAN_TRANSPORT=ws`, дефолт — сломанный HTTP.
Итог: пользователь запускает приложение — чат не работает, потому что спавн
идёт в никуда.

## Root cause (из ADR-004 §«Что меняется в gateway.rs»)
- `gateway.rs:161-184` — spawn команда `hermes gateway` вместо `serve`.
  Контракт ADR-004 §1: `serve --host 127.0.0.1 --port 0`.
- `gateway.rs:205-258` — env шлёт `API_SERVER_ENABLED/PORT/KEY`, а нужно
  `HERMES_DASHBOARD_SESSION_TOKEN=<random 32 bytes base64url>`.
- `gateway.rs:101-111 allocate_port` — хардкод 8642; с `--port 0` порт
  назначается ОС и печатается бэкендом в stdout (`HERMES_BACKEND_READY port=<n>`).
- `gateway.rs:300-332` health — TCP/HTTP poll на 8642; readiness теперь
  определяется WS-handshake (событие `gateway.ready`).
- `chat.rs` Local-ветка — дефолт `STEERSMAN_TRANSPORT=http` надо сменить на `ws`.

## Подзадачи (каждая = Red→Green→коммит)

### P1.1 — Команда spawn: `serve --port 0`
- **Контракт:** args `["--profile", p, "serve", "--host", "127.0.0.1", "--port", "0"]`.
- **DoD:** unit-тест на конструктор args (мок spawn не нужен — проверка массива).
  `start_gateway` вызывает бэкенд с этими args. Убрать `API_SERVER_*` из env.

### P1.2 — Чтение порта из stdout
- **Контракт:** бэкенд печатает `HERMES_BACKEND_READY port=<n>`. Steersman
  должен парсить это из stdout child (фон-поток чтения уже есть,
  `gateway.rs:286-298`).
- **DoD:** тест на парсер строки `HERMES_BACKEND_READY port=9420` → `9420`.
  `GatewayProcess.port` наполняется распарсенным значением, а не allocate_port.

### P1.3 — Генерация и проброс session token
- **Контракт (ADR-004 §2):** `HERMES_DASHBOARD_SESSION_TOKEN = secrets.token_urlsafe(32)`.
  Steersman генерирует, передаёт в env child, хранит в `GatewayProcess`
  для последующего WS-URL.
- **DoD:** тест на генератор (длина 43 символа base64url, уникальность).
  Добавить поле `session_token` в `GatewayProcess`. WS-URL строится из
  `port` + `session_token`.

### P1.4 — Readiness через WS handshake (замена TCP health)
- **Контракт:** после spawn — открыть WS, дождаться `gateway.ready`,
  закрыть (или оставить как долгоживущее — см. P2).
- **DoD:** `api_server_available` устанавливается в `Some(true)` только после
  получения `gateway.ready`. Таймаут 30s сохранить.

### P1.5 — WS дефолт; HTTP-путь за флагом
- **DoD:** дефолт `STEERSMAN_TRANSPORT=ws`. Инвертировать флаг: `=http`
  включает legacy (для отката). Обновить ADR-004 §DoD: WS теперь primary.

## Жёсткие ограничения
- НЕ удаляй HTTP-функции в chat.rs (send_message_via_api и т.д.) — это P2.
  Оставь их достижимыми через `STEERSMAN_TRANSPORT=http`.
- НЕ трогай Remote/SSH ветки send_message — отдельная задача (P3).
- Новых crate не добавлять (tokio-tungstenite уже есть из Phase 0).
- `unwrap()` в затронутом коде → `?` или явная ошибка.
- Conventional commits: `feat(gateway): spawn hermes serve --port 0` и т.д.

## Verification
- `cargo test` — все зелёные, включая новые (RED→GREEN для каждой подзадачи).
- `cargo clippy -D warnings` — чисто в gateway.rs / chat.rs.
- E2E (вручную): запустить приложение БЕЗ env-флагов → отправить сообщение →
  токены стримятся в ChatView. Скриншот/лог как evidence.
- ДО запуска: прогони P-AUDIT, чтобы убедиться что не делаешь уже закрытое.
