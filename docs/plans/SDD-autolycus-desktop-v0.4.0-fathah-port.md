# SDD: Autolycus Desktop v0.4.0 — Hermes Agent Port

> **Статус:** черновик
> **Дата:** 2026-06-09
> **Автор:** OWL
> **Источник:** анализ fathah/hermes-desktop (v0.5.8) + NikolayGusev-astra/autolycus-desktop (v0.3.0)

---

## 1. Контекст

### Проблема

autolycus-desktop v0.3.0 — минимальный Tauri-обёрт вокруг `tui_gateway.entry`. Работает только локально, только с одним профилем, без session management, без remote/SSH, без gateway lifecycle management. Чат использует примитивный JSON-RPC stdin/stdout, который не соответствует реальному протоколу hermes gateway.

fathah/hermes-desktop v0.5.8 — полноценный Electron-клиент (22K+ строк TS в main процессе), реализующий:
- 3 режима подключения (local / remote / SSH tunnel)
- Gateway lifecycle management (start/stop/restart/health polling)
- Per-profile изоляция (env, config, skills, gateway, sessions)
- SSE-стриминг с reasoning, tool events, usage
- Session management через SQLite (better-sqlite3)
- Skills, cron jobs, kanban, MCP servers management
- Auto-update через electron-updater

### Цель

Портировать ключевую функциональность fathah/hermes-desktop в нашу Tauri-архитектуру, сохранив:
- Tauri 2 как платформу (не переходим на Electron)
- React + TypeScript frontend
- Rust backend для native operations

---

## 2. Архитектура

### Текущая (v0.3.0)

```
┌─────────────────────────────────────────────┐
│  React Frontend (TSX)                       │
│  App.tsx → ChatView → AgentClient           │
│  stores: gatewayStore, uiStore              │
├─────────────────────────────────────────────┤
│  Tauri IPC (invoke / listen)                │
├─────────────────────────────────────────────┤
│  Rust Backend (lib.rs, 682 строки)          │
│  AgentState { child, stdin_tx, config }     │
│  Commands: start_agent, stop_agent,         │
│            send_rpc, detect_instances        │
│  Protocol: JSON-RPC stdin/stdout            │
└─────────────────────────────────────────────┘
```

### Целевая (v0.4.0)

```
┌─────────────────────────────────────────────┐
│  React Frontend (TSX)                       │
│  + ConnectionScreen (local/remote/ssh)      │
│  + GatewayStatusBar (health, model, tokens) │
│  + SessionList (from SQLite)                │
│  + ProfileSwitcher                          │
│  stores: gatewayStore, sessionStore,        │
│          profileStore, connectionStore      │
├─────────────────────────────────────────────┤
│  Tauri IPC (invoke / listen)                │
├─────────────────────────────────────────────┤
│  Rust Backend (~2500 строк)                 │
│  ├── gateway    — start/stop/restart/health │
│  ├── connection — local/remote/ssh modes    │
│  ├── ssh        — tunnel, remote exec       │
│  ├── config     — .env, config.yaml I/O     │
│  ├── sessions   — SQLite (state.db) access  │
│  ├── models     — models.json CRUD          │
│  ├── skills     — list/install/uninstall    │
│  ├── profiles   — per-profile management    │
│  ├── installer  — HERMES_HOME, install      │
│  └── chat       — SSE stream, API fallback  │
└─────────────────────────────────────────────┘
```

---

## 3. Модули для портирования

### Приоритет 1 (критично — без этого не работает)

| Модуль fathah | Наш аналог | Строки | Что делает |
|---|---|---|---|
| `hermes.ts` (gateway part) | `gateway.rs` | ~800 | Start/stop/restart gateway, health polling, per-profile ports |
| `hermes.ts` (chat part) | `chat.rs` | ~600 | sendMessage with SSE streaming, API fallback, session management |
| `ssh-tunnel.ts` | `ssh.rs` | ~300 | SSH tunnel, health check, port forwarding |
| `config.ts` | `config.rs` | ~200 | .env, config.yaml, connection config read/write |
| `installer.ts` (core) | `installer.rs` | ~200 | HERMES_HOME resolution, path validation |

### Приоритет 2 (важно — базовая функциональность)

| Модуль fathah | Наш аналог | Строки | Что делает |
|---|---|---|---|
| `sessions.ts` | `sessions.rs` | ~300 | SQLite state.db access, session list/search/messages |
| `models.ts` | `models.rs` | ~150 | models.json CRUD, custom providers from config.yaml |
| `profiles.ts` | `profiles.rs` | ~150 | Per-profile CRUD, active profile |
| `ssh-remote.ts` | `ssh.rs` (extend) | ~200 | Remote execution via SSH for all operations |

### Приоритет 3 (желательно — полнота)

