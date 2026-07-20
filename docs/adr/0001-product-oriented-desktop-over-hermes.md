# ADR-001: Product-oriented Desktop поверх Hermes Agent Runtime

* **Статус:** Proposed
* **Дата:** 2026-07-20
* **Владельцы:** Autolycus Desktop maintainers
* **Область:** продуктовая архитектура, UX, интеграция с Hermes Agent, MCP, управление действиями и миграция существующего приложения
* **Связанные решения:** последующие ADR по протоколу, permission model, интеграциям, локальному runtime и хранению данных

## 1. Контекст

Autolycus Desktop создаётся как готовый рабочий ассистент для руководителя, менеджера проекта и инженера. Целевой пользователь не должен:

* устанавливать и настраивать AI-runtime вручную;
* выбирать модель и reasoning effort для каждого запроса;
* понимать MCP, gateway, stdio, WebSocket, SSH или YAML;
* самостоятельно создавать интеграции;
* поддерживать вторую Jira, второй календарь или второй список задач;
* исправлять конфигурацию после обновления Hermes;
* быть AI-энтузиастом.

Текущая реализация исторически является полным портом Hermes Desktop с Electron на Tauri и сохраняет многие административные поверхности: Local/Remote/SSH, Gateway Control, Tools Manager, диагностику, credential pool и настройку runtime. Это полезно для разработчика и администратора, но не соответствует целевому пользовательскому сценарию «установил, вошёл, подключил рабочие системы, начал работать».

При этом Hermes Agent уже решает сложные инфраструктурные задачи:

* управление AI-agent lifecycle;
* сессии и история;
* model runtime;
* tool execution;
* MCP discovery;
* approvals;
* memory;
* skills;
* cron и messaging gateway;
* локальные и удалённые execution backends.

Поэтому Autolycus не должен форкать или повторно реализовывать Hermes. Он должен стать продуктовым слоем над Hermes.

## 2. Проблема

Сейчас граница между продуктом и runtime размыта:

```text
React UI
  ├── пользовательские сценарии
  ├── Hermes-команды
  ├── модели и reasoning
  ├── MCP
  ├── gateway
  ├── connection mode
  └── диагностика runtime
          │
          ▼
Tauri commands / Rust
  ├── product logic
  ├── process control
  ├── config.yaml editing
  ├── WebSocket protocol
  ├── SSH
  ├── MCP management
  └── local data
          │
          ▼
Hermes Agent
```

Следствия:

1. Пользователь видит внутреннюю архитектуру.
2. Frontend зависит от имён Hermes RPC и форматов событий.
3. Обновление Hermes может ломать React-компоненты.
4. Бизнес-действия маскируются под chat/tool events.
5. Настройки пользователя, администратора и разработчика смешаны.
6. В Autolycus повторно реализуются функции, которые уже принадлежат Hermes.
7. Невозможно независимо тестировать продуктовые сценарии и Hermes wire protocol.
8. Масштабный UX-рефакторинг становится одновременно runtime-рефакторингом.

## 3. Цели решения

Принятое решение должно обеспечить:

* готовый продукт для не технического пользователя;
* сохранение Hermes Agent как основного agent runtime;
* отсутствие форка Hermes;
* постепенную миграцию без big-bang rewrite;
* совместимость с Local, Managed Remote и Enterprise deployment;
* устойчивость к изменениям Hermes;
* типизированный контракт между Autolycus и Hermes;
* единый lifecycle сессий, процессов и соединений;
* прозрачные подтверждения опасных действий;
* разделение пользовательского и административного интерфейса;
* сохранение внешних систем как источников истины;
* возможность создавать role packs без ветвления всего UI.

## 4. Не цели

В рамках этого ADR не принимаются решения:

* о конкретном коммерческом AI-провайдере;
* о замене Tauri, React или Rust;
* о создании собственного agent runtime;
* о создании собственного MCP-протокола;
* о полной замене Jira, Linear, Outlook, Slack или GitHub;
* о поддержке каждого commit ветки `main` Hermes Agent;
* о redesign каждого экрана;
* о реализации всех role packs одновременно;
* о немедленном удалении Developer Edition.

## 5. Зафиксированные контракты Hermes

### 5.1 Выбор протокола

Hermes предоставляет три программных интерфейса:

| Интерфейс             | Транспорт                          | Назначение                                                          |
| --------------------- | ---------------------------------- | ------------------------------------------------------------------- |
| ACP                   | JSON-RPC через stdio               | IDE и ACP-клиенты                                                   |
| TUI Gateway           | JSON-RPC через stdio или WebSocket | Полное управление сессиями, streaming, approvals, tools и командами |
| OpenAI-compatible API | HTTP и SSE                         | Стандартные chat-клиенты и внешние HTTP consumers                   |

Для интерактивного desktop-приложения выбирается **TUI Gateway JSON-RPC over persistent WebSocket**, потому что он предоставляет полный session lifecycle, streaming events, tool progress, approvals, clarification и interrupt. OpenAI-compatible API не является основным desktop-контрактом, а ACP предназначен прежде всего для IDE-клиентов.

