# CI Baseline & Ratchet

Frozen technical-debt snapshots that let `main` stay green while pre-existing
lint/clippy violations are paid down incrementally. The system is a **ratchet**,
not an amnesty.

## Design principles

1. **Multiset, not set.** Each baseline maps `fingerprint → count`. Adding a
   second identical violation where one was baselined is caught (count grows).
2. **Baseline never grows silently.** `guard` compares the committed baseline
   against `origin/main` and fails on any growth. Shrinking is allowed.
3. **No `continue-on-error` on analyzers.** ESLint exit 1 (violations) is
   tolerated by a wrapper; exit 2+ (config/runtime error) aborts CI. Clippy
   warnings never set a non-zero exit without `-D warnings`, so no wrapper.
4. **Strict modules have no baseline.** `src/ws_transport.rs` must be warning-
   free; `clippy:strict` fails on any warning there.

## Files

| File | Purpose |
|------|---------|
| `ci/eslint-baseline.json` | `{ "fingerprint": count }` — frozen ESLint violations. |
| `ci/clippy-baseline.json` | `{ "fingerprint": count }` — frozen Clippy warnings. |
| `scripts/lint-ratchet.mjs` | Unified comparator: `extract`, `check`, `strict`, `guard`. |

## Fingerprint format

`ruleId|relativeFile|normalizedMessage` — line/column excluded (they drift on
unrelated edits). Whitespace in messages is collapsed.

## npm scripts

| Script | What it does |
|--------|--------------|
| `lint:report` | Run ESLint → `.artifacts/eslint.json` (tolerates exit 1, fails on 2+). |
| `lint:check` | Ratchet: live multiset ≤ baseline multiset. |
| `lint:guard` | Baseline monotonicity: PR baseline ≤ `origin/main` baseline. |
| `lint:baseline:gen` | Regenerate `ci/eslint-baseline.json` from current report. |
| `clippy:report` | Run clippy → `.artifacts/clippy.json`. |
| `clippy:check` | Ratchet for clippy. |
| `clippy:guard` | Baseline guard for clippy. |
| `clippy:strict` | Zero warnings in strict modules (`src/ws_transport.rs`). |
| `clippy:baseline:gen` | Regenerate clippy baseline. |

## Strict (always block, no baseline)

```
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
npm run typecheck
npm run test
npm run clippy:strict
```

## When you fix violations

```bash
npm run lint:report && npm run lint:baseline:gen > ci/eslint-baseline.json
npm run clippy:report && npm run clippy:baseline:gen > ci/clippy-baseline.json
git add ci/*-baseline.json
```

The ratchet reports the shrinkage; committing the smaller baseline locks it in.

## When you need to add debt

You don't — fix the violation. If genuinely unavoidable (temporary migration
bridge), a maintainer must review the baseline diff in the PR. The `guard`
step makes baseline growth visible and reviewable; it is never automatic.

## Strict modules

Defined in `scripts/lint-ratchet.mjs::STRICT_FILES`:

- `src/ws_transport.rs` — Phase 1B persistent gateway client.

`src/hermes_protocol.rs` has ~50 `dead_code` from Phase 1A DTO fields; it is
baselined and joins strict when Phase 1C consumes those types.

## Toolchain pinning

`src-tauri/rust-toolchain.toml` pins Rust 1.93.1 so clippy messages (and thus
fingerprints) stay stable. Bump only in a dedicated PR that also regenerates
the clippy baseline.