| Модуль fathah | Наш аналог | Строки | Что делает |
|---|---|---|---|
| `skills.ts` | `skills.rs` | ~200 | Skills list/install/uninstall |
| `memory.ts` | `memory.rs` | ~100 | memory.md, user.md read/write |
| `cronjobs.ts` | `cronjobs.rs` | ~150 | Cron jobs management |
| `mcp-servers.ts` | `mcp.rs` | ~200 | MCP servers management |
| `kanban.ts` | `kanban.rs` | ~200 | Kanban boards/tasks |

### Не портируем (не наш слой)

| Модуль fathah | Причина |
|---|---|
| `claw3d.ts` | 3D office — не наша функциональность |
| `registry.ts` | Marketplace — зависимость от fathah GitHub |
| `office-start.ts` | Office stack — не наша функциональность |
| `updater-log` | Заменяем на tauri-updater |
| `security.ts` | Electron-specific — у нас Tauri sandbox |
| `locale.ts` | i18n — не критично |

---

## 4. Протокол общения

### Текущий (v0.3.0) — сломанный

```
Frontend → invoke("send_rpc", {method, params}) → Rust → stdin → Python
Python → stdout → Rust → emit("agent_event") → Frontend
```

Проблема: `tui_gateway.entry` не понимает JSON-RPC. Он работает через stdin команды (prompt/response), не через RPC.

### Целевой (v0.4.0) — 3 режима

**Режим 1: TUI Gateway (local)**
```
Frontend → invoke("send_message", {text, session_id}) → Rust → stdin → gateway
gateway → stdout (SSE) → Rust → emit("chat_event") → Frontend
```

**Режим 2: API (remote)**
```
Frontend → invoke("send_message", {text, session_id}) → Rust → HTTP POST /v1/chat/completions
Response (SSE) → Rust → emit("chat_event") → Frontend
```

**Режим 3: SSH (tunnel + remote)**
```
Frontend → invoke("send_message", {text, session_id}) → Rust → SSH tunnel → remote API
Remote response → tunnel → Rust → emit("chat_event") → Frontend
```

### События (единый формат для всех режимов)

```rust
#[derive(Serialize, Clone)]
#[serde(tag = "type")]
enum ChatEvent {
    Token { content: String },
    Reasoning { content: String },
    ToolStart { name: String, tool_call_id: String },
    ToolComplete { name: String, tool_call_id: String, output: String, duration_ms: u64 },
    ApprovalRequest { request_id: String, tool_name: String, tool_input: String, action: String, command_class: String },
    PipelineStatus { backend: String, model: Option<String>, tokens_used: Option<u64>, tokens_limit: Option<u64>, cost_usd: Option<f64> },
    Done { session_id: Option<String> },
    Error { message: String },
}
```

---

## 5. Connection Modes

### Local mode

- Запуск hermes gateway как child process
- Per-profile port allocation (8642 + profile index)
- Health polling каждые 15 сек
- Auto-restart при падении

### Remote mode

- HTTP/HTTPS к удалённому hermes API
- URL + API key из connection config
- Поддержка `/v1/chat/completions` (OpenAI-compatible)
- Поддержка `/v1/runs` (Hermes native, если доступен)

### SSH mode

- SSH tunnel: `ssh -N -L {local_port}:127.0.0.1:{remote_port} user@host`
- Remote execution через SSH для file operations
- Все операции проксируются через SSH

### Connection config (desktop.json)

```json
{
  "connectionMode": "local",
  "remoteUrl": "https://hermes.example.com:8443",
  "remoteApiKey": "sk-...",
  "ssh": {
    "host": "147.90.10.50",
    "port": 22,
    "username": "root",
    "keyPath": "~/.ssh/id_rsa",
    "remotePort": 8642,
    "localPort": 18642
  }
}
```

---

## 6. Session Management

### SQLite state.db

fathah использует `better-sqlite3` для прямого доступа к `~/.hermes/state.db`. Мы используем `rusqlite`.

```rust
// sessions.rs
pub fn list_sessions(db_path: &Path, limit: i64, offset: i64) -> Result<Vec<Session>>
pub fn get_session_messages(db_path: &Path, session_id: &str) -> Result<Vec<SessionMessage>>
pub fn search_sessions(db_path: &Path, query: &str, limit: i64) -> Result<Vec<SearchResult>>
pub fn delete_session(db_path: &Path, session_id: &str) -> Result<()>
```

### Session cache

Локальный кэш сгенерированных заголовков (как у fathah):
- `session-cache.ts` → `session_cache.rs`
- Автоматическая генерация title из первого сообщения
- FTS5 поиск по содержимому

---

## 7. Per-Profile Isolation

### Структура директорий

```
~/.hermes/
├── .env                    # default profile env
├── config.yaml             # default profile config
├── models.json             # global models
├── state.db                # default profile sessions
├── skills/                 # default profile skills
├── memory.md               # default profile memory
├── user.md                 # default profile user
├── profiles/
│   ├── work/
│   │   ├── .env
│   │   ├── config.yaml
│   │   ├── state.db
│   │   └── skills/
│   └── personal/
│       ├── .env
│       ├── config.yaml
│       ├── state.db
│       └── skills/
```