Hermes определяет границу ответственности аналогично требуемой архитектуре: UI владеет отображением, а Python runtime — сессиями, инструментами, model calls и командами.

### 5.2 JSON-RPC envelope

Hermes отправляет события в форме:

```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "type": "message.delta",
    "session_id": "live-session-id",
    "payload": {}
  }
}
```

`type` определяет тип события, `session_id` является process-local идентификатором живой сессии, `payload` зависит от события.

После установки WebSocket-соединения Hermes отправляет `gateway.ready`.

### 5.3 Версия desktop-контракта

В текущей реализации Hermes объявляет:

```text
DESKTOP_BACKEND_CONTRACT = 4
```

Значение возвращается в `session.create.info.desktop_contract` и последующих `session.info`.

Autolycus не должен предполагать совместимость только по версии Hermes package. Основной compatibility key — `desktop_contract` плюс capability probes.

### 5.4 Два идентификатора сессии

`session.create` возвращает:

* `session_id` — process-local live handle;
* `stored_session_id` — durable session key;
* начальную историю;
* session info;
* `desktop_contract`.

Пустая сессия не записывается в базу до первого prompt, поэтому создание draft/probe-сессии не должно оставлять пустые разговоры.

`session.resume` принимает durable session ID и возвращает новый live `session_id`. Durable key остаётся стабильным, а live ID может меняться после reconnect или restart.

Следовательно:

```text
ConversationId = durable Hermes session key
RuntimeSessionId = process-local Hermes session_id
```

UI и продуктовый domain используют только `ConversationId`. `RuntimeSessionId` принадлежит инфраструктурному адаптеру.

### 5.5 Интерактивные методы

Минимальный поддерживаемый desktop subset:

```text
session.create
session.resume
session.list
session.history
session.status
session.title
session.interrupt
session.close

prompt.submit
session.steer

approval.respond
clarify.respond
sudo.respond
secret.respond

image.attach
image.attach_bytes
pdf.attach
file.attach

process.list
process.kill
reload.mcp
```

Hermes также предоставляет дополнительные методы, но их наличие не должно автоматически становиться частью публичного Autolycus API. Каталог методов и событий является более широким, чем продуктовый контракт Autolycus.

### 5.6 События

Минимально поддерживаемые события:

```text
gateway.ready

message.start
message.delta
message.complete

thinking.delta
reasoning.delta

tool.start
tool.progress
tool.complete

approval.request
clarify.request
sudo.request
sudo.expire
secret.request
secret.expire

session.info
session lifecycle events
error
```

Expiry events содержат исходный `request_id`; клиент должен удалить только соответствующий pending prompt.

### 5.7 Approval contract

Hermes отправляет `approval.request` и поддерживает choices:

```text
once
session
always
deny
```

Ответ выполняется отдельным RPC `approval.respond` с `session_id`, `choice` и при необходимости `all`. Approval не должен передаваться как JSON-текст через `prompt.submit`.

### 5.8 Attachments

Для файлов на машине gateway Hermes принимает path-based attachments. Для remote client он поддерживает byte/base64-варианты, включая изображения, PDF и произвольные файлы. Hermes самостоятельно staging-ит remote-файлы в workspace.

### 5.9 MCP

Hermes поддерживает:

* local stdio MCP;
* remote HTTP MCP;
* automatic discovery;
* per-server tool filtering;
* OAuth;
* curated MCP catalog.

MCP configuration является runtime-концепцией Hermes. Пользователь Autolycus не должен редактировать `mcp_servers`, `command`, `args`, `env` или transport.

`reload.mcp` может инвалидировать prompt cache и поэтому требует явного подтверждения. Интеграции не должны незаметно переключаться внутри активного разговора.

## 6. Решение

### 6.1 Зафиксировать границу продуктов

Принимается следующая ответственность:

```text
Autolycus
├── пользовательский опыт
├── onboarding
├── role packs
├── Today / briefings
├── Action Center
├── integration catalog
├── business-level permissions
├── external source projections
├── audit trail
├── notifications
└── deployment UX

Hermes
├── agent execution
├── models
├── reasoning
├── tool execution
├── MCP client
├── skills
├── memory
├── session persistence
├── agent cron
├── terminal backends
└── low-level approvals
```

Autolycus не реализует второй agent runtime, второй MCP client, вторую memory subsystem или второй cron engine.

### 6.2 Ввести Anti-Corruption Layer

Между приложением и Hermes создаётся обязательный слой:

```text
React Product UI
        │
        ▼
Autolycus Application API
        │
        ▼
Product Domain Services
        │
        ▼
AgentRuntime trait
        │
        ▼
Hermes Adapter
        │
        ▼
Hermes TUI Gateway JSON-RPC
```

Ни один React-компонент и ни один продуктовый service не должен знать строки:

```text
prompt.submit
approval.respond
message.delta
session.info
reload.mcp
```

