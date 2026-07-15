# Задача: T1 — config_health.rs хардкодит устаревший порт 8642

## Роль
Ты — senior Rust-инженер в проекте Steersman Desktop. Фиксишь staleness
в модуле config_health. Действуй по TDD: сначала тест, затем фикс.

## Контекст (читай перед работой)
- `docs/plans/ADR-004-ws-transport-contract.md` — `hermes serve --port 0`
  назначает порт ОС; фиксированного 8642 больше нет.
- `SDD.md` §7.6 — config_health.rs помечен stale (порт 8642).
- Тех-стек: Rust 2021, Tauri 2.

## Проблема
`auto_fix_issue("CONFIG_MISSING")` создаёт дефолтный config.yaml с
`platforms.api_server.port: 8642` (`config_health.rs:140-149`). На реальной
установке этот порт не слушается (ADR-004: `hermes serve --port 0`). Пользователь,
запустивший auto-fix, получает конфиг, ссылающийся на несуществующий endpoint.

## Root cause (из аудита)
`src-tauri/src/config_health.rs:148` — `port: 8642` хардкожен в шаблоне
дефолт-конфига. Потому что ADR-002 предполагал фиксированный API Server порт,
но ADR-004 supersede'нул это: порт назначается ОС.
Контракт требует: дефолт-конфиг не должен хардкодить несуществующий порт;
`platforms.api_server` секция либо отсутствует (бэкенд сам дефолтит), либо
содержит корректный placeholder.

## Definition of Done
1. Тест RED: парсит шаблон дефолт-конфига и ассертит отсутствие `8642`.
2. Фикс GREEN: убрать `platforms.api_server.port: 8642` из шаблона (оставить
   только `model:` блок, либо закомментировать api_server с пометкой).
3. `cargo test --lib` — зелёные.

## Ограничения
- Не удаляй весь модуль config_health (он живой — Tauri-команда).
- Не меняй структуру HealthReport/HealthIssue.
- Сохраняй стиль окружающего кода.
