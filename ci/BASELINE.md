# CI Baseline & Ratchet

This directory holds **frozen technical-debt snapshots** that let `main` stay
green while pre-existing lint/clippy violations are paid down incrementally.

The system is a **ratchet**, not an amnesty:

- existing debt is recorded once as a set of stable fingerprints;
- **new** violations fail CI immediately;
- resolved violations are reported and can be locked in by shrinking the
  baseline;
- the total count is never compared — only individual fingerprints — so
  swapping one old violation for a new one still fails.

## Files

| File | Purpose |
|------|---------|
| `ci/eslint-baseline.json` | Frozen ESLint fingerprints (rule \| file \| message). |
| `ci/clippy-baseline.json` | Frozen Clippy fingerprints (code \| file \| message). |

## Fingerprint stability

A fingerprint is `ruleId|relativeFile|normalizedMessage`. Line/column numbers
are deliberately excluded — they drift on unrelated edits (added imports,
reordered code) and would cause false "new violation" reports.

The `hermes-rpc/no-raw-invoke` rule embeds the offending command name in its
message (`Direct invoke('<cmd>') ...`), so two different commands in the same
file produce two distinct fingerprints.

## What is strict (never baseline-able)

These checks run **without** any baseline and always block on failure:

```
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
npm run typecheck
npm run test
npm run clippy:strict     # zero clippy warnings in src/ws_transport.rs
```

Compilation, formatting, type, and test failures are not technical debt.

## What is ratcheted

| Check | Script | Behavior |
|-------|--------|----------|
| ESLint | `npm run lint:baseline` | Fails on any `hermes-rpc/no-raw-invoke` (or other) fingerprint not in the baseline. |
| Clippy (all targets) | `npm run clippy:baseline` | Fails on any clippy warning fingerprint not in the baseline, excluding strict modules. |
| Clippy (strict modules) | `npm run clippy:strict` | Fails on ANY warning in `src/ws_transport.rs`. |

## Strict modules

Defined in `scripts/extract-clippy-baseline.mjs` and `scripts/check-clippy-strict.mjs`
(both must list the same set):

- `src/ws_transport.rs` — actively-developed persistent gateway client (Phase 1B).

`src/hermes_protocol.rs` currently has ~50 `dead_code` warnings from Phase 1A
DTO fields not yet wired into production callers. It is **baselined**, not
strict, and joins the strict set once Phase 1C consumes those types.

## Workflow after resolving violations

When you fix a violation, the ratchet reports it as resolved and passes, but
the stale entry lingers in the baseline. To lock the win in and prevent
regression:

```bash
npm run lint:report
node scripts/extract-eslint-baseline.mjs > ci/eslint-baseline.json

npm run clippy:report
node scripts/extract-clippy-baseline.mjs > ci/clippy-baseline.json
```

Commit the shrunken baseline alongside your fix.

## Workflow when adding intentional new debt

Normally: don't. Fix the violation instead. If a violation is genuinely
intended (e.g. a temporary bridge during a migration), regenerate the baseline
and explain why in the commit message and PR description:

```bash
npm run lint:report && node scripts/extract-eslint-baseline.mjs > ci/eslint-baseline.json
git add ci/eslint-baseline.json
```

The ratchet will then treat the new fingerprint as pre-existing.