Они разрешены только внутри `infrastructure/hermes`.

### 6.3 Ввести внутренний `AgentRuntime` contract

Продуктовый код зависит от интерфейса:

```rust
#[async_trait]
pub trait AgentRuntime {
    async fn capabilities(&self) -> Result<RuntimeCapabilities, RuntimeError>;

    async fn create_conversation(
        &self,
        request: CreateConversation,
    ) -> Result<ConversationHandle, RuntimeError>;

    async fn resume_conversation(
        &self,
        conversation_id: ConversationId,
    ) -> Result<ConversationHandle, RuntimeError>;

    async fn submit(
        &self,
        handle: &ConversationHandle,
        request: UserRequest,
    ) -> Result<RunHandle, RuntimeError>;

    async fn interrupt(
        &self,
        handle: &ConversationHandle,
    ) -> Result<(), RuntimeError>;

    async fn respond_to_approval(
        &self,
        response: ApprovalResponse,
    ) -> Result<(), RuntimeError>;

    async fn attach(
        &self,
        handle: &ConversationHandle,
        attachment: Attachment,
    ) -> Result<AttachmentRef, RuntimeError>;

    fn subscribe(&self) -> RuntimeEventStream;
}
```

`HermesAgentRuntime` является первой реализацией. Mock runtime используется для frontend, integration и acceptance tests.

### 6.4 Типизировать Hermes wire protocol

Создаётся отдельный infrastructure package/module:

```text
src-tauri/src/infrastructure/hermes/
├── client.rs
├── connection.rs
├── protocol/
│   ├── envelope.rs
│   ├── methods.rs
│   ├── events.rs
│   ├── errors.rs
│   └── contract.rs
├── translator.rs
├── session_registry.rs
├── reconnect.rs
└── fixtures/
```

Правила:

1. Все RPC request/response типизированы через Serde.
2. Неизвестные JSON-поля игнорируются.
3. Неизвестные события логируются и игнорируются.
4. Отсутствие обязательного поля создаёт typed protocol error.
5. Raw JSON не выходит за пределы `infrastructure/hermes`.
6. Request ID генерируется централизованно.
7. Каждый pending request имеет timeout.
8. Все pending requests завершаются ошибкой при disconnect.
9. Secrets и sudo responses никогда не логируются.
10. Ошибки Hermes переводятся в стабильный `AppError`.

### 6.5 Использовать одно persistent WebSocket-соединение

Local, Remote и SSH различаются только способом получения endpoint:

```rust
enum RuntimeEndpoint {
    LocalManaged,
    RemoteManaged { endpoint: Url },
    Enterprise { endpoint: Url },
    SshTunnel { local_endpoint: Url },
}
```

После разрешения endpoint весь последующий protocol flow одинаков.

Запрещается поддерживать отдельные chat transports для Local, Remote и SSH.

Lifecycle:

```text
Disconnected
   ↓
Connecting
   ↓
AwaitingGatewayReady
   ↓
Compatible
   ↓
Active
   ↓
Reconnecting
   ├── resume durable conversations
   └── reconcile pending runs
```

Reconnect использует exponential backoff с jitter.

После reconnect:

1. создаётся новое WebSocket-соединение;
2. ожидается `gateway.ready`;
3. проверяется compatibility;
4. активные разговоры восстанавливаются через durable ID;
5. обновляются live session IDs;
6. запрашиваются `session.status` и при необходимости `session.history`;
7. локальный UI reconciliation выполняется по durable conversation ID.

### 6.6 Ввести compatibility handshake

Autolycus поддерживает явный диапазон contract versions:

```rust
const MIN_HERMES_DESKTOP_CONTRACT: u32 = 4;
const MAX_HERMES_DESKTOP_CONTRACT: u32 = 4;
```

В дальнейшем диапазон расширяется только после contract tests.

Handshake:

1. дождаться `gateway.ready`;
2. создать lightweight session с:

   * `source = "desktop"`;
   * `close_on_disconnect = true`;
3. прочитать `info.desktop_contract`;
4. построить `RuntimeCapabilities`;
5. закрыть probe session;
6. разрешить запуск product services.

При несовместимости:

* приложение не пытается продолжать работу «на удачу»;
* пользователь получает понятное сообщение;
* Developer/Admin UI показывает версии и diagnostic details;
* Managed Local предлагает совместимое обновление;
* Remote/Enterprise предлагает обратиться к администратору.

### 6.7 Добавить capability model

Одного номера контракта недостаточно для долгосрочной совместимости.

Внутренняя модель:

```rust
struct RuntimeCapabilities {
    contract_version: u32,

    sessions: SessionCapabilities,
    streaming: StreamingCapabilities,
    approvals: ApprovalCapabilities,
    attachments: AttachmentCapabilities,
    integrations: IntegrationCapabilities,
    background_runs: BackgroundRunCapabilities,
}
```

До появления официального Hermes capability RPC возможности определяются по:

* contract version;
* известной compatibility matrix;
* безопасным read-only probes;
* наличию ожидаемых response fields.

