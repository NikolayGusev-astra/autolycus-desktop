/**
 * Ratchet comparator for ESLint violations.
 *
 * Exit codes:
 *   0  OK — no new fingerprints; old ones may have decreased.
 *   1  FAIL — one or more NEW fingerprints not in the baseline.
 *   2  ERROR — could not read inputs (run `npm run lint:report` first).
 *
 * Semantics (ratchet):
 *   - A fingerprint present in the live report but NOT in the baseline → NEW → FAIL.
 *   - A fingerprint present in the baseline but NOT in the live report → RESOLVED → OK,
 *     and printed so the developer can shrink the baseline.
 *   - Count comparisons are intentionally NOT used: swapping one old violation
 *     for a different new one must fail even if the total is unchanged.
 *
 * To shrink the baseline after resolving violations, re-run:
 *   npm run lint:report && node scripts/extract-eslint-baseline.mjs > ci/eslint-baseline.json
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";

const cwd = process.cwd();
const reportPath = resolve(cwd, ".artifacts/eslint.json");
const baselinePath = resolve(cwd, "ci/eslint-baseline.json");

function readJson(p) {
  try {
    return JSON.parse(readFileSync(p, "utf8"));
  } catch (e) {
    console.error(`Cannot read ${p}: ${e.message}`);
    process.exit(2);
  }
}

const report = readJson(reportPath);
const baseline = new Set(readJson(baselinePath));

// Rebuild the live fingerprint set the same way extract-eslint-baseline.mjs does.
const live = new Set();
for (const file of report) {
  let rel = file.filePath;
  if (rel.startsWith(cwd)) rel = rel.slice(cwd.length + 1);
  rel = rel.replace(/\\/g, "/");
  for (const msg of file.messages || []) {
    if (!msg.ruleId) continue;
    const normMsg = String(msg.message).replace(/\s+/g, " ").trim();
    live.add(`${msg.ruleId}|${rel}|${normMsg}`);
  }
}

const newOnes = [...live].filter((f) => !baseline.has(f)).sort();
const resolved = [...baseline].filter((f) => !live.has(f)).sort();

if (resolved.length > 0) {
  console.log(`✓ ${resolved.length} violation(s) resolved since baseline (shrink the baseline to lock this in):`);
  for (const f of resolved) console.log(`  - ${f}`);
}

if (newOnes.length === 0) {
  console.log(`✓ ESLint ratchet OK: ${live.size} live fingerprint(s), ${baseline.size} baseline, 0 new.`);
  process.exit(0);
}

console.error(`✗ ESLint ratchet FAILED: ${newOnes.length} NEW violation(s) not in baseline:`);
for (const f of newOnes) console.error(`  + ${f}`);
console.error("");
console.error("Fix the new violations, or (if intentional) regenerate the baseline:");
console.error("  npm run lint:report && node scripts/extract-eslint-baseline.mjs > ci/eslint-baseline.json");
process.exit(1);
