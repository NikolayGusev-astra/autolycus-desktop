# ADR-002: API-контракт с Hermes Agent — ground truth

> **Статус:** принято  
> **Дата:** 2026-07-12  
> **Решение:** Привести порт в соответствие с реальным API Hermes Agent

---

## Контекст

ADR-001 принял решение портировать fathah/hermes-desktop на Tauri 2 + Rust.
Порт был выполнен "по интуиции" — без сверки с исходниками Hermes Agent.
Спустя 3 месяца эксплуатации выявились систематические расхождения между
портом и upstream-контрактом. Чат не работает (403 от upstream), брифинг
показывает нули, сессии теряются, креды не доходят до MCP-серверов.

Полный аудит (3 параллельных исследования исходников) выявил **21 расхождение**
с ground truth. Этот ADR фиксирует контракт, выявленный из исходников, чтобы
исключить дальнейший брутфорс кодом.

## Источники ground truth

| Источник | Метод | Что покрыто |
|----------|-------|------------|
| `fathah/hermes-desktop` (оригинал) | WebFetch всех .ts файлов | Chat transport (3 варианта), session ID, auth, gateway spawn, IPC контракт, SSE parsing |
| `NousResearch/hermes-agent` (upstream) | WebFetch репозитория + docs | API endpoints, auth, config.yaml schema, MCP env whitelist, proxy chain, model resolution |
| Локальная установка `C:\Users\n.gusev\AppData\Local\hermes` | Чтение config.yaml, .env, server.py, logs/ | Реальная конфигурация, actual source values, logs ошибок |

## Найденный API-контракт

### Hermes Agent содержит ТРИ различных сервера

| Сервер | Транспорт | Назначение | Порт |
|--------|-----------|------------|------|
| **1. TUI Gateway / Dashboard WS** (`tui_gateway/` + `hermes_cli/web_server.py`) | WebSocket JSON-RPC 2.0 `/api/ws` | Официальное Hermes Desktop (`apps/desktop/`) | dashboard |
| **2. API Server platform** (`gateway/platforms/api_server.py`) | HTTP REST, OpenAI-compatible `/v1/chat/completions` | "Hermes-as-backend" для Open WebUI/LibreChat/нашего десктопа | **8642** |
| **3. Subscription Proxy** (`hermes_cli/proxy/`) | HTTP OpenAI-compatible `/v1` | Сырой inference, без agent/tools | 8645 |

**Наш порт использует сервер #2** (HTTP API Server) — это корректно для Tauri-десктопа, но ВСЕ детали запроса должны соответствовать контракту сервера #2.

### Chat endpoint контракт (сервер #2)

```
POST http://127.0.0.1:8642/v1/chat/completions
Authorization: Bearer <API_SERVER_KEY>          (обязательно, ≥16 символов)
Content-Type: application/json
Content-Length: <точная длина body в байтах>     (требует middleware)
X-Hermes-Session-Id: desk-<timestamp>-<uuid4>   (опционально, но требует auth)

Body:
{
  "model": "<ignored unless model_routes configured>",
  "messages": [{"role":"user","content":"..."}],
  "stream": true,
  "session_id": "desk-<ts>-<uuid>"               (только при resume),
  "reasoning_effort": "medium"                    (top-level строка, не вложенный объект!)
}
```

**Критически важно:**
- `model` в body **игнорируется** gateway — реальная модель из `config.yaml model.default`
- `session_id` — это **HEADER** (`X-Hermes-Session-Id`), не body-поле. Body-поле только при resume.
- `reasoning_effort` — **top-level строка**, не вложенный `reasoning:{effort,context}`
- Отправка `X-Hermes-Session-Id` **без** `API_SERVER_KEY` → **403**
- Неверный `API_SERVER_KEY` → **401** (не 403!)
- 403 от upstream provider проходит **сквозь** gateway как `hermes.failed:true` в теле 200-ответа

### Три chat-транспорта в оригинале (fathah/hermes-desktop)

| Транспорт | Endpoint | Когда используется |
|-----------|----------|-------------------|
| **A. Runs API (PREFERRED)** | `POST /v1/runs` + `GET /v1/runs/{id}/events` (SSE) | Capability-detected через `GET /v1/capabilities`. Agent tool loops, reasoning events. |
| **B. Chat Completions (FALLBACK)** | `POST /v1/chat/completions` | Если Runs не поддерживается |
| **C. TUI Gateway (WS)** | `ws://host:port/api/ws` | Если нет аттачментов, нет model override, не remote |

