/**
 * Extract ESLint violations into a sorted baseline file.
 *
 * Fingerprint = ruleId + ":" + relativeFilePath + ":" + normalizedMessage
 *
 * We deliberately exclude line/column numbers: they shift on unrelated edits
 * (added imports, reordered code) and would cause false "new violation" reports.
 * The (rule, file, message) triple is stable for the hermes-rpc/no-raw-invoke
 * rule because the message includes the offending command name.
 *
 * Usage: node scripts/extract-eslint-baseline.mjs > ci/eslint-baseline.json
 *   reads  .artifacts/eslint.json   (produced by `npm run lint:report`)
 *   writes sorted JSON array to stdout
 */
import { readFileSync } from "node:fs";
import { resolve, relative } from "node:path";
import process from "node:process";

const cwd = process.cwd();
const reportPath = resolve(cwd, ".artifacts/eslint.json");

let data;
try {
  data = JSON.parse(readFileSync(reportPath, "utf8"));
} catch (e) {
  console.error(`Cannot read ${reportPath}: ${e.message}`);
  console.error("Run `npm run lint:report` first.");
  process.exit(2);
}

const fingerprints = new Set();

for (const file of data) {
  const abs = file.filePath;
  let rel = abs;
  if (abs.startsWith(cwd)) {
    rel = relative(cwd, abs);
  }
  rel = rel.replace(/\\/g, "/");

  for (const msg of file.messages || []) {
    if (!msg.ruleId) continue;
    // Normalize: collapse internal whitespace, trim.
    const normMsg = String(msg.message).replace(/\s+/g, " ").trim();
    fingerprints.add(`${msg.ruleId}|${rel}|${normMsg}`);
  }
}

const sorted = [...fingerprints].sort();
process.stdout.write(JSON.stringify(sorted, null, 2) + "\n");
