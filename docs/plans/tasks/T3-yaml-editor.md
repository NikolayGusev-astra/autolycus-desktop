# Задача: T3 — line-based YAML editor → serde_yaml round-trip

## Роль
Ты — senior Rust-инженер. Заменяешь хрупкий line-based YAML editor на
безопасный serde_yaml round-trip. TDD.

## Контекст (читай перед работой)
- `docs/plans/ADR-002-hermes-api-contract.md` §«Config write».
- `SDD.md` §7.4 #19 — VALID (line-based хрупок).
- Тех-стек: Rust 2021, serde_yaml (deps), yaml-rust2 0.8.

## Проблема
`set_yaml_block_scalars` (`config.rs:830`) — line-based editor: читает config.yaml
по строкам, ищет блок по header, заменяет scalar-значения. Ломается на:
- комментариях (теряются при перезаписи),
- вложенных структурах с одинаковыми ключами,
- multi-line strings (|, >),
- quoted strings с escapes.
Вызывается из `update_model_config` (config.rs:809) и `sync_mcp_env_blocks`
(mcp.rs:348) — критичные пути записи config пользователя.

## Root cause (из аудита)
`src-tauri/src/config.rs:830-900` — алгоритм walk-by-lines вместо parse-modify-serialize.
Контракт требует: round-trip через serde_yaml (или yaml-rust2 с preserve-comments).

## Definition of Done
1. Тест RED: round-trip сохраняет комментарии и nested-структуры
   (создать fixture config.yaml с комментарием + nested, записать, прочитать,
   ассертить что комментарий и nested на месте).
2. Фикс GREEN: `set_yaml_block_scalars` переписан на serde_yaml Value
   round-trip (или новая функция `set_yaml_scalar_roundtrip`).
3. update_model_config + sync_mcp_env_blocks используют новый путь.
4. `cargo test --lib` — зелёные.

## Ограничения
- serde_yaml НЕ сохраняет комментарии по умолчанию. Если комментарии критичны —
  использовать yaml-rust2 (уже в deps, поддерживает preserve). Иначе принять
  потерю комментариев как trade-off (зафиксировать в ADR).
- Не ломай существующий format config.yaml (field order не критичен).
- Сохраняй публичную сигнатуру set_yaml_block_scalars (callers не должны меняться).