Следует предложить upstream изменение Hermes:

```text
runtime.capabilities
```

или расширение `gateway.ready`:

```json
{
  "desktop_contract": 5,
  "methods": [],
  "events": [],
  "features": {}
}
```

Autolycus не должен поддерживать capability negotiation через разбор строк ошибок.

### 6.8 Разделить runtime session и product conversation

В product domain:

```rust
struct Conversation {
    id: ConversationId,
    title: String,
    role_pack_id: RolePackId,
    external_context: Vec<EvidenceRef>,
    state: ConversationState,
}
```

В infrastructure:

```rust
struct HermesSessionBinding {
    conversation_id: ConversationId,
    live_session_id: HermesLiveSessionId,
    durable_session_id: HermesStoredSessionId,
    contract_version: u32,
}
```

Frontend не хранит Hermes live ID как основную идентичность разговора.

### 6.9 Ввести product event model

Hermes events переводятся в стабильные события Autolycus:

| Hermes event       | Autolycus event              |
| ------------------ | ---------------------------- |
| `message.start`    | `AssistantRunStarted`        |
| `message.delta`    | `AssistantTextDelta`         |
| `message.complete` | `AssistantRunCompleted`      |
| `thinking.delta`   | `InternalProgressUpdated`    |
| `tool.start`       | `RuntimeActivityStarted`     |
| `tool.progress`    | `RuntimeActivityUpdated`     |
| `tool.complete`    | `RuntimeActivityCompleted`   |
| `approval.request` | `ActionApprovalRequested`    |
| `clarify.request`  | `UserInputRequested`         |
| `sudo.request`     | `AdminCredentialRequested`   |
| `secret.request`   | `SecretRequested`            |
| `session.info`     | `ConversationRuntimeUpdated` |
| `error`            | `AssistantRunFailed`         |

UI подписывается только на Autolycus events.

### 6.10 Создать Action Center

Chat transcript перестаёт быть единственным местом, где отображается работа агента.

Создаётся доменная сущность:

```rust
struct ActionProposal {
    id: ActionId,
    conversation_id: ConversationId,
    run_id: RunId,

    capability: CapabilityId,
    title: String,
    explanation: String,
    effect_summary: String,

    risk: RiskLevel,
    reversible: bool,
    preview: Option<ActionPreview>,

    evidence: Vec<EvidenceRef>,
    state: ActionState,
    created_at: DateTime,
}
```

Состояния:

```text
Proposed
AwaitingApproval
Approved
Running
Succeeded
Failed
Denied
Expired
Cancelled
```

Action Center показывает:

* что агент собирается сделать;
* где;
* от имени кого;
* какие данные изменятся;
* можно ли отменить;
* на основании каких источников принято решение;
* итог выполнения;
* причину ошибки.

Tool event не всегда является бизнес-действием. Read-only search, memory lookup и внутреннее reasoning отображаются как Activity. Внешние write/destructive operations становятся Action.

### 6.11 Ограничить approval choices

В пользовательском Manager Mode разрешаются:

```text
Разрешить один раз  → once
Разрешить до конца текущей работы → session
Отклонить → deny
```

`always` не показывается обычному пользователю, пока Hermes не предоставляет достаточно точный structured scope.

Нельзя обещать пользователю:

> Всегда разрешать создавать черновики Outlook

если runtime фактически понимает только более широкое:

> always allow this tool/approval category

Постоянные разрешения управляются через отдельную product policy и соответствующую конфигурацию Hermes.

### 6.12 Использовать defense in depth для действий

Разрешение действия контролируется на трёх уровнях:

1. **Connector scope**
   OAuth выдаёт минимально необходимые read/write scopes.

2. **Hermes runtime policy**
   Опасный tool требует approval и не работает в YOLO-режиме.

3. **Autolycus product policy**
   Пользователь видит business preview и подтверждает действие.

В Manager Mode Autolycus никогда автоматически не включает Hermes YOLO mode.

### 6.13 Абстрагировать MCP как каталог интеграций

Пользователь видит:

```text
Microsoft 365
Google Workspace
Jira
Linear
GitHub
Slack
Teams
Confluence
```

Пользователь не видит:

```text
MCP
stdio
command
args
env
headers
SSE
HTTP MCP
reload.mcp
```

Создаётся `IntegrationManifest`:

```rust
struct IntegrationManifest {
    id: IntegrationId,
    display_name: String,
    description: String,
    icon: AssetRef,

    authentication: AuthenticationKind,
    required_scopes: Vec<PermissionScope>,

    capabilities: Vec<BusinessCapability>,
    risk_policy: IntegrationRiskPolicy,

    runtime_binding: RuntimeIntegrationBinding,
}
```

`RuntimeIntegrationBinding` может ссылаться на Hermes MCP catalog entry, native Hermes tool или product-owned connector.

Autolycus:

