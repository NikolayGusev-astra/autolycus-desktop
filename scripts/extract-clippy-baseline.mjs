/**
 * Extract Clippy warnings into a sorted baseline file.
 *
 * Fingerprint = lintCode + ":" + relativeFilePath + ":" + normalizedMessage
 *
 * Line numbers are excluded (they drift on unrelated edits). The (code, file,
 * message) triple is stable: clippy messages are deterministic for a given
 * construct.
 *
 * Excludes files in the "strict modules" list — those must be warning-free and
 * are checked separately with `-D warnings`. Currently strict:
 *   src/ws_transport.rs, src/hermes_protocol.rs
 *
 * Usage: node scripts/extract-clippy-baseline.mjs > ci/clippy-baseline.json
 *   reads  .artifacts/clippy.json   (produced by `npm run clippy:report`)
 */
import { readFileSync } from "node:fs";
import { resolve, relative } from "node:path";
import process from "node:process";

const cwd = process.cwd();
const reportPath = resolve(cwd, ".artifacts/clippy.json");

// Files that must stay clippy-clean (checked separately under -D warnings).
// ws_transport.rs is the actively-developed persistent gateway client (Phase 1B).
// hermes_protocol.rs has ~50 dead_code warnings from Phase 1A DTO fields not yet
// wired into production callers; it joins STRICT once Phase 1C consumes them.
const STRICT_FILES = new Set(["src/ws_transport.rs"]);

function isStrict(rel) {
  return STRICT_FILES.has(rel);
}

let raw;
try {
  raw = readFileSync(reportPath, "utf8");
} catch (e) {
  console.error(`Cannot read ${reportPath}: ${e.message}`);
  console.error("Run `npm run clippy:report` first.");
  process.exit(2);
}

const fingerprints = new Set();
let skippedStrict = 0;

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
  if (m.level !== "warning") continue; // errors are never baseline-able
  const span = (m.spans || [])[0];
  if (!span) continue;

  let rel = span.file_name || "";
  rel = rel.replace(/\\/g, "/");

  if (isStrict(rel)) {
    skippedStrict++;
    continue;
  }

  const normMsg = String(m.message).replace(/\s+/g, " ").trim();
  fingerprints.add(`${m.code.code}|${rel}|${normMsg}`);
}

const sorted = [...fingerprints].sort();
if (skippedStrict > 0) {
  console.error(
    `note: excluded ${skippedStrict} warning(s) in strict modules (${[...STRICT_FILES].join(", ")}); ` +
      `these must be fixed, not baselined.`
  );
}
process.stdout.write(JSON.stringify(sorted, null, 2) + "\n");
