# Задача: P2 — удалить HTTP dead code, перевести WS в долгоживущий режим

> **Статус задачи:** blocked on P1 · **Зависимости:** P1 стабилен ≥1 e2e прогона
> **Создан:** 2026-07-15

## Роль
Ты — senior Rust-инженер. Убираешь технический долг после того, как WS-транспорт
(P1) стал продакшен-дефолтом и доказал стабильность. TDD где применимо.

## Контекст (читай перед работой)
1. `docs/plans/ADR-004-ws-transport-contract.md` — WS теперь primary транспорт.
2. P1 должен быть ВЫПОЛНЕН до старта P2 (spawn=serve, WS=дефолт). Если нет —
   остановись и вернись к P1.
3. `src-tauri/src/chat.rs` — HTTP-функции, ставшие dead code:
   `send_message_via_api` (~177-350), `send_message_via_runs` (~550-686),
   `supports_runs_transport` (~517-544), `send_via_best_transport` (~775-807),
   `parse_runs_event` (~692-768).
4. `src-tauri/src/ws_transport.rs` — сейчас connect-per-message (Phase 0).
5. Сначала прогони P-AUDIT — его список dead code точнее §7.5 SDD.

## Проблема
После P1 в репозитории ~600 строк HTTP-транспорта, которые больше не вызываются.
Они путают читателя, раздували `as any`-касты, и любой рефакторинг ModelConfig
требует их синхронизировать. Плюс connect-per-message в WS создаёт оверхед
(TCP+WS handshake на каждое сообщение) и не позволяет серверу пушить
события между turn'ами (status updates, async tool completion).

## Root cause
- `chat.rs` — пять HTTP-функций (список выше) достижимы только через
  `STEERSMAN_TRANSPORT=http`, который после P1 не дефолт. Grep вызовов
  подтверждает: вне переключателя их нет.
- `ws_transport.rs` — `send_message_via_ws` открывает новое соединение на каждый
  `send_message_cmd`. upstream рассчитывает на долгоживущее WS
  (`session["transport"]` перепривязывается, но соединение держит клиент).

## Подзадачи

### P2.1 — Подтвердить и удалить HTTP dead code
- **Шаг 1 (RED):** тест, который вызывает удалённые функции — падает
  (функций нет). Это страховка, что ничего внешнего их не юзает.
  Альтернатива: `cargo modules` / grep-аудит вызовов.
- **Шаг 2:** удалить send_message_via_api, send_message_via_runs,
  supports_runs_transport, send_via_best_transport, parse_runs_event.
  Удалить переключатель `STEERSMAN_TRANSPORT` (WS теперь единственный путь
  в Local; legacy-флаг больше не нужен).
- **Шаг 3:** убрать `reqwest`-зависимость из chat.rs, ЕСЛИ он не используется
  в других модулях (проверить grep'ом по crate; reqwest может быть нужен
  для mcp/ssh/брифинга — тогда оставить).
- **DoD:** `cargo build` зелёный, `cargo test` зелёный, `cargo clippy` чист.

### P2.2 — Долгоживущее WS-соединение
- **Контракт:** одно WS на lifetime gateway-процесса (или на session).
  Рекомендация — одно на gateway-процесс + отдельное подключение-на-сессию
  по мере создания (как делает оригинальный десктоп).
- **RED:** интеграционный тест (#[ignore], нужен живой бэкенд):
  открыть WS → session.create → prompt.submit #1 → дождаться done →
  prompt.submit #2 на ТОМ ЖЕ соединении → done. Доказывает, что соединение
  переиспользуется.
- **GREEN:** рефакторинг `ws_transport.rs`:
  - структура `WsConnection { ws, session_id }`, хранится в `GatewayState`
    (новое поле `ws: Arc<Mutex<Option<WsConnection>>>`);
  - `send_message_via_ws` переиспользует существующее соединение или
    открывает новое при отсутствии/разрыве;
  - реконнект: при ошибке сокета — один ретрай с новым connect.
- **DoD:** второе сообщение в диалоге не делает новый handshake.
  Логирование (`eprintln!("[ws] reuse session=X"` / `[ws] reconnect"`).

### P2.3 — Удалить прочий dead code (из P-AUDIT §7.5/§7.6)
- Только то, что P-AUDIT подтвердил как dead/stale:
  - `chat.rs:354-407 send_message_via_gateway` (old WS stub — теперь заменён
    на ws_transport.rs);
  - `config_health.rs` (весь модуль, проверяет несуществующий sessions.db);
  - `src-tauri/python/tcp_server.py`;
  - `gateway.rs:393 check_gateway_health` (дубликат inline health).
- **DoD:** grep вызовов = 0 для каждого. Удаление + `cargo build` зелёный.

## Жёсткие ограничения
- P2.1 (удаление HTTP) — ТОЛЬКО после подтверждения P1 стабилен хотя бы 1
  прогоном e2e. Не удаляй транспорт, которым люди реально пользуются.
- Не трогай Remote/SSH (P3).
- P2.2 — не делай «идеальный» connection-pool. Минимум: одно соединение,
  переиспользование, один ретрай. Сложный pooling — отдельная задача.
- Каждое удаление — отдельный коммит (чтобы откатить точечно).

## Verification
- `cargo test` — зелёные (новые WS-тесты + регрессия).
- `cargo clippy -D warnings` — чист.
- Размер `chat.rs` после P2.1 должен уменьшиться на ~600 строк — подтверди `wc -l`.
- E2E: диалог из 3+ сообщений — второе и третье используют одно соединение
  (видно по логам `[ws] reuse`).
- `git diff --stat` в финальном коммите покажет net-удаление строк.