* не редактирует `config.yaml` строками;
* не записывает MCP command/args/env напрямую из React;
* не реализует второй MCP discovery;
* устанавливает интеграцию через поддерживаемую Hermes operation;
* выполняет reload только вне активного run либо после понятного подтверждения;
* проверяет доступные capabilities после подключения.

При отсутствии стабильного Hermes management RPC допускается временный adapter к официальной Hermes CLI-команде. Прямое ручное YAML patching запрещается.

### 6.14 Не хранить секреты в product domain

Правила:

* React никогда не получает сохранённый secret обратно.
* Secret передаётся через отдельный secure Tauri command.
* Secret не попадает в Zustand, logs, telemetry или action history.
* Autolycus использует OS keyring для product credentials.
* Hermes credentials устанавливаются через поддерживаемый Hermes auth/config interface.
* Если Hermes требует `.env`, запись выполняется только внутри Hermes adapter с проверкой file permissions.
* В product database хранятся только credential references и connection status.

### 6.15 Ввести Role Packs

Role pack — продуктовая конфигурация, а не новая реализация агента.

```rust
struct RolePack {
    id: RolePackId,
    title: String,
    description: String,

    starter_workflows: Vec<WorkflowTemplate>,
    required_integrations: Vec<IntegrationRequirement>,
    optional_integrations: Vec<IntegrationRequirement>,

    briefing_templates: Vec<BriefingTemplate>,
    starter_prompts: Vec<PromptTemplate>,
    default_permissions: PermissionPolicy,
    notification_defaults: NotificationPolicy,
}
```

Начальные пакеты:

```text
Executive
Project Manager
Engineering Manager
Individual Contributor
```

Role pack не должен напрямую изменять Hermes global config при каждом переключении.

Исполнение role pack состоит из:

* product prompt templates;
* selected workflow;
* integration capability requirements;
* permission policy;
* optional mapping на заранее подготовленный Hermes profile;
* optional Hermes skill bundle.

Hermes profile является execution/security boundary. Role pack является UX/workflow boundary. Эти понятия не должны быть автоматически равны.

### 6.16 Сохранить внешние системы источниками истины

Autolycus не становится второй Jira или вторым Outlook.

Локальная база хранит:

* projections;
* normalized references;
* пользовательские preferences;
* briefing results;
* draft actions;
* action ledger;
* cached metadata;
* mappings между объектами.

Пример:

```rust
struct WorkItemRef {
    integration_id: IntegrationId,
    external_id: String,
    external_url: Option<String>,
    display_key: String,
    cached_title: String,
    cached_status: String,
    observed_at: DateTime,
}
```

Изменение Jira-задачи всегда выполняется через Jira capability. Autolycus не создаёт параллельную authoritative копию.

Локальные задачи допускаются только как явно обозначенный тип:

```text
Personal
Draft proposed by assistant
External Jira
External Linear
External GitHub
```

### 6.17 Создать evidence contract

Каждый значимый insight должен иметь источники:

```rust
struct EvidenceRef {
    integration_id: IntegrationId,
    object_type: String,
    object_id: String,
    label: String,
    deep_link: Option<String>,
    observed_at: DateTime,
}
```

Карточка «Проект находится под риском» без evidence считается неполной.

Evidence используется в:

* Today;
* meeting briefing;
* project risk;
* prepared replies;
* action proposals;
* executive summary.

### 6.18 Разделить интерфейсы User и Admin

#### User Surface

Основная навигация:

```text
Сегодня
Ассистент
Действия
Работа
```

Пользовательские настройки:

```text
Аккаунт
Подключённые приложения
Уведомления
Приватность и разрешения
Язык и внешний вид
```

#### Admin / Developer Surface

```text
Runtime
Providers
Models
Gateway
MCP
Skills
Credentials
Profiles
SSH
Terminal
Logs
Diagnostics
```

В обычной установке Admin Surface скрыт.

Для self-hosted/Developer Edition допускается переход в Hermes Dashboard вместо повторной реализации всех administrative screens. Hermes Dashboard уже управляет configuration, API keys, MCP, gateway, memory, sessions, logs, cron и skills.

### 6.19 Изменить onboarding

Новый onboarding:

```text
1. Войти
2. Выбрать роль
3. Выбрать желаемые результаты
4. Подключить рабочие системы
5. Настроить разрешения
6. Получить первый briefing
```

В обычном onboarding отсутствуют:

```text
Local / Remote / SSH
Gateway URL
Session token
Provider
Model
Reasoning effort
MCP
Hermes installation path
```

Deployment mode определяется:

* аккаунтом;
* organization policy;
* managed service discovery;
* установочным профилем.

Технические вопросы показываются только в Developer Edition или при диагностике.

### 6.20 Не создавать второй scheduler

Регулярные agent jobs, которые должны работать при закрытом desktop-приложении, принадлежат Hermes gateway/cron, а не Tauri interval timers. Hermes messaging gateway является фоновым процессом, который управляет сессиями и cron jobs.

Autolycus отвечает за:

* настройку понятного расписания;
* показ результата;
* notifications;
* action approvals;
* управление enable/disable.