Наш порт использует только **B**. Transport A (`/v1/runs`) не реализован → теряются tool events и reasoning stream.

### Gateway spawn контракт (fathah/hermes-desktop)

```typescript
spawn(HERMES_PYTHON, hermesCliArgs(["gateway"]), {
  cwd: HERMES_REPO,           // ОБЯЗАТЕЛЬНО — source checkout
  detached: true,
  stdio: ["ignore","ignore", stderrFd]
})
```

Env vars для gateway:
```
...process.env                 // ПОЛНОЕ наследование (включая прокси из shell)
PATH = getEnhancedPath()       // augmented с venv/Scripts
HOME = <homedir>
HERMES_HOME = <resolved>
API_SERVER_ENABLED = "true"    // ОБЯЗАТЕЛЬНО — включает API server
API_SERVER_PORT = "<port>"
+ ALL keys from readEnv(profile)  // ВСЕ ключи .env, не только выборочные
+ API_SERVER_KEY = <resolved>
```

**Профиль через CLI:** `--profile <name>` flag (не `HERMES_PROFILE_HOME` env var).

**Proxy:** оригинал НЕ делает явной инъекции — наследует `process.env`. Но upstream Hermes Agent читает: `HTTPS_PROXY` → `HTTP_PROXY` → `ALL_PROXY` (case-insensitive).

### Health check контракт

```
GET /health (public, без auth)
→ 200 {"status":"ok","platform":"hermes-agent","version":"<ver>"}
```

Оригинал: non-blocking 3s setTimeout → poll `/health`. Наш порт: 30s blocking TCP connect — это **неправильно**, TCP accept ≠ HTTP ready.

### Config.yaml schema (upstream NousResearch/hermes-agent)

```yaml
model: ""                        # строка, не объект
providers: {}
fallback_providers: []
network:
  force_ipv4: false              # НЕТ proxy subkey в upstream!
platforms:
  api_server:
    host: 127.0.0.1
    port: 8642
    key: <API_SERVER_KEY>        # может быть здесь или в .env
    model_routes: {}
mcp_servers:
  <name>:
    command: <path>
    args: [...]
    enabled: true
    env:                          # ОБЯЗАТЕЛЬНО для кред!
      EMAIL_ADDRESS: <value>
      JIRA_PAT: <value>
```

**Критично:** секции `500-network`, top-level `proxy:`, `model.proxy` — **НЕ существуют** в upstream. Это fork/пользовательские дополнения. Правильное место для прокси — env vars (`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY`).

### MCP env whitelist (самая частая причина пустых данных)

Hermes Agent запускает MCP-серверы через `_build_safe_env()` (`tools/mcp_tool.py:425`):
```
whitelist = [PATH, HOME, USER, LANG, SYSTEMROOT, TEMP, ...]
→ ВЫРЕЗАЕТ всё остальное (EMAIL_*, JIRA_*, KILOCODE_API_KEY, ...)
```

**Креды ДОЛЖНЫ быть в config.yaml `mcp_servers.<name>.env:` блоке** — иначе они не дойдут до MCP-сервера. Запись в `.env` НЕ работает для MCP-серверов, запущенных Hermes Agent.

### Брифинг — НЕ существует в оригинале

В `fathah/hermes-desktop` **нет кода брифинга**. Это invention нашего порта. Поэтому:
- Нет reference-реализации для сверки
- "Брифинг не работает" = design divergence, не porting regression
- Брифинг-сервер spawn'ит email/jira MCP — те не получают креды (см. whitelist)

---

## Решение

### Принятые принципы

1. **Единый источник правды** — этот ADR. Код должен соответствовать контракту здесь, не наоборот.
2. **Без брутфорса** — любое изменение API/запроса сверяется с этим документом.
3. **Совместимость с upstream** — используем ровно те поля/headers/env, что ждёт Hermes Agent.

### Изменения к реализации

См. **SDD.md §7** — полный список из 21 расхождения с severity и планом фиксов.

## Последствия

- Положительные: чат заработает (session+auth), брифинг получит креды (MCP env blocks), прокси найдётся (правильный chain)
- Отрицательные: требуется касаться `config.yaml` Hermes (добавлять `env:` блоки) — это конфиг пользователя, не только наш код
- Риски: transport A (`/v1/runs`) — большой объём реализации; можно отложить, B работает если починить session/auth
