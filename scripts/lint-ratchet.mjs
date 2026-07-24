/**
 * Lint ratchet: multiset comparator for ESLint and Clippy.
 *
 * A baseline is a JSON object mapping fingerprint → count:
 *   { "rule|file|message": 3, ... }
 *
 * Ratchet rule (per fingerprint, multiset semantics):
 *   live[f] <= baseline[f]   → OK (shrinkage allowed)
 *   live[f] >  baseline[f]   → FAIL (growth forbidden)
 *   baseline[f] exists, live[f] missing → RESOLVED (OK, report it)
 *
 * This blocks adding a second identical violation where one was baselined,
 * unlike a plain Set which collapses duplicates.
 *
 * Subcommands:
 *   extract <eslint|clippy>   Read .artifacts/<tool>.json, write baseline to stdout.
 *   check   <eslint|clippy>   Compare live report against ci/<tool>-baseline.json.
 *   strict  clippy            Zero warnings allowed in STRICT_FILES.
 *
 * Usage:
 *   node scripts/lint-ratchet.mjs extract eslint > ci/eslint-baseline.json
 *   node scripts/lint-ratchet.mjs check   eslint
 *   node scripts/lint-ratchet.mjs strict  clippy
 */
import { readFileSync, existsSync, writeFileSync } from "node:fs";
import { resolve, relative } from "node:path";
import process from "node:process";

const cwd = process.cwd();

// Files that must stay clippy-clean (checked by `strict`, excluded from baseline).
// hermes_protocol.rs has ~50 dead_code from Phase 1A DTOs not yet wired in;
// it joins STRICT when Phase 1C consumes those types.
const STRICT_FILES = new Set(["src/ws_transport.rs"]);

function norm(p) {
  let rel = p;
  if (rel.startsWith(cwd)) rel = rel.slice(cwd.length + 1);
  return rel.replace(/\\/g, "/");
}

function normMsg(s) {
  return String(s).replace(/\s+/g, " ").trim();
}

// ── ESLint fingerprinting ──────────────────────────────────────────────────
function eslintFingerprints(reportPath) {
  const data = JSON.parse(readFileSync(reportPath, "utf8"));
  const counts = {};
  for (const file of data) {
    const rel = norm(file.filePath);
    for (const msg of file.messages || []) {
      if (!msg.ruleId) continue;
      const fp = `${msg.ruleId}|${rel}|${normMsg(msg.message)}`;
      counts[fp] = (counts[fp] || 0) + 1;
    }
  }
  return counts;
}

// ── Clippy fingerprinting ──────────────────────────────────────────────────
function primarySpan(spans) {
  // Use the primary span; fall back to first. Macros/multi-span diagnostics
  // would otherwise be misattributed to a non-primary file.
  return (spans || []).find((s) => s.is_primary) || (spans || [])[0];
}

function clippyFingerprints(reportPath, { excludeStrict = false } = {}) {
  const raw = readFileSync(reportPath, "utf8");
  const counts = {};
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
    const span = primarySpan(m.spans);
    if (!span) continue;
    const rel = norm(span.file_name);
    if (excludeStrict && STRICT_FILES.has(rel)) continue;
    const fp = `${m.code.code}|${rel}|${normMsg(m.message)}`;
    counts[fp] = (counts[fp] || 0) + 1;
  }
  return counts;
}

// ── Multiset comparator ────────────────────────────────────────────────────
function compare(live, baseline) {
  const grown = [];
  const resolved = [];
  for (const fp of Object.keys({ ...live, ...baseline })) {
    const l = live[fp] || 0;
    const b = baseline[fp] || 0;
    if (l > b) grown.push({ fp, baseline: b, live: l });
    else if (b > 0 && l === 0) resolved.push(fp);
  }
  grown.sort((a, b) => a.fp.localeCompare(b.fp));
  resolved.sort();
  return { grown, resolved };
}

function totalViolations(counts) {
  return Object.values(counts).reduce((a, b) => a + b, 0);
}

// ── CLI ────────────────────────────────────────────────────────────────────
const [cmd, tool] = process.argv.slice(2);

function fail(msg, code = 1) {
  console.error(msg);
  process.exit(code);
}

if (!["extract", "check", "strict", "guard"].includes(cmd) || !["eslint", "clippy"].includes(tool)) {
  fail(`Usage: node scripts/lint-ratchet.mjs <extract|check|strict|guard> <eslint|clippy>`);
}

const reportPath = resolve(cwd, `.artifacts/${tool}.json`);
const baselinePath = resolve(cwd, `ci/${tool}-baseline.json`);