До появления стабильного structured cron API автоматизации могут запускаться только в Developer Edition либо через ограниченный adapter.

## 7. Целевая структура кода

### 7.1 Rust/Tauri

```text
src-tauri/src/
├── app/
│   ├── bootstrap.rs
│   ├── state.rs
│   └── errors.rs
│
├── domain/
│   ├── actions/
│   ├── assistant/
│   ├── briefings/
│   ├── conversations/
│   ├── evidence/
│   ├── integrations/
│   ├── permissions/
│   ├── role_packs/
│   └── work_items/
│
├── application/
│   ├── action_service.rs
│   ├── assistant_service.rs
│   ├── briefing_service.rs
│   ├── integration_service.rs
│   ├── onboarding_service.rs
│   └── today_service.rs
│
├── infrastructure/
│   ├── hermes/
│   ├── persistence/
│   ├── keyring/
│   ├── notifications/
│   └── process/
│
├── commands/
│   ├── actions.rs
│   ├── assistant.rs
│   ├── integrations.rs
│   ├── onboarding.rs
│   ├── settings.rs
│   └── today.rs
│
└── lib.rs
```

`lib.rs` содержит только:

* Tauri builder;
* plugin registration;
* managed state;
* command registration;
* lifecycle hooks.

### 7.2 Frontend

```text
src/
├── app/
├── api/
├── domain/
├── features/
│   ├── actions/
│   ├── assistant/
│   ├── integrations/
│   ├── onboarding/
│   ├── settings/
│   ├── today/
│   └── work/
├── shared/
└── admin/
```

Frontend вызывает только product commands:

```text
assistant_create_conversation
assistant_submit
assistant_interrupt

actions_list
actions_approve
actions_deny

integrations_list
integrations_connect
integrations_disconnect

today_refresh
today_get

onboarding_get_state
onboarding_complete_step
```

Frontend не вызывает Hermes-named commands.

## 8. Процессная модель

Вводится единый `RuntimeSupervisor`:

```text
RuntimeSupervisor
├── HermesProcess
├── HermesWebSocket
├── SshTunnel optional
├── health state
├── restart policy
└── cancellation token
```

Состояния:

```text
NotInstalled
Stopped
Starting
Ready
Degraded
Reconnecting
Stopping
Failed
Incompatible
```

Все process operations используют:

* `tokio::process`;
* cancellation;
* bounded timeouts;
* controlled shutdown;
* stdout/stderr capture;
* readiness events;
* один authoritative state object.

Local, Remote и SSH не имеют разных session implementations.

## 9. Миграционная стратегия

Рефакторинг выполняется по Strangler Fig pattern. Старый интерфейс остаётся рабочим, пока новые слои поэтапно перехватывают функции.

### Phase 0 — Contract Baseline

Цель: зафиксировать существующее поведение до рефакторинга.

Работы:

* создать `docs/adr`;
* зафиксировать поддерживаемый Hermes contract;
* записать JSON fixtures всех используемых RPC и events;
* добавить end-to-end smoke с реальным Hermes;
* добавить compatibility manifest;
* ввести feature flags;
* запретить новые прямые Hermes invokes из React.

Результат:

```text
Поведение не изменилось.
Контракт стал тестируемым.
```

### Phase 1 — Hermes Adapter

Цель: спрятать wire protocol.

Работы:

* typed JSON-RPC client;
* persistent WS;
* session registry;
* durable/live ID separation;
* event translator;
* reconnect;
* typed error mapping;
* compatibility handshake.

Существующий Chat UI временно работает через новый adapter.

Критерий:

```text
В React отсутствует parsing Hermes events.
```

### Phase 2 — Runtime Supervisor

Цель: унифицировать lifecycle.

Работы:

* перевести process management на async;
* объединить local/remote/SSH после endpoint resolution;
* удалить connect-per-message transport;
* добавить health state;
* controlled shutdown;
* restart/reconnect policy.

Критерий:

```text
Для всех connection modes используется один AgentRuntime.
```

### Phase 3 — Product Domain

Цель: создать устойчивые продуктовые модели.

Работы:

* Conversation;
* Run;
* ActionProposal;
* EvidenceRef;
* Integration;
* RolePack;
* WorkItemRef;
* AppError;
* product Tauri commands.

Критерий:

```text
Product services не импортируют Hermes protocol DTO.
```

### Phase 4 — User/Admin Split

Цель: убрать инфраструктуру из пользовательского интерфейса.

Работы:

* новый Settings;
* hidden Admin Surface;
* перенос runtime panels;
* model/reasoning routing по умолчанию;
* удаление model picker из стандартного composer;
* Developer Mode flag.

Критерий:

```text
Manager Mode не содержит терминов MCP, gateway, provider, SSH и reasoning.
```

### Phase 5 — Integration Catalog

Цель: подключение приложений вместо MCP-настройки.

Работы:

* integration manifests;
* OAuth flows;
* capability verification;
* keyring references;
* Hermes MCP binding;
* удаление ручного YAML editing;
* integration health.

