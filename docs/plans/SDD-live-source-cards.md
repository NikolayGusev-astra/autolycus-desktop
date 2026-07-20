# SDD: Live Source Cards (ADR-007)

> **Связанный ADR:** ADR-007-live-source-cards.md
> **Дата:** 2026-07-18

## 1. Обзор

Live Source Cards — живые данные из MCP серверов напрямую в Ленте (без агента).
v1: Email. v2: Jira, Calendar, Confluence.

## 2. Компоненты

### `mcp_client.rs` — generic stdio MCP клиент
- `McpStdioClient::spawn(command, args, env)` — `tokio::process::Command` + `Stdio::piped()`
- `initialize()` — MCP handshake (`protocolVersion: 2024-11-05`)
- `call_tool(name, args)` → `result` from JSON-RPC response
- `shutdown()` — kill child
- Framing: newline-delimited JSON (`read_line`, `write_all + \n`)
- `Drop` impl: `start_kill` как safety net

### `feed_sources.rs` — source fetchers
- `EmailMessage { id, subject, from, date }` — карточка без body
- `list_email_unread(hermes_home, profile)`:
  1. `read_mcp_servers_yaml` → `mcp_servers.email` (command, args, env)
  2. `McpStdioClient::spawn` + `initialize`
  3. `call_tool("list_inbox", {unread_only: true, days: 7, limit: 20})`
  4. `parse_email_list_response` — распаковать `content[0].text` JSON
  5. `shutdown`
- `parse_email_list_response` / `parse_email_payload` — чистые функции (тестируемые)

### `FeedView.tsx` — Email tile
- `interface EmailMessage` (Rust↔TS контракт)
- `load()` → `invoke<EmailMessage[]>("list_email_unread_cmd")` (в `Promise.all`)
- Tile: иконка `Mail`, карточки (subject + from), count badge, empty/error/loading states
- Hybrid refresh: `setInterval(load, 5 min)` + on mount

## 3. Data Flow

```
FeedView mount / 5-min poll
  → invoke("list_email_unread_cmd", { profile: null })
    → feed_sources::list_email_unread
      → mcp::read_mcp_servers_yaml (config.yaml mcp_servers.email)
      → McpStdioClient::spawn(python, server.py, EMAIL_* env)
        → child process
      → client.initialize()  [JSON-RPC: initialize]
      → client.call_tool("list_inbox", {unread_only:true, days:7})
          [JSON-RPC: tools/call]
        → email MCP: IMAP fetch UNSEEN
          ← {messages: [{id,subject,from,date,body}], total}
      → parse_email_list_response → Vec<EmailMessage>
      → client.shutdown()
    ← Vec<EmailMessage>
  ← setEmailItems(msgs)
  → render Email tile
```

## 4. Расширяемость (v2)

Добавление нового источника (например Jira):
1. `feed_sources.rs`: `struct JiraTask`, `list_jira_overdue(hermes_home, profile)` — тот же паттерн (read config → spawn MCP → call_tool → parse)
2. `lib.rs`: `list_jira_overdue_cmd` Tauri команда
3. `FeedView.tsx`: новый tile (copy-paste Email tile, поменять иконку/поля)

MCP клиент (`mcp_client.rs`) — переиспользуемый, не меняется.

## 5. Тестовое покрытие

| Тест | Что проверяет |
|---|---|
| `mcp_initialize_request_is_valid_jsonrpc` | handshake frame |
| `mcp_tools_call_request_is_valid_jsonrpc` | tools/call frame |
| `mcp_call_tool_serializes_arguments` | args → params.arguments |
| `e2e_mcp_client_initialize_and_call_tool` | полный lifecycle с mock Python MCP |
| `parse_email_list_response_extracts_messages` | парсинг MCP ответа |
| `parse_email_response_handles_empty` | пустой inbox |
| `parse_email_response_errors_on_missing_content` | malformed → error |
| `parse_email_defaults_missing_fields` | "(no subject)" fallback |
| `list_email_unread_returns_error_if_not_configured` | нет config → error |

**Итого: 117 тестов (107 регрессия + 10 новых), все зелёные.**
