# План: Починить WS chat — payload extraction + session.create params

> Применены cursor-скилы: `why` (root cause из live Hermes Desktop + upstream Python),
> `architect` (cross-boundary design), `principle-fix-root-causes`.
> Источники: `tui_gateway/server.py:8464-9462` (upstream контракт) +
> `apps/desktop/.../gateway-event.ts:113-702` (reference Electron UI).

## Диагноз (точно, из исходников)

Прямой WS-показал: backend ПРИНИМАЕТ `prompt.submit` и СТРИМИТ токены
(`message.delta`). Steersman их не видит. Причина — **неправильное чтение payload**.

Wire-формат события (server.py:1140-1144, `_emit`):
```
{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":"<sid>","payload":{"text":"<token>"}}}
```
Текст = `params.payload.text`. А Steersman читает `params.text` / `params.delta`
(оба пустые). Hermes Desktop читает `payload?.text` (gateway-event.ts:311).

## Задачи (verifiable units)

### F1 — extract_payload helper + фикс token/reasoning/tool (КРИТИЧНО)
- Добавить helper `payload_text(value) -> Option<&str>`: читает
  `params.payload.text` с fallback на `params.text` (обратная совместимость).
- `message.delta`: content из `payload.text` (не params.text).
- `reasoning.delta`/`thinking.delta`: content из `payload.text`.
- `tool.start`/`tool.complete`: `name`, `tool_id`, `output` — из `payload`
  (в upstream `_emit` payload = `{name, tool_id, output, duration_ms}`).
- `message.complete`: `payload.text` = финальный текст, `payload.usage` = метрики.
- **TDD**: тесты с реальным wire-format fixture (`params.payload.text`).
- Файл: `chat.rs:107-210` (`parse_gateway_event`).

### F2 — session.create params (source/cols)
- `ws_transport.rs:122-128`: добавить `source:"desktop"`, `cols:96` в params
  (как Electron Desktop: gateway-event.ts reference). Без `source` backend
  может логировать warning; `cols` нужно для рендеринг-ширины.
- Файл: `ws_transport.rs` `create_session`.

### F3 — TDD-тест с полным wire-format fixture
- Тест: реальный envelope `{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","session_id":"s1","payload":{"text":"Hello"}}}`
  → `ChatEvent::Token{content:"Hello"}`. Сейчас падает (читает params.text).

## Verification (принцип prove-it-works)
1. `cargo test --lib chat::` — все зелёные, включая F1/F3 тесты.
2. e2e: dev → отправить «привет» → токены стримятся в UI.
3. Сравнение wire-формата с Hermes Desktop: `params.payload.text`.

## Что НЕ трогать
- `prompt.submit` формат — совпадает с desktop (агент подтвердил byte-for-byte).
- Frontend ChatView listener — работает (получает ChatEvent::Token через Tauri emit).
- `session.create` обязательность — уже делаем перед prompt.submit.