Критерий:

```text
Пользователь подключает Jira без редактирования command/args/env.
```

### Phase 6 — Today и Action Center

Цель: сделать приложение проактивным.

Работы:

* prioritized insights;
* evidence;
* meeting preparation;
* prepared drafts;
* action ledger;
* business previews;
* approval mapping;
* retry/cancel/expiry states.

Критерий:

```text
Пользователь видит результат и следующий шаг, а не список tool events.
```

### Phase 7 — Role-based Onboarding

Цель: time-to-value без технической настройки.

Работы:

* role selection;
* workflow selection;
* required integrations;
* permission presets;
* first briefing;
* managed runtime discovery.

Критерий:

```text
Новый пользователь получает первый полезный результат без открытия Admin Surface.
```

### Phase 8 — Automation

Цель: регулярная ценность.

Работы:

* morning briefing;
* meeting preparation;
* project risk monitoring;
* weekly summary;
* Hermes cron adapter;
* notification delivery;
* missed-run recovery.

Критерий:

```text
Автоматизации продолжают выполняться при закрытом UI, если deployment поддерживает background runtime.
```

### Phase 9 — Legacy Removal

Удаляются:

* direct Hermes invokes из React;
* второй WebSocket transport;
* chat-based approval tunneling;
* ручной YAML patching;
* пользовательские model/MCP/gateway panels;
* duplicate process lifecycle;
* старый onboarding;
* obsolete Tauri commands.

## 10. Feature flags

Для безопасной миграции:

```text
new_hermes_adapter
runtime_supervisor_v2
product_commands
manager_mode
integration_catalog_v2
action_center
today_v2
role_onboarding
legacy_admin_ui
```

Каждая фаза:

* включается отдельно;
* имеет rollback;
* не требует миграции всех пользователей одновременно;
* выпускается как работающий release.

## 11. Тестирование контрактов

### 11.1 Fixture tests

Для каждого Hermes contract сохраняются fixtures:

```text
fixtures/hermes/contract-4/
├── gateway-ready.json
├── session-create-response.json
├── session-resume-response.json
├── message-stream.jsonl
├── tool-lifecycle.jsonl
├── approval-flow.jsonl
├── clarify-flow.jsonl
├── attachment-flow.jsonl
├── disconnect-reconnect.jsonl
└── error-cases.jsonl
```

### 11.2 Contract tests

Проверяются:

* обязательные поля;
* tolerant parsing дополнительных полей;
* неизвестные события;
* error mapping;
* live/durable session mapping;
* event ordering;
* reconnect;
* pending request cleanup;
* expired approval;
* attachment limits;
* no-secret logging.

### 11.3 Real Hermes smoke

CI или scheduled workflow запускает совместимую закреплённую версию Hermes и проверяет:

```text
connect
gateway.ready
session.create
desktop_contract
prompt.submit
message.delta
message.complete
session.resume
session.interrupt
approval flow
session.close
```

### 11.4 Compatibility CI

Два уровня:

1. **Required** — закреплённая поддерживаемая Hermes version.
2. **Informational nightly** — актуальная Hermes `main`.

Падение nightly не ломает release Autolycus, но создаёт compatibility issue.

## 12. Observability

Каждая операция получает:

```text
correlation_id
conversation_id
run_id
action_id
runtime_request_id
```

Логи разделяются:

```text
product
runtime
protocol
integration
security
```

Запрещено логировать:

* API keys;
* OAuth tokens;
* passwords;
* sudo input;
* secret responses;
* полные приватные документы;
* полные email bodies без debug opt-in.

Метрики:

* runtime connection success;
* reconnect count;
* contract mismatch;
* session resume success;
* run completion;
* approval latency;
* action failure;
* integration health;
* first-value completion;
* briefing usefulness feedback.

## 13. Rollback

Каждая новая поверхность имеет legacy fallback до Phase 9.

Rollback не должен:

* менять Hermes config обратно;
* терять durable sessions;
* удалять action ledger;
* инвалидировать интеграции;
* требовать downgrade Hermes.

При несовместимости нового adapter приложение переключается только на предыдущий adapter, но не на частично работающий protocol guess.

## 14. Последствия

### Положительные

* Пользователь получает продукт, а не AI-консоль.
* Hermes можно обновлять через один compatibility layer.
* React перестаёт зависеть от wire protocol.
* Local, Remote и SSH используют один runtime path.
* MCP становится невидимой реализационной деталью.
* Снижается количество дублирующей логики.
* Появляется тестируемый action и approval lifecycle.
* Можно создавать role packs без копирования экранов.
* Внешние системы остаются источниками истины.
* Admin и Manager UX могут развиваться независимо.
* Появляется возможность заменить runtime без переписывания продукта.

### Отрицательные