### Port allocation

- default profile: 8642
- named profiles: 8642 + hash(profile_name) % 1000
- Каждый profile имеет свой gateway process

---

## 8. Frontend Changes

### Новые компоненты

| Компонент | Описание |
|---|---|
| `ConnectionScreen` | Выбор режима (local/remote/ssh), настройка параметров |
| `GatewayStatusBar` | Статус gateway, модель, токены, cost |
| `SessionList` | Список сессий из SQLite с поиском |
| `ProfileSwitcher` | Переключение между профилями |
| `RemoteConfigForm` | Форма настройки remote URL + API key |
| `SshConfigForm` | Форма настройки SSH подключения |

### Изменения существующих

| Компонент | Что меняется |
|---|---|
| `App.tsx` | Добавить ConnectionScreen, ProfileSwitcher, GatewayStatusBar |
| `ChatView.tsx` | Использовать новый chat protocol, показывать reasoning |
| `Header.tsx` | Интеграция с GatewayStatusBar |
| `SettingsPanel.tsx` | Добавить вкладку Connection |
| `gatewayStore.ts` | Добавить connection state, profile state, session state |

### Новые stores

```typescript
interface ConnectionStore {
  mode: "local" | "remote" | "ssh";
  remoteUrl: string;
  sshConfig: SshConfig;
  setMode: (mode: ConnectionMode) => void;
  setRemoteConfig: (url: string, apiKey: string) => void;
  setSshConfig: (config: SshConfig) => void;
  testConnection: () => Promise<boolean>;
}

interface ProfileStore {
  profiles: Profile[];
  activeProfile: string;
  listProfiles: () => Promise<void>;
  switchProfile: (name: string) => Promise<void>;
  createProfile: (name: string, clone: boolean) => Promise<void>;
  deleteProfile: (name: string) => Promise<void>;
}

interface SessionStore {
  sessions: Session[];
  currentSession: string | null;
  listSessions: (limit?: number, offset?: number) => Promise<void>;
  loadSession: (id: string) => Promise<Message[]>;
  searchSessions: (query: string) => Promise<Session[]>;
  deleteSession: (id: string) => Promise<void>;
}
```

---

## 9. Rust Dependencies

```toml
[dependencies]
# Существующие
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "5"
libc = "0.2"

# Новые
rusqlite = { version = "0.31", features = ["bundled"] }  # SQLite для sessions
tokio = { version = "1", features = ["full"] }            # Async runtime
reqwest = { version = "0.12", features = ["stream"] }     # HTTP client для remote mode
futures = "0.3"                                           # Stream processing
ssh2 = "0.9"                                              # SSH tunnel + remote exec
yaml-rust2 = "0.8"                                        # config.yaml parsing
tauri-plugin-updater = "2"                                # Auto-update
```

---

## 10. Миграция с v0.3.0

### Breaking changes

1. `AgentConfig` — новое поле `connection_mode`
2. `start_agent` → `connect` (унифицированный метод для всех режимов)
3. `send_rpc` → `send_message` (другой протокол)
4. `agent_event` → `chat_event` (единый формат событий)

### Совместимость

- Старый `start_agent` оставляем как deprecated wrapper
- Автоматическая миграция desktop.json при первом запуске

---

## 11. Оценка

| Компонент | Строк Rust | Строк TS/TSX | Итого |
|---|---|---|---|
| gateway.rs | 800 | — | 800 |
| chat.rs | 600 | — | 600 |
| ssh.rs | 500 | — | 500 |
| config.rs | 200 | — | 200 |
| installer.rs | 200 | — | 200 |
| sessions.rs | 300 | — | 300 |
| models.rs | 150 | — | 150 |
| profiles.rs | 150 | — | 150 |
| skills.rs | 200 | — | 200 |
| memory.rs | 100 | — | 100 |
| Frontend (new components) | — | 800 | 800 |
| Frontend (changes) | — | 400 | 400 |
| Stores (new) | — | 300 | 300 |
| **Итого** | **3200** | **1500** | **4700** |

---

## 12. Риски

| Риск | Вероятность | Импакт | Митигация |
|---|---|---|---|
| tui_gateway протокол несовместим | Высокая | Критичный | Сначала пробуем в dev mode, fallback на API |
| rusqlite FTS5 недоступен | Средняя | Средний | Используем bundled feature |
| SSH на Windows не работает | Средняя | Средний | Conditional compilation, OpenSSH for Windows |
| Rust compile time рост | Высокая | Низкий | Используем cargo build cache |
| API несовместимость с разными hermes версиями | Средняя | Высокий | Capability probing перед использованием |
