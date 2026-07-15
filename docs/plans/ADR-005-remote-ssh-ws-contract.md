# ADR-005: Remote/SSH WebSocket-контракт

> **Статус:** принято
> **Дата:** 2026-07-15
> **Решение:** Перевести ветки `ConnectionMode::Remote` и `ConnectionMode::Ssh`
> в `chat.rs` на тот же WebSocket `/api/ws` транспорт, что и Local (ADR-004).

---

## Контекст

ADR-004 зафиксировал WS-контракт для **Local** режима (`hermes serve` + WS
`/api/ws` + session token). ADR-004 §«Что не покрывает этот ADR» явно оставил
Remote/SSH вне scope: «там бэкенд чужой, контракт может отличать».

После анализа кода и upstream-контракта выяснилось: **бэкенд тот же самый**
(`hermes serve` / TUI Gateway из NousResearch/hermes-agent). Контракт `/api/ws`
JSON-RPC 2.0 + `?token=` auth не зависит от того, где запущен сервер — на
localhost, на удалённой машине или за SSH-туннелем. Различаются только:

1. **URL source** — как Steersman узнаёт host:port
2. **Token source** — откуда берётся session token
3. **Транспортный путь** — прямой TCP (Remote) vs SSH-туннель (SSH)

## Контракт по режимам

### Remote (прямое подключение к удалённому `hermes serve`)

```
ws_url = ws://<remote_host>:<remote_port>/api/ws?token=<session_token>
```

- `remote_host:remote_port` — из `connectionStore.config.remote_url` (уже
  передаётся в `send_message` как `remote_url`).
- **Проблема**: `remote_url` сейчас приходит как `http://...` — для WS нужна
  схема `ws://`. Конвертация: `http://` → `ws://`, `https://` → `wss://`.
- `session_token` — **не** `remote_api_key` (это legacy `API_SERVER_KEY`).
  Для Remote нужен session token удалённого бэкенда. Два пути:
  - (a) Пользователь вводит его в Settings → Connection (как сейчас вводит
    api_key). Поле переименовывается из `API Key` в `Session Token`.
  - (b) Steersman получает ticket через `POST /api/auth/ticket` (upstream
    `mintTicket`, видно в electron-main.mjs). Сложнее, требует OAuth flow.
  - **Решение P3.2**: путь (a) — переиспользовать существующее поле api_key
    как session token. Минимальное изменение, не требует UI-редизайна.

### SSH (туннель к удалённому `hermes serve`)

```
ssh -L <local_port>:127.0.0.1:<remote_port> <user>@<host>
ws_url = ws://127.0.0.1:<local_port>/api/ws?token=<session_token>
```

- SSH-туннель (`ssh.rs:78-88`) — это **generic TCP forwarding**
  (`-L local:remote`). Он пробрасывает ЛЮБОЙ TCP-трафик, включая WebSocket
  handshake и frames. WS-over-SSH работает без модификаций SSH-слоя.
- `local_port` — из `SshConfig.local_port` (`config.rs:117`), уже туннелируется.
- **Проблема**: `get_tunnel_url` (`ssh.rs:56`) возвращает `http://127.0.0.1:<port>`
  — для WS нужен `ws://`. Нужно либо новое `get_tunnel_ws_url`, либо конвертация
  схемы на стороне вызова.
- `session_token` — тот же, что для Remote (удалённого бэкенда).

## Решение

### Принципы
1. **WS везде** — Remote/SSH используют тот же `send_message_via_ws`, что Local.
   Никакого HTTP `/v1/chat/completions` ни в одном режиме.
2. **Схема-конвертация** — единая helper-функция `to_ws_url(http_url)`,
   заменяющая `http://`→`ws://`, `https://`→`wss://`. Покрыта unit-тестом.
3. **Token = session token** — поле `remote_api_key` интерпретируется как
   session token (переименование UI — отдельная задача; контракт уже работает).
4. **SSH-туннель не трогать** — он generic TCP, WS проходит сквозь. Только
   меняется scheme URL после туннеля.

### Что меняется в коде

| Файл | Изменение |
|---|---|
| `chat.rs` Remote-ветка | `send_via_best_transport(...)` → `send_message_via_ws(ws_url, token, ...)` |
| `chat.rs` SSH-ветка | то же; `tunnel_url` конвертируется `to_ws_url` |
| `chat.rs` | новая helper `fn to_ws_url(url: &str) -> String` (unit-tested) |
| `ssh.rs` | `get_tunnel_url` остаётся (legacy); WS-ветка конвертирует схему сама |

### Что НЕ меняется
- `ssh.rs` логика туннеля (`start_ssh_tunnel`, `is_tunnel_active`) — generic TCP,
  работает для WS как есть.
- SSH-конфиг (`SshConfig`) — структура полей та же.
- Frontend `connectionStore` — поля те же; интерпретация `api_key` как session
  token прозрачно для UI (label переименуется отдельно).

## Последствия

- **Положительные**: WS становится единственным транспортным путём во ВСЕХ
  режимах → **разблокируется P2.1** (удаление ~600 строк HTTP dead code:
  `send_message_via_api`, `send_message_via_runs`, `supports_runs_transport`,
  `send_via_best_transport`, `parse_runs_event`).
- **Отрицательные**: Remote/SSH теперь требуют session token (не API_SERVER_KEY).
  Пользователи, у которых настроен Remote с `API_SERVER_KEY`, должны будут
  ввести session token удалённого бэкенда. Это breaking change для Remote config
  (но Remote-режим и так не работал — endpoint `/v1/chat/completions` отсутствует).
- **Риски**: SSH-туннель + WS — нужно подтвердить, что tungstenite корректно
  работает через loopback forwarding (должно, т.к. это transparent TCP).

## Верификация

P3.2 не требует живой удалённой установки для валидации кода:
- `to_ws_url` — unit-тест (чистая функция).
- Remote/SSH ветки — мигрируются на тот же `send_message_via_ws`, что уже
  доказан e2e в Phase 0/P1.
- SSH-туннель: WS-over-SSH — стандартный паттерн; туннель generic TCP.
- Полное e2e для Remote/SSH требует реальной удалённой машины — это принимается
  как runtime-верификация пользователем, не блокер для code-миграции.

## Статус предыдущих ADR

- **ADR-004**: Remote/SSH больше не «вне scope» — этот ADR их покрывает.
- **ADR-002/003**: транспортные части (`/v1/chat/completions`, 8642,
  `API_SERVER_KEY`) окончательно устарели для всех режимов.
