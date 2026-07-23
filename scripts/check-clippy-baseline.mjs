/**
 * Ratchet comparator for Clippy warnings.
 *
 * Exit codes:
 *   0  OK — no new fingerprints; old ones may have decreased.
 *   1  FAIL — one or more NEW fingerprints not in the baseline.
 *   2  ERROR — could not read inputs (run `npm run clippy:report` first).
 *
 * Strict modules (listed in extract-clippy-baseline.mjs::STRICT_FILES) are
 * excluded from BOTH the baseline and the live set here: they must be warning-
 * free and are enforced by a separate `cargo clippy -- -D warnings` gate.
 *
 * Semantics (ratchet), identical to check-eslint-baseline.mjs:
 *   - live fingerprint not in baseline → NEW → FAIL.
 *   - baseline fingerprint not live → RESOLVED → OK (shrink baseline to lock in).
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";

const cwd = process.cwd();
const reportPath = resolve(cwd, ".artifacts/clippy.json");
const baselinePath = resolve(cwd, "ci/clippy-baseline.json");

// Must match extract-clippy-baseline.mjs.
const STRICT_FILES = new Set(["src/ws_transport.rs"]);

function readJsonOrExit(p) {
  try {
    return JSON.parse(readFileSync(p, "utf8"));
  } catch (e) {
    console.error(`Cannot read ${p}: ${e.message}`);
    process.exit(2);
  }
}

const raw = readFileSync(reportPath, "utf8");
const baseline = new Set(readJsonOrExit(baselinePath));

// Rebuild live fingerprint set, mirroring the extractor exactly.
const live = new Set();
for (const line of raw.split("\n")) {
  if (!line.trim()) continue;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    continue;
  }
  if (msg.reason !== "compiler-message") continue;
  const m = msg.message;
  if (!m || !m.code || !m.code.code) continue;
  if (m.level !== "warning") continue;
  const span = (m.spans || [])[0];
  if (!span) continue;
  const rel = (span.file_name || "").replace(/\\/g, "/");
  if (STRICT_FILES.has(rel)) continue; // strict: never baseline, never ignored here
  const normMsg = String(m.message).replace(/\s+/g, " ").trim();
  live.add(`${m.code.code}|${rel}|${normMsg}`);
}

const newOnes = [...live].filter((f) => !baseline.has(f)).sort();
const resolved = [...baseline].filter((f) => !live.has(f)).sort();

if (resolved.length > 0) {
  console.log(`✓ ${resolved.length} clippy warning(s) resolved since baseline (shrink the baseline to lock this in):`);
  for (const f of resolved) console.log(`  - ${f}`);
}

if (newOnes.length === 0) {
  console.log(`✓ Clippy ratchet OK: ${live.size} live fingerprint(s), ${baseline.size} baseline, 0 new.`);
  process.exit(0);
}

console.error(`✗ Clippy ratchet FAILED: ${newOnes.length} NEW warning(s) not in baseline:`);
for (const f of newOnes) console.error(`  + ${f}`);
console.error("");
console.error("Fix the new warnings, or (if intentional) regenerate the baseline:");
console.error("  npm run clippy:report && node scripts/extract-clippy-baseline.mjs > ci/clippy-baseline.json");
process.exit(1);