// ── guard: baseline monotonicity vs target branch ──────────────────────────
//
// The `check` command compares the live lint report against the baseline in the
// SAME branch. That alone lets a developer add a violation, regenerate the
// baseline, and commit both — CI stays green. `guard` closes that hole by
// enforcing that the committed baseline never grows relative to the target
// branch (origin/main), so the only way to silence a violation is to actually
// remove it.
//
// Multiset rule: prBaseline[f] <= mainBaseline[f] for every fingerprint f.
// Shrinkage is allowed and reported; growth fails.
if (cmd === "guard") {
  const { execSync } = await import("node:child_process");
  const target = process.env.RATCHET_TARGET || "origin/main";

  function loadBaseline(ref) {
    // Read the baseline file as it exists at `ref` (empty if absent).
    let blob;
    try {
      blob = execSync(`git show ${ref}:ci/${tool}-baseline.json`, { cwd, stdio: ["pipe", "pipe", "pipe"] }).toString();
    } catch {
      return {}; // baseline doesn't exist at ref yet — treat as empty
    }
    try {
      const parsed = JSON.parse(blob);
      // Migrate legacy array format (Set-based) → multiset (count 1 each).
      if (Array.isArray(parsed)) {
        const ms = {};
        for (const fp of parsed) ms[fp] = (ms[fp] || 0) + 1;
        return ms;
      }
      return parsed;
    } catch {
      fail(`ci/${tool}-baseline.json at ${ref} is not valid JSON`, 2);
    }
  }

  if (!existsSync(baselinePath)) {
    fail(`Cannot read ${baselinePath}. Generate it with 'npm run ${tool}:baseline:gen'.`, 2);
  }
  const prBaselineRaw = JSON.parse(readFileSync(baselinePath, "utf8"));
  // Local baseline must be multiset (object). If it's still the legacy array,
  // that's a bug — the generator always emits an object now.
  if (Array.isArray(prBaselineRaw)) {
    fail(`ci/${tool}-baseline.json is legacy array format. Regenerate with 'npm run ${tool}:baseline:gen'.`, 2);
  }
  const prBaseline = prBaselineRaw;
  const mainBaseline = loadBaseline(target);

  const { grown, resolved } = compare(prBaseline, mainBaseline);
  // compare() treats prBaseline as "live" and mainBaseline as "baseline".
  // grown = fingerprints that increased in PR vs main (forbidden).
  // resolved = fingerprints present in main but gone in PR (allowed).

  if (resolved.length > 0) {
    const rcount = resolved.reduce((a, fp) => a + mainBaseline[fp], 0);
    console.log(`✓ ${tool} baseline shrank by ${rcount} violation(s) vs ${target}.`);
  }

  if (grown.length === 0) {
    console.log(`✓ ${tool} baseline guard OK: no growth vs ${target}.`);
    process.exit(0);
  }

  const gcount = grown.reduce((a, g) => a + (g.live - g.baseline), 0);
  console.error(`✗ ${tool} baseline guard FAILED: baseline grew by ${gcount} vs ${target}:`);
  for (const g of grown) {
    console.error(`  + ${g.live - g.baseline} new (${g.baseline}→${g.live})  ${g.fp}`);
  }
  console.error("");
  console.error("The baseline may only shrink. To add debt, get explicit maintainer approval");
  console.error("in the PR review — do not silently regenerate the baseline.");
  process.exit(1);
}

if (!existsSync(reportPath) && cmd !== "guard") {
  fail(`Cannot read ${reportPath}. Run 'npm run ${tool}:report' first.`, 2);
}

if (cmd === "extract") {
  const counts =
    tool === "eslint" ? eslintFingerprints(reportPath) : clippyFingerprints(reportPath, { excludeStrict: true });
  process.stdout.write(JSON.stringify(counts, null, 2) + "\n");
  process.exit(0);
}

if (cmd === "strict") {
  // Only clippy has strict modules.
  if (tool !== "clippy") fail("strict is only defined for clippy");
  const counts = clippyFingerprints(reportPath, { excludeStrict: false });
  const strict = {};
  for (const fp of Object.keys(counts)) {
    const file = fp.split("|")[1];
    if (STRICT_FILES.has(file)) strict[fp] = counts[fp];
  }
  const n = totalViolations(strict);
  if (n === 0) {
    console.log(`✓ Clippy strict OK: 0 warnings in strict modules (${[...STRICT_FILES].join(", ")}).`);
    process.exit(0);
  }
  console.error(`✗ Clippy strict FAILED: ${n} warning(s) in strict modules:`);
  for (const [fp, c] of Object.entries(strict)) console.error(`  + (${c}x) ${fp}`);
  process.exit(1);
}

// cmd === "check"
if (!existsSync(baselinePath)) {
  fail(`Cannot read ${baselinePath}. Generate it with 'npm run ${tool}:baseline:gen'.`, 2);
}
const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
const live =
  tool === "eslint" ? eslintFingerprints(reportPath) : clippyFingerprints(reportPath, { excludeStrict: true });

const { grown, resolved } = compare(live, baseline);

if (resolved.length > 0) {
  const rcount = resolved.reduce((a, fp) => a + baseline[fp], 0);
  console.log(`✓ ${rcount} violation(s) resolved since baseline — shrink the baseline to lock in.`);
  for (const fp of resolved) console.log(`  - (${baseline[fp]}x) ${fp}`);
}

if (grown.length === 0) {
  console.log(`✓ ${tool} ratchet OK: ${totalViolations(live)} live, ${totalViolations(baseline)} baseline, 0 grown.`);
  process.exit(0);
}

const gcount = grown.reduce((a, g) => a + (g.live - g.baseline), 0);
console.error(`✗ ${tool} ratchet FAILED: ${gcount} NEW violation(s) beyond baseline:`);
for (const g of grown) {
  console.error(`  + ${g.live - g.baseline} new (${g.baseline}→${g.live})  ${g.fp}`);
}
console.error("");
console.error("Fix the new violations. To intentionally grow the baseline, a maintainer must");
console.error("review the baseline diff in the PR — growth is never automatic.");
process.exit(1);
