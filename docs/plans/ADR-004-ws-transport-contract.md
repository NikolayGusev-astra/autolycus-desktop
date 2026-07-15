# ADR-004: Реальный транспортный контракт — `hermes serve` + WebSocket `/api/ws`

> **Статус:** принято ( supersedses транспортные части ADR-002 и ADR-003 )
> **Дата:** 2026-07-15
> **Решение:** Перевести чат-движок Steersman с HTTP API Server (сервер #2,
> порт 8642) на WebSocket JSON-RPC (сервер #1, `hermes serve`), потому что
> именно этот транспорт использует оригинальный Hermes Desktop от NousResearch,
> а сервер #2 в актуальной установке upstream **отсутствует**.

---

## Контекст

ADR-002 зафиксировал как ground truth, что десктоп общается с Hermes через
**HTTP API Server #2** (`POST /v1/chat/completions`, порт 8642, заголовок
`X-Hermes-Session-Id`, `API_SERVER_KEY` в `.env`). Весь чат-движок Steersman
(`src-tauri/src/chat.rs`) написан под этот контракт.

**Инспекция живой установки** (`C:\Users\n.gusev\AppData\Local\hermes`,
версия upstream на 2026-07-15) показала, что этот контракт в реальности
**не работает**:

| Проверка | Результат |
|---|---|
| `curl http://127.0.0.1:8642/health` | `HTTP 000` — **порт не слушается** |
| `grep API_SERVER_KEY .env` | **отсутствует** (только `OPENROUTER_API_KEY`) |
| `grep platforms/api_server/mcp_servers config.yaml` | **секций нет** |
| `hermes gateway --help` | *«Manage the messaging gateway (Telegram, Discord, WhatsApp)»* — **это мессенджер-интеграции, не HTTP API** |

Запущенный бэкенд слушает `127.0.0.1:64724` и отвечает:
`{"error":"Headless backend (hermes serve): web UI disabled — use hermes dashboard"}`.
Это **сервер #1 (TUI Gateway)**, поднятый командой `hermes serve`.

### Как оригинальный Hermes Desktop реально spawn'ит бэкенд

Доказательство — исходник оригинального десктопа
(`apps/desktop/release/win-unpacked/resources/app.asar → dist/electron-main.mjs`):

```js
const token = crypto.randomBytes(32).toString("base64url");
const backendArgs = ["--profile", profile, "serve", "--host", "127.0.0.1", "--port", "0"];
// ... spawn backend, передать HERMES_DASHBOARD_SESSION_TOKEN = token ...
wsUrl: `ws://127.0.0.1:${port}/api/ws?token=${encodeURIComponent(authToken)}`
```

Маркеры, **которые в коде оригинального десктопа отсутствуют**: `hermes gateway`
(как команда запуска бэкенда), `API_SERVER_ENABLED`, `API_SERVER_KEY`, `8642`,
`/v1/chat/completions`, `/v1/runs`, `/v1/capabilities`, `pythonw.exe`.

### Почему ADR-002/003 оказались неверны

`hermes gateway` в CLI upstream — это **мессенджер-сервис** (Telegram/Discord/
WhatsApp), а не HTTP API-сервер. Команда `hermes serve` поднимает бэкенд.
ADR-002, по-видимому, опирался на более старую или иную версию Hermes, где
HTTP API Server существовал; в актуальной установке его нет, а транспорт
десктопа — это WS.

---

## Контракт (ground truth из исходников upstream)

### 1. Spawn бэкенда

```
<HERMES_HOME>/hermes-agent/venv/Scripts/python.exe \
  -m hermes_cli.main --profile <name> serve --host 127.0.0.1 --port 0
```

- `--port 0` → ОС выдаёт **случайный свободный порт** (текущий пример: 64724).
  Дефолт `9119` (`hermes serve --help`), но десктоп deliberately просит `0`,
  чтобы избежать конфликтов. **Порт нужно прочитать из stdout бэкенда** (см. handshake).
- `--profile <name>` передаётся как CLI arg (это совпадает с ADR-003 §профиль).
- `cwd = HERMES_REPO` (source checkout) — обязательно (совпадает с ADR-003).
- `host = 127.0.0.1` → loopback → auth gate **выключен** (`should_require_auth`
  возвращает False для loopback, `web_server.py:398-417`).

### 2. Auth: session token

```
_SESSION_TOKEN = os.environ.get("HERMES_DASHBOARD_SESSION_TOKEN")
                 or secrets.token_urlsafe(32)              # web_server.py:279
