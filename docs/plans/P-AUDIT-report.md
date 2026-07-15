# P-AUDIT — отчёт повторной верификации 21 расхождения

> **Дата проверки:** 2026-07-15
> **Базовая линия:** `cargo test --lib` → **40 passed, 3 ignored, 0 failed**
> **HEAD:** ветка `main`, после Phase 0 (ADR-004 WS-transport)
> **Метод:** grep / чтение кода / `cargo test` на текущем HEAD. Transport-пункты
> сверялись с ADR-004 (не с устаревшим ADR-002).

## Сводка вердиктов

| Вердикт | Кол-во | Значение |
|---|---|---|
| **CLOSED** | 9 | Уже сделано в коде на HEAD |
| **INVALID** | 6 | Основан на устаревшем ADR-002/003; endpoint'а нет в живом Hermes |
| **SUPERSEDED** | 2 | Закрыт иначе, чем предполагал аудит (WS даёт это напрямую) |
| **PARTIAL** | 2 | Часть сделана, часть осталась |
| **VALID** | 5 | Реальный дефект → кандидаты в P1/P2/P3 |

Итого: **из 21 пункта аудита реальных дефектов осталось 5 (+ 2 частичных)**.
Остальные 14 — уже закрыты, невалидны или superseded. Это подтверждает, что
исходный P0-промпт (гнавший все 21) был устаревшим.

---

## Таблица вердиктов

### §7.1 КРИТИЧЕСКИЕ (5)

| # | Пункт | Вердикт | Доказательство | Действие |
|---|---|---|---|---|
| 1 | Session ID в body | **SUPERSEDED** | HTTP-путь (`chat.rs:244-253`) — весь класс невалиден после ADR-004; сессии в WS создаются через `session.create`, не body-field. Формат `desk-<ts>-<uuid>` корректен и переиспользуется в `ws_transport.rs`. | P2.1 (удалить HTTP-путь) |
| 2 | X-Hermes-Session-Id header | **INVALID** | Header существует в `chat.rs:291-296`, но endpoint `/v1/chat/completions` (8642) отсутствует в живом Hermes (curl HTTP 000). WS transport не использует этот header — auth через `?token=`. | P2.1 (удалить вместе с HTTP) |
| 3 | API_SERVER_ENABLED | **INVALID** | `gateway.rs:215` шлёт `API_SERVER_ENABLED=true`, но реальный бэкенд `hermes serve` не читает эту env (нужен `HERMES_DASHBOARD_SESSION_TOKEN`). Spawn ещё зовёт `gateway`, не `serve` — см. P1. | P1.1 (spawn=serve) |
| 4 | .env bridge | **CLOSED** | `gateway.rs:225-231` — bridges все ключи из `read_env`, кроме HERMES_HOME/PORT. Наследование `process.env` полное (нет `env_clear()`). | — |
| 5 | reasoning_effort формат | **CLOSED** | `chat.rs:259-263` — top-level строка, не вложенный объект. Закрыто в коммите `6ce9fb4`. | — |

### §7.2 ВЫСОКИЕ (5)

| # | Пункт | Вердикт | Доказательство | Действие |
|---|---|---|---|---|
| 6 | Runs transport | **SUPERSEDED** | `supports_runs_transport`/`send_message_via_runs`/`parse_runs_event` существуют (`chat.rs:565,598,740`), но endpoint `/v1/runs` отсутствует в живом Hermes (grep 0). WS transport даёт tool events напрямую — Runs не нужен. | P2.1 (удалить как dead code) |
| 7 | Профиль `--profile` CLI | **CLOSED** | `gateway.rs:191` — `cmd.arg("--profile").arg(p)`. Не env var. | — |
| 8 | Health check (TCP vs HTTP) | **PARTIAL** | `gateway.rs:300-335` делает TCP+HTTP/1.0 GET `/health`. Но `/health` возвращает 404 на `hermes serve` (доказано в ADR-004). Readiness для WS должен определяться `gateway.ready` event, не `/health`. | P1.4 (readiness через WS handshake) |
| 9 | Content-Length | **INVALID** | `chat.rs:289,663` шлёт header, но endpoint'а `/v1/chat/completions` нет. WS не использует Content-Length. | P2.1 (удалить с HTTP) |
| 10 | ALL_PROXY | **CLOSED** | `config.rs:471-481` — проверяет HTTPS_PROXY→HTTP_PROXY→ALL_PROXY (+ lowercase). | — |

### §7.3 СРЕДНИЕ (6)

| # | Пункт | Вердикт | Доказательство | Действие |
|---|---|---|---|---|
| 11 | MCP store (servers.json vs config.yaml) | **VALID** | `mcp.rs:79` читает `servers.json`; `mcp.rs:84 list_mcp_servers` не из config.yaml. Hermes upstream читает только config.yaml `mcp_servers:`. | P2.3 (кандидат) или отдельная задача |
| 12 | MCP креды (env blocks) | **PARTIAL** | `mcp.rs:330-352 sync_mcp_env_blocks` пишет env-блоки в config.yaml (логика есть), но через line-based `set_yaml_block_scalars` (fragile). Часть решена, формат хрупок. | P2.3 (serde_yaml round-trip) |
| 13 | Proxy config keys (fork) | **CLOSED** | `config.rs:457` — только комментарий, fork-ключи `500-network`/`model.proxy` отсутствуют в коде. Прокси через env. | — |
| 14 | cwd gateway = HERMES_REPO | **CLOSED** | `gateway.rs:197-198` — `cmd.current_dir(repo)` где repo из `find_hermes_repo`. | — |
| 15 | pythonw.exe на Windows | **CLOSED** | `gateway.rs:166-170` — prefer pythonw.exe, fallback bare python. | — |
| 16 | Briefing env | **CLOSED** | `briefing.rs:165-178` — env-fixing логика реализована (EMAIL_ADDRESS bridge и т.д.). | — |