* Потребуется временно поддерживать legacy и новую архитектуру.
* Возрастёт количество domain DTO.
* Необходимо поддерживать compatibility matrix Hermes.
* Некоторые желаемые функции потребуют upstream additions.
* Полный переход займёт несколько релизных циклов.
* Action classification потребует integration manifests.
* Некоторые административные возможности временно будут доступны только через Hermes Dashboard.

### Риски

#### Hermes protocol меняется быстрее Autolycus

Снижение риска:

* contract pinning;
* fixtures;
* nightly compatibility CI;
* capability negotiation;
* отсутствие зависимости от `main` в production.

#### Product policy расходится с Hermes approval policy

Снижение риска:

* defense in depth;
* запрет YOLO в Manager Mode;
* scoped connector permissions;
* отсутствие misleading `always`;
* security tests.

#### Role packs превращаются в набор hardcoded branches

Снижение риска:

* data-driven manifests;
* общий domain;
* workflow templates;
* отсутствие role-specific transport/runtime code.

#### Autolycus становится второй Jira

Снижение риска:

* external IDs;
* explicit source badges;
* projection-only persistence;
* все external writes через integration capability.

## 15. Отклонённые варианты

### 15.1 Продолжить развивать текущую архитектуру

Отклонено, потому что product UI останется связан с Hermes internals, а каждая новая роль увеличит количество специальных случаев.

### 15.2 Форкнуть Hermes Agent

Отклонено из-за стоимости синхронизации, security updates, MCP ecosystem и постоянного merge debt.

### 15.3 Использовать только OpenAI-compatible API

Отклонено как основной desktop protocol, поскольку продукту нужны structured approvals, clarification, session operations, tool lifecycle и fine-grained events.

API server может использоваться дополнительными consumers, но не заменяет TUI Gateway для основного desktop UX.

### 15.4 Использовать ACP

Отклонено для менеджерского desktop-приложения. ACP остаётся правильным вариантом для IDE-интеграций, но не является главным product contract Autolycus.

### 15.5 Встроить Hermes Dashboard как весь интерфейс

Отклонено, потому что Dashboard является административной поверхностью, а не role-oriented manager assistant.

Допускается использовать Dashboard как временную Admin Surface.

### 15.6 Переписать всё одним релизом

Отклонено из-за высокого риска регрессий в session lifecycle, remote connection, approvals, attachments и process management.

## 16. Необходимые upstream предложения Hermes

Для снижения стоимости интеграции Autolycus должен предложить в Hermes additive изменения:

1. `runtime.capabilities`.
2. Machine-readable OpenRPC или JSON Schema.
3. Contract version в `gateway.ready`.
4. Event sequence number и replay cursor.
5. Structured approval scope.
6. Structured action risk metadata.
7. Stable integration management RPC:

   * catalog;
   * install;
   * connect;
   * enable;
   * disable;
   * health;
   * uninstall.
8. Structured cron management RPC.
9. Structured source/evidence metadata.
10. Binary/chunked upload для больших remote attachments.

Эти изменения должны быть additive и обратно совместимыми. Autolycus не должен поддерживать приватный fork протокола.

## 17. Критерии принятия ADR

ADR считается реализованным, когда:

* React не вызывает raw Hermes RPC.
* Все Hermes события парсятся только в infrastructure layer.
* Используется один persistent WebSocket transport.
* Local, Remote и SSH сходятся в один `AgentRuntime`.
* Durable и live session IDs разделены.
* Contract mismatch определяется до начала пользовательского run.
* Approval отправляется через `approval.respond`.
* Manager Mode не показывает MCP, provider, gateway, SSH и reasoning.
* Интеграции подключаются через product catalog.
* Секреты не проходят через frontend state.
* Action Center имеет persistent lifecycle.
* Today показывает evidence-backed priorities.
* External work items сохраняют source-of-truth identity.
* Role onboarding приводит к первому briefing без Admin Surface.
* Существуют fixture и real-Hermes contract tests.
* Legacy transport и direct YAML editing удалены.

## 18. Следующие ADR

После принятия этого решения должны быть созданы:

```text
ADR-002 Hermes Protocol Compatibility and Versioning
ADR-003 Runtime Supervisor and Deployment Modes
ADR-004 Action, Approval and Permission Model
ADR-005 Integration Catalog and MCP Boundary
ADR-006 Product Data, Projections and Source-of-Truth Policy
ADR-007 Role Packs and Workflow Templates
ADR-008 Background Automations and Hermes Cron
ADR-009 Secrets and Credential Storage
ADR-010 Observability, Audit and Privacy
```

## 19. Итоговое решение

Autolycus становится **product-oriented assistant workspace**, а Hermes Agent остаётся **agent execution runtime**.

Hermes предоставляет:

```text
reasoning
sessions
tools
MCP
memory
skills
execution
```

Autolycus предоставляет:

```text
role
context
priorities
evidence
actions
permissions
workflow
usability
trust
```

Граница закрепляется через типизированный `AgentRuntime` и Hermes Anti-Corruption Layer. Миграция выполняется поэтапно, с contract tests, feature flags и сохранением рабочего legacy path до завершения каждой фазы.