```

Десктоп генерирует случайный 32-байтный токен, передаёт его бэкенду через env
`HERMES_DASHBOARD_SESSION_TOKEN`, а затем использует в WS-подключении. Валидация
(`web_server.py:324-341`, `hmac.compare_digest`), принимается любой из:

- query: `?token=<_SESSION_TOKEN>` ← **используется десктопом**
- header: `X-Hermes-Session-Token: <_SESSION_TOKEN>`
- header: `Authorization: Bearer <_SESSION_TOKEN>` (legacy)

На loopback без токена запросы на `/api/*` → **401** (это мы и наблюдали).

### 3. Handshake

После accept WS-соединения сервер немедленно шлёт событие готовности
(`tui_gateway/ws.py:324`):

```json
{"jsonrpc": "2.0", "method": "event", "params": {"type": "gateway.ready", ...}}
```

Wire protocol (`tui_gateway/ws.py:11`): *«newline-delimited JSON-RPC in both
directions. Identical to stdio.»* — каждое сообщение — отдельная JSON-строка,
разделённая `\n`.

### 4. JSON-RPC методы чата

| Метод | Назначение | Файл:строка upstream |
|---|---|---|
| `session.create` | Создать сессию (возвращает `session_id`) | `tui_gateway/server.py:5205` |
| `session.list` | Список сессий | `server.py:5351` |
| `session.interrupt` | Прервать текущий turn | `server.py:8158` |
| **`prompt.submit`** | **Отправить сообщение агенту** — главный метод | `server.py:8464` |

`prompt.submit` params: `{ "session_id": "<sid>", "text": "<user message>" }`.
Это **асинхронный** RPC: ответ `{result}` возвращается, когда turn стартовал,
а сами токены/инструменты приходят отдельными **событиями** (см. ниже) по тому
же WS-соединению. Steersman не должен ждать полный ответ в одном сообщении.

### 5. Streaming-события (сервер → клиент)

Эмитятся через `_emit(event_type, sid, payload)` (`server.py:3648-3921`).
Steersman уже имеет парсер этих типов в `chat.rs:409-509` (`parse_gateway_event`) —

| Событие upstream | ChatEvent в Steersman | Что несёт |
|---|---|---|
| `message.chunk` / `message.delta` | `Token` | текст токена |
| `reasoning.delta` / `thinking.delta` | `Reasoning` | thinking-контент |
| `tool.start` | `ToolStart` | имя инструмента + tool_id |
| `tool.complete` | `ToolComplete` | результат + длительность |
| `approval.request` | `ApprovalRequest` | запрос подтверждения действия |
| `status.update` | `Status` | изменение статуса turn'а |
| `message.end` / `done` | `Done` | завершение (с `session_id`) |
| `error` | `Error` | ошибка с сообщением |

> **Важно:** `_STREAMING_EVENT_TYPES = {message.delta, reasoning.delta,
> thinking.delta}` коalesce'ятся (буфер + flush ~30 fps, `ws.py:55-72`).
> Не-streaming события (tool/approval/status/completion) flush'ат буфер вперед
> себя — ordering сохранён.

### 6. Что НЕ существует в актуальном upstream

`grep` по всему `hermes-agent/` (папки `gateway/`, `tui_gateway/`,
`hermes_cli/`):

- **`/v1/chat/completions`** — не найден
- **`/v1/runs`** — не найден
- **`/v1/capabilities`** — не найден
- **`API_SERVER_ENABLED` / `API_SERVER_KEY`** — не в коде десктопа

Эти endpoint'ы и env-переменные — основа текущего `chat.rs`. Любой запрос к ним
гарантированно падает (connection refused на 8642).

---

## Решение

### Принципы

1. **Единый транспорт** — WS `/api/ws` JSON-RPC 2.0, как в оригинальном десктопе.
   Никакого HTTP `/v1/*` до тех пор, пока upstream его не вернёт.
2. **`hermes serve`, не `hermes gateway`** — spawn команда и env из §1 выше.
3. **Случайный порт** — `--port 0`, чтение реального порта из stdout/handshake.
4. **Token auth** — генерация `HERMES_DASHBOARD_SESSION_TOKEN`, передача в env
   бэкенду и в `?token=` WS-URL.
5. **Асинхронный чат** — `prompt.submit` стартует turn, токены приходят events.

### Что меняется в `src-tauri/src/chat.rs`

| Компонент | Статус | Действие |
|---|---|---|
| `send_message_via_api` (HTTP `/v1/chat/completions`) | Невалиден | Удалить или оставить как dead-code с пометкой |
| `send_message_via_runs` / `supports_runs_transport` | Невалиден (endpoint'а нет) | Удалить |
| `send_via_best_transport` | Невалиден | Удалить |
| **`send_message_via_gateway`** (`chat.rs:354-407`) | **Был мёртвым, станет главным** | Доработать до WS-клиента: connect, auth, `session.create`, `prompt.submit`, event-loop |
| **`parse_gateway_event`** (`chat.rs:409-509`) | Уже соответствует upstream | Использовать как основной парсер |

### Что меняется в `src-tauri/src/gateway.rs`

| # | Что (по ADR-003) | Станет (по ADR-004) |
|---|---|---|
| 1 | Spawn `gateway` + `API_SERVER_ENABLED=true` | Spawn `serve --host 127.0.0.1 --port 0` |
| 2 | `API_SERVER_KEY` bridge в env | `HERMES_DASHBOARD_SESSION_TOKEN=<random>` в env |
| 3 | Порт фиксирован 8642 | Порт читается из stdout после spawn |
| 4 | Health: HTTP `GET /health` (сервер #2) | Handshake: WS accept + `gateway.ready` event |

`--profile`, `cwd = HERMES_REPO`, наследование `process.env`, прокси через env —
**остаются в силе** (это корректные части ADR-003).

---

## Последствия

- **Положительные:** чат заработает с реальной установкой; tool events и
  reasoning stream пойдут из коробки (сервер их эмитит); `parse_gateway_event`
  уже написан и соответствует upstream — минимум нового кода.
- **Отрицательные:** ~600 строк HTTP-транспорта в `chat.rs` (`send_message_via_api`,
  `send_message_via_runs`, `supports_runs_transport`, `send_via_best_transport`)
  становится dead code под удаление. Нужен WS-клиент на Rust (`tokio-tungstenite`).
- **Риски:** WS-протокол асинхронный — требует пересмотра flow в `gatewayStore`
  (TS): ответ приходит не в одном сообщении, а потоком events. Это влияет на
  streaming-логику в `ChatView`.
- **Новые зависимости:** `tokio-tungstenite` (WS-клиент) — требует записи в ADR
  согласно ограничениям задачи P0 (новые crate → только с ADR; этот ADR её и
  покрывает).

### Статус ADR-002 / ADR-003

- **ADR-002**: раздел «Chat endpoint контракт (сервер #2)» и описание трёх
  серверов — **устарели** для актуальной установки. Разделы про config.yaml
  schema, MCP env whitelist, proxy chain — **остаются в силе**.
- **ADR-003**: принципы `--profile` CLI, `cwd=HERMES_REPO`, наследование
  `process.env`, прокси через env — **остаются в силе**. Команда spawn
  (`gateway` → `serve`), `API_SERVER_*`, порт 8642 — **устарели**, см. ADR-004.

### Что не покрывает этот ADR

- Подробная имплементация WS-клиента (отдельная задача, см. пересмотренный план).
- Remote/SSH режимы — там бэкенд чужой, контракт может отличаться (требует
  отдельной проверки удалённой установки).
- Брифинг — это invention порта (ADR-002 §«Брифинг не существует в оригинале»),
  его transport-зависимость рассматривается отдельно.

---

## Приложение: покрытие Phase 0 (2026-07-15)

Phase 0 реализовала ядро WS-транспорта и **доказала его жизнеспособность e2e**
на живом `hermes serve`. Конкретно сделано:

| Артефакт | Файл | Назначение |
|---|---|---|
| `parse_ws_message` | `chat.rs:537` | Dispatcher wire-envelope JSON-RPC → `ChatEvent` |
| фикс `session_id` в `done` | `chat.rs` (message.end) | upstream шлёт session_id в params; раньше терялся |
| `send_message_via_ws` | `ws_transport.rs` | WS-клиент: connect → gateway.ready → session.create → prompt.submit → events |
| `send_via_ws_local` | `chat.rs:865` | Local-обёртка: строит WS URL из порта + токена |
| переключатель `STEERSMAN_TRANSPORT` | `chat.rs:915` | `=ws` включает WS, дефолт `http` (для отката) |
| `tokio-tungstenite` dep | `Cargo.toml` | WS-клиент (rustls, совпадает со стеком reqwest) |
| 8 тестов | `chat.rs mod tests` | RED→GREEN: token/reasoning/tool/done/error/status/rpc-response/garbage |

**E2E-доказательство:** поднят изолированный `hermes serve --port 9420` с
известным токеном → `HTTP 101 Switching Protocols` → `gateway.ready` event →
`session.create` → `sid=5ab8c71a`. Транспорт + auth + handshake + session
верифицированы на реальной установке.

**Важное уточнение риска (исправление):** в §Последствия выше предполагалось,
что WS потребует пересмотра `gatewayStore`/`ChatView`. Phase 0 доказала
обратное — фронтенд **не требует изменений**: WS-клиент эмитит события в
тот же Tauri-канал `chat_event`, который `ChatView.tsx:65-249` уже слушает.
Граница контракта Rust↔фронтенд не сдвинулась.

**Что Phase 0 НЕ покрыла (→ P1):** spawn `gateway.rs` всё ещё зовёт `gateway`
(не `serve`), порт хардкожен `allocate_port` (8642), токен берётся из env
(не генерируется), WS не дефолт. Это задачи P1 (`docs/plans/tasks/P1-ws-wiring.md`).