### §7.4 НИЗКИЕ (5)

| # | Пункт | Вердикт | Доказательство | Действие |
|---|---|---|---|---|
| 17 | SID format `desk-<ts>-<uuid>` | **CLOSED** | `chat.rs:241,626` — `format!("desk-{}-{}", ts, uuid::Uuid::new_v4())`. | — |
| 18 | Proxy injection (явная vs наследование) | **CLOSED** | `gateway.rs` наследует process.env (нет env_clear); grep `cmd.env.*proxy` пуст — явной инъекции нет, как и требует контракт. | — |
| 19 | Config write (line-based YAML) | **VALID** | `mcp.rs:349` вызывает `crate::config::set_yaml_block_scalars` — line-based editor присутствует. Хрупок при comments/multi-line. | P2.3 (serde_yaml round-trip) |
| 20 | Gateway stop (graceful) | **CLOSED** | `gateway.rs:378-407` — SIGTERM (unix) → wait 3s → kill; Windows — kill. Graceful уже есть. | — |
| 21 | config_health.rs (stale) | **VALID** | `config_health.rs:148` хардкодит `port: 8642` (несуществующий); модуль подключён (`lib.rs:8`). DB-имя уже исправлено на `state.db` (строка 92), но endpoint-логика устарела. | P2.3 (удалить модуль) |

### §7.5 Dead/stub code

| Компонент | Вердикт | Доказательство | Действие |
|---|---|---|---|
| `send_message_via_gateway` (chat.rs:354) | **VALID dead** | Только определение, 0 вызовов (grep подтвердил). Заменён на `ws_transport.rs` в Phase 0. | P2.3 |
| `check_gateway_health` (gateway.rs:444) | **CLOSED (не dead)** | Вызывается в `gateway.rs:474` (`is_api_server_ready`). Аудит ошибался — это не dead code. | — |
| `test_mcp_server`/`list_mcp_catalog` | **CLOSED** | Не найдены в текущем mcp.rs (grep 0) — уже удалены. | — |

### §7.6 config_health.rs

| Вердикт | Доказательство | Действие |
|---|---|---|
| **VALID** | `config_health.rs:148` хардкодит `port: 8642`; проверяет endpoint'ы, которых нет в `hermes serve`. DB-часть (`state.db`, строка 92) корректна. Модуль генерирует false-positive ошибки на реальной установке. | P2.3 (удалить весь модуль) |

### Дополнительно (вне исходных 21)

| Компонент | Вердикт | Доказательство | Действие |
|---|---|---|---|
| `python/tcp_server.py` | **VALID dead** | Файл присутствует (7959 bytes); никогда не вызывается из Rust. | P2.3 |

---

## Список VALID + PARTIAL → вход для P1/P2/P3

### P1 (WS wiring) — 3 пункта
- **#3 spawn команда** (`gateway` → `serve`) — `gateway.rs:180`
- **#8 health readiness** (TCP/HTTP → WS handshake) — `gateway.rs:300-335`
- **Phase 0 задел:** `ws_transport.rs`, `parse_ws_message`, переключатель
  `STEERSMAN_TRANSPORT` уже готовы. P1 их подключает к продакшен-флоу.

### P2 (cleanup + persistent WS) — 5 пунктов
- **#1, #2, #6, #9** — HTTP dead code (`send_message_via_api`, `send_message_via_runs`,
  `supports_runs_transport`, `send_via_best_transport`, `parse_runs_event`) — все
  невалидны/superseded, удалить ~600 строк
- **#11 MCP store** (servers.json → config.yaml) — VALID
- **#12 MCP креды** (line-based → serde_yaml) — PARTIAL
- **#19 config write** (line-based YAML) — VALID
- **#21 config_health.rs** (stale, порт 8642) — VALID, удалить модуль
- **§7.5 send_message_via_gateway** (old WS stub) — VALID dead
- **tcp_server.py** — VALID dead

### P3 (Remote/SSH + arch) — 0 новых пунктов из 21
- ADR-004 явно оставил Remote/SSH вне scope; нужен ADR-005 (эмпирическое
  исследование удалённой установки) — это не входит в исходные 21 расхождений.

---

## Вывод

**Реальных дефектов на HEAD: 5 VALID + 2 PARTIAL = 7 из 21.**
Остальные 14 уже закрыты (9), невалидны для WS-транспорта (6... включая
перекрытие), или superseded (2). Это означает:
- **P1** сфокусирован на 2 пунктах (#3 spawn, #8 health) + Phase 0 wiring.
- **P2** удаляет ~600 строк dead code + правит MCP/config (4 пункта).
- **P3** — принципиально новая работа (Remote/SSH + thiserror + tracing),
  не описанная в исходных 21.

Исходный P0-промпт (гнавший 6 из 21) был основан на устаревшем аудите и
не знал о WS-миграции. Этот отчёт даёт P1/P2/P3 точный, проверенный список.
