# Задача: P3 — Remote/SSH транспорт, типизированные ошибки, structured logging

> **Статус задачи:** blocked on P2 · **Зависимости:** P2 завершён
> **Создан:** 2026-07-15

## Роль
Ты — senior инженер. Доводишь проект до индустриального уровня: Remote/SSH режимы
на WS, типобезопасные ошибки, structured logging. TDD везде, кроме logging.

## Контекст (читай перед работой)
1. `docs/plans/ADR-004-ws-transport-contract.md` — Local-режим готов (P1+P2).
   Remote/SSH в ADR-004 отмечены как «не покрывает — требуют проверки удалённой
   установки».
2. P1 и P2 должны быть ВЫПОЛНЕНЫ. Сначала прогони P-AUDIT — он уточнит,
   какие Remote/SSH-проблемы реальны.
3. `chat.rs` `send_message` — ветки `ConnectionMode::Remote` (865-877) и
  `ConnectionMode::Ssh` (878-913) до сих пор зовут удалённый/HTTP-путь.
4. `gateway.rs`, `ssh.rs`, `config.rs` — текущие error-типы все `String`.
5. Cargo.toml: `thiserror` и `tracing` ОТСУТСТВУЮТ (Phase 0/P1/P2 их не вводили).

## Проблема
После P1/P2 Local-режим работает на WS, но Remote/SSH сломаны (зовут удалённый
HTTP). Все ошибки — `Result<T, String>` (фронтенд не может отличить
«gateway not started» от «network error»). Логи — `eprintln!`, без уровней
и полей — невозможно фильтровать в продакшене.

## Root cause
- `chat.rs:865-913` — Remote/SSH-ветки не мигрированы на WS (Phase 0 явно
  оставила их «вне границ»).
- ADR-004 не зафиксировал Remote/SSH-контракт — его надо вывести эмпирически
  (как Local выводился инспекцией живой установки).
- `Result<T, String>` — наследие быстрого портирования; ADR-004 §Последствия
  упоминает thiserror как отдельную задачу.
- `eprintln!` — выбран в Phase 0 как «как окружающий код»; теперь окружающий
  код — это WS-транспорт, и logging надо поднимать системно.

## Подзадачи

### P3.1 — Исследовать и зафиксировать Remote/SSH контракт
- Это ИССЛЕДОВАНИЕ, не код. Поднять удалённый Hermes (или SSH-туннель к нему),
  проверить: какой порт слушает, требует ли token, работает ли `/api/ws` через
  туннель. Зафиксировать в **ADR-005** (новый).
- **DoD:** ADR-005 описывает Remote/SSH WS-контракт. Без этого P3.2 не стартует.

### P3.2 — Мигрировать Remote/SSH на WS (после ADR-005)
- **RED:** интеграционный тест через SSH-туннель к тестовому бэкенду.
- **GREEN:** `send_message` Remote/SSH-ветки зовут `send_message_via_ws`
  с URL `ws://<tunnel-or-remote>/api/ws?token=...`.
- **DoD:** Remote-режим работает через WS. SSH-туннель проксирует WS корректно.

### P3.3 — Типизированные ошибки через thiserror
- **Контракт (ADR-004 §Последствия):** доменные Error-enum'ы.
- **RED:** тест, который матчит конкретный вариант ошибки
  (`matches!(err, GatewayError::Timeout { .. })`) — падает, т.к. сейчас String.
- **GREEN:** ввести `thiserror`, определить:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum GatewayError {
      #[error("hermes installation not found")] NotInstalled,
      #[error("gateway failed to start: {0}")] SpawnFailed(String),
      #[error("gateway did not become ready within {timeout}s")] Timeout { timeout: u64 },
      #[error("api/session token missing")] TokenMissing,
      #[error("websocket transport error: {0}")] Ws(String),
  }
  pub type Result<T> = std::result::Result<T, GatewayError>;
  ```
  Мигрировать gateway.rs + ws_transport.rs + chat.rs Local-путь.
  Tauri сериализует Error в JSON → фронтенд может матчить по типу.
- **DoD:** `Result<T, String>` → `Result<T, GatewayError>` в затронутых модулях.
  Existing-тесты зелёные.

### P3.4 — Structured logging через tracing
- **Контракт:** `tracing` + `tracing-subscriber` (env-filter + json feature).
- НЕ TDD (логирование — не поведение). Но RED-стиль: до ввода — grep `eprintln!`
  в затронутом коде, после — grep должен показать 0 в gateway.rs/chat.rs/ws_transport.rs.
- **DoD:** `tracing::info!(port=%p, "gateway started")` вместо `eprintln!`.
  Init в `main.rs`/`lib.rs` startup. Env-filter `RUST_LOG`.
- Обновить ADR-004: logging теперь tracing, не eprintln.

## Жёсткие ограничения
- P3.1 (ADR-005) — обязательный гейт перед P3.2. Не кодируй Remote/SSH наугад,
  если не проверил контракт (урок P0: угаданный контракт = сломанный код).
- Новые crate (thiserror, tracing, tracing-subscriber) — обосновать в ADR
  (расширить ADR-004 §Последствия или ADR-005). «Новые crate только с ADR».
- Не переписывай Zustand stores / не вводи react-query (это вне ADR-004 scope,
  отдельный фронтенд-трек).
- P3.3 и P3.4 можно делать параллельно (независимы).

## Verification
- `cargo test` — зелёные, включая Remote/SSH integration (если есть тестовая
  установка) и error-matching тесты.
- `cargo clippy -D warnings` — чист.
- Демонстрация: фронтенд получает типизированную ошибку и показывает
  человекочитаемое сообщение (не generic «Error»).
- `RUST_LOG=debug` показывает structured поля (port, session_id, error).
- ADR-005 написан; ADR-004 обновлён (thiserror/tracing).
