/**
 * Run ESLint with JSON output, tolerating "violations found" (exit 1) but
 * failing on config/runtime errors (exit 2+).
 *
 * ESLint exit codes:
 *   0 — clean
 *   1 — violations found (JSON report is still valid and complete)
 *   2 — configuration or internal error (report may be missing/garbage)
 *
 * The baseline comparator reads the JSON report, so this wrapper must leave a
 * valid .artifacts/eslint.json on exit 0/1 and abort on exit 2. Without it,
 * `continue-on-error: true` in CI would hide a broken ESLint config and let
 * the ratchet compare against a partial/empty report.
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, existsSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";

const cwd = process.cwd();
const artifactsDir = resolve(cwd, ".artifacts");
const outPath = resolve(artifactsDir, "eslint.json");
mkdirSync(artifactsDir, { recursive: true });

// Run eslint directly via the local node_modules binary.
const isWin = process.platform === "win32";
const eslintBin = resolve(cwd, "node_modules/.bin/eslint" + (isWin ? ".cmd" : ""));

const result = spawnSync(eslintBin, [
  "src",
  "--ext", "ts,tsx",
  "--format", "json",
  "--output-file", outPath,
], { stdio: "inherit", shell: isWin });

const code = result.status ?? 1;

if (code >= 2) {
  // Real failure: config error, missing plugin, parse error in the rule itself.
  // The report may be incomplete — do NOT let the ratchet run on it.
  console.error(`ESLint exited with code ${code} (config/runtime error). Report may be invalid; aborting.`);
  process.exit(code);
}

// code 0 or 1: ensure a parseable report exists.
if (!existsSync(outPath)) {
  writeFileSync(outPath, "[]");
}
console.log(`ESLint report written (exit ${code}: ${code === 0 ? "clean" : "violations present, deferred to ratchet"}).`);
process.exit(0);
