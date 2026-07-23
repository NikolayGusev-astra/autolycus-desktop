/**
 * Strict Clippy gate: zero warnings allowed in the actively-developed modules.
 *
 * Unlike check-clippy-baseline.mjs (which allows pre-existing debt), this gate
 * FAILS on ANY clippy warning in the strict modules. New code in these files
 * must be clean; there is no baseline to hide behind.
 *
 * Exit codes:
 *   0  OK — no clippy warnings in strict modules.
 *   1  FAIL — one or more warnings found in a strict module.
 *   2  ERROR — could not read the clippy report.
 *
 * Usage: node scripts/check-clippy-strict.mjs
 *   reads .artifacts/clippy.json  (produced by `npm run clippy:report`)
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";

const cwd = process.cwd();
const reportPath = resolve(cwd, ".artifacts/clippy.json");

// Must match extract-clippy-baseline.mjs.
const STRICT_FILES = new Set(["src/ws_transport.rs"]);

let raw;
try {
  raw = readFileSync(reportPath, "utf8");
} catch (e) {
  console.error(`Cannot read ${reportPath}: ${e.message}`);
  console.error("Run `npm run clippy:report` first.");
  process.exit(2);
}

const violations = [];
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
  if (!STRICT_FILES.has(rel)) continue;
  violations.push({ code: m.code.code, file: rel, line: span.line_start, message: m.message });
}

if (violations.length === 0) {
  console.log(`✓ Clippy strict OK: 0 warnings in strict modules (${[...STRICT_FILES].join(", ")}).`);
  process.exit(0);
}

console.error(`✗ Clippy strict FAILED: ${violations.length} warning(s) in strict modules:`);
for (const v of violations) {
  console.error(`  + ${v.code}  ${v.file}:${v.line}  ${v.message}`);
}
console.error("");
console.error("Strict modules must be warning-free. Fix these; do not baseline them.");
process.exit(1);
