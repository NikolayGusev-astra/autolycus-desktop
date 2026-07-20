# ADR-007: Live Source Cards via stdio MCP client

> **Статус:** принято
> **Дата:** 2026-07-18
> **Решение:** Steersman запускает stdio MCP серверы как подпроцессы, общается
> по JSON-RPC (newline-delimited), получает структурированные данные и
> рендерит live-карточки в Ленте активности. v1 — email.

---

## Контекст

Лента активности (`FeedView.tsx`) показывает только лог прошлых чат-сессий из
`state.db`. Референсные скриншоты показывают **живые данные** из источников:
непрочитанные письма, задачи, встречи, обновления каналов.

Решено (согласовано с пользователем):
- Источники по приоритету: Почта → Jira → Календарь → Каналы/Confluence
- Свежесть: гибрид (опрос при открытии + фон каждые 5 мин)
- MCP процессы: **свои stdio подпроцессы** (креды из config.yaml)
- Брифинг (LLM-анализ) остаётся; Live cards — отдельный слой
- Скоуп v1: Email end-to-end

## Контракт

### McpStdioClient (`mcp_client.rs`)
- `spawn(config, env)` → `tokio::process::Command` + `Stdio::piped()` (stdin/stdout/stderr)
- `initialize()` → JSON-RPC `{method:"initialize"}`, ждать ответа
- `call_tool(name, args)` → `{method:"tools/call", params:{name, arguments}}`, ждать ответа по id
- `shutdown()` → kill child
- Framing: newline-delimited JSON (`write_all + \n`, read line)

### Что НЕ меняется
- Брифинг, WS persistent connection, существующие tiles Ленты

### Что добавляется
- `mcp_client.rs` — generic stdio MCP клиент
- `feed_sources.rs` — email fetcher
- `list_email_unread_cmd` Tauri команда
- Email tile в FeedView

## Последствия
- +1 подпроцесс на каждый source (email). v1: spawn-per-request.
- v2: keep-alive пул, Jira/Calendar/Confluence tiles.

## Риски
| Риск | Митигация |
|---|---|
| MCP падает при spawn | Err → UI "Почта не настроена" |
| Процесс течёт | shutdown() + Drop |
| EMAIL_PASSWORD пустой | sources.json sync проверяет; UI "требует настройки" |
