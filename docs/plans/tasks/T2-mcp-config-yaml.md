# Задача: T2 — MCP-серверы хранятся в servers.json вместо config.yaml

## Роль
Ты — senior Rust-инженер в Steersman Desktop. Мигрируешь хранилище MCP-серверов
на канонический upstream-формат. TDD где применимо.

## Контекст (читай перед работой)
- `docs/plans/ADR-002-hermes-api-contract.md` §«Config.yaml schema» —
  `mcp_servers:` блок — единственный источник правды для upstream.
- `SDD.md` §7.3 #11 — VALID.
- Тех-стек: Rust 2021, serde_yaml (уже в deps), yaml-rust2 0.8.

## Проблема
Steersman хранит MCP-серверы в `.hermes/mcp/servers.json` (`mcp.rs:75-80`),
кастомном JSON-формате. Hermes upstream читает ТОЛЬКО `config.yaml mcp_servers:`
блок. MCP-серверы, добавленные через десктоп, не видны Hermes Agent →
MCP-инструменты недоступны агенту.

## Root cause (из аудита)
`src-tauri/src/mcp.rs:75-80` `mcp_config_path()` → `.hermes/mcp/servers.json`.
`list_mcp_servers` (84), `add_mcp_server` (117), `remove_mcp_server` (174) —
все читают/пишут servers.json.
Контракт требует: `config.yaml mcp_servers:` блок как single source of truth.

## Definition of Done
1. Тест RED: `add_mcp_server` → `list_mcp_servers` читает из config.yaml
   (не servers.json).
2. Фикс GREEN: миграция `mcp_config_path` + list/add/remove на config.yaml
   через serde_yaml round-trip (parse → modify → serialize).
3. servers.json больше не создаётся/читается.
4. `cargo test --lib` — зелёные.

## Ограничения
- Не удаляй servers.json пользователя при миграции (если существует —
  one-time import в config.yaml с предупреждением).
- Сохраняй McpServer struct (фронтенд зависит от его формы).
- serde_yaml уже в зависимостях — новых crate не добавлять.
