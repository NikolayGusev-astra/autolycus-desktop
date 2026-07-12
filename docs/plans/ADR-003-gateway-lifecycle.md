# ADR-003: Gateway lifecycle и мост окружения

> **Статус:** принято  
> **Дата:** 2026-07-12  
> **Решение:** Привести lifecycle gateway в соответствие с оригиналом

---

## Контекст

ADR-002 зафиксировал API-контракт. Этот ADR детализирует lifecycle gateway:
спавн процесса, проброс окружения, health-check, профили, прокси.
Текущая реализация (`gateway.rs`) имеет 8 расхождений с оригиналом, из них
3 критические (ломают старт/работу gateway).

## Контракт gateway spawn (ground truth из fathah/hermes-desktop)

### Команда

```
Windows:  <HERMES_HOME>/hermes-agent/venv/Scripts/pythonw.exe -m hermes_cli.main gateway
Linux:    <HERMES_HOME>/hermes-agent/venv/bin/python <HERMES_REPO>/hermes gateway
```

**cwd = HERMES_REPO** (source checkout) — обязательно, т.к. gateway и зависимости
резолвят относительные пути (config, mcp servers, skills) от cwd.

### Профиль

```
--profile <name>          # CLI flag (НЕ env var HERMES_PROFILE_HOME)
```

Оригинал передаёт профиль через CLI аргумент. `HERMES_PROFILE_HOME` env var —
недокументирован и может не активировать profile loader в Hermes Agent.

### Окружение

Оригинал наследует **полный `process.env`** и добавляет:

```python
PATH             = getEnhancedPath()       # augmented с venv/Scripts, git/bin
HOME             = <homedir>
HERMES_HOME      = <resolved>
API_SERVER_ENABLED = "true"                # ВКЛЮЧАЕТ API server — критично!
API_SERVER_PORT  = "<profile port>"
# + ВСЕ ключи из readEnv(profile) — не только выборочные
# + API_SERVER_KEY = <resolved>            # bridge в process env
```

### Прокси

Оригинал **не делает явной инъекции** прокси — наследует `process.env`.
Но upstream Hermes Agent читает env в порядке:
```
HTTPS_PROXY → HTTP_PROXY → ALL_PROXY   (case-insensitive)
```

Схемы: `http://`, `https://`, `socks5://`.

**В config.yaml upstream НЕТ proxy-ключей** (нет `network.proxy`, нет `500-network`).
Прокси передаётся только через env vars. Если пользователь работает в shell
с `HTTP_PROXY=http://127.0.0.1:12334`, gateway наследует это.

### Health-check

```
GET /health → 200 {"status":"ok","platform":"hermes-agent","version":"<ver>"}
```

Оригинал: non-blocking 3s `setTimeout`, затем poll `/health` HTTP endpoint.
**TCP connect ≠ HTTP ready** — gateway может принять TCP-соединение пока
Python ещё импортирует модули → чат-запрос упадёт на half-started server.

### Останов

```python
# Windows: нет graceful SIGTERM эквивалента
child.kill()  # immediate terminate
```

В лётных запросах нет состояния — immediate kill допустим.

## Решение

### Принципы

1. **Наследовать process.env полностью** — не фильтровать, не выборочно инжектить
2. **Дополнять, не заменять** — добавляем API_SERVER_ENABLED, PATH, .env bridge поверх
3. **Profile через CLI** — `--profile <name>` flag
4. **Health через HTTP** — poll `/health`, не TCP
5. **Прокси через env** — `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY`, не config.yaml ключи

### Что меняется в gateway.rs

| # | Что | Было | Станет |
|---|-----|------|--------|
| 1 | process.env | Не наследуется (только выборочные ключи) | `cmd.env_clear()` НЕ вызываем — наследуем полностью |
| 2 | API_SERVER_ENABLED | Не устанавливается | `cmd.env("API_SERVER_ENABLED", "true")` |
| 3 | .env bridge | Нет | Читаем `.env`, инжектим все ключи |
| 4 | API_SERVER_KEY bridge | Нет | Инжектим в process env gateway |
| 5 | cwd | Не установлен | `cmd.current_dir(HERMES_REPO)` |
| 6 | pythonw.exe | Bare `python` | `pythonw.exe` на Windows (no console) |
| 7 | --profile | `HERMES_PROFILE_HOME` env | `--profile <name>` CLI arg |
| 8 | health | TCP connect | HTTP `/health` poll |

### Что меняется в config.rs (прокси-резолвер)

| # | Что | Было | Станет |
|---|-----|------|--------|
| 9 | Источники прокси | config.yaml `500-network.proxy` (fork-ключи) | env: `HTTPS_PROXY` → `HTTP_PROXY` → `ALL_PROXY` |
| 10 | System proxy | Регистр Windows, scutil, gsettings | Оставить (fallback когда env пуст) |

## Последствия

- Положительные: gateway стартует в окружении, идентичном `hermes` CLI → чат работает
- Отрицательные: меняется семантика env — теперь наследуем ВСЁ, включая потенциально нежелательные vars
- Риски: полный process.env наследование требует осторожности с секретами — но оригинал делает именно так
- Тестирование: `eprintln!("[gateway] ...")` логи для отладки env/port/health
