/**
 * ESLint rule: ban Tauri `invoke()` outside the service layer.
 *
 * ADR-001 boundary: `invoke` is only allowed in `src/services/**`. Components,
 * hooks, and stores must call typed service functions, never `invoke` directly.
 *
 * Previous version checked a command-name whitelist + string-literal first arg,
 * which was bypassable (variable arg, unlisted command, moving the call into a
 * hook/store). This version bans ANY `invoke` call regardless of arguments, so
 * the boundary is structural, not pattern-based.
 *
 * Allowed paths (configurable via options.allowedPaths, defaults below):
 *   src/services/   — the anti-corruption layer itself
 *   src-tauri/      — Rust code (not linted by ESLint anyway)
 *
 * NOT allowed (intentional tightening vs. the old rule):
 *   src/hooks/      — hooks depend on services, not on invoke
 *   src/stores/     — stores depend on services, not on invoke
 *
 * Existing violations in hooks/stores are captured in the lint baseline and
 * migrate to services during Phase 0.3A.
 */

const TAURI_SOURCES = new Set([
  "@tauri-apps/api/core",
  "@tauri-apps/api",
  "@tauri-apps/api/tauri",
]);

export default {
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow Tauri invoke() outside src/services — use the typed service layer (ADR-001)",
      category: "Best Practices",
      recommended: true,
    },
    fixable: null,
    schema: [
      {
        type: "object",
        properties: {
          allowedPaths: {
            type: "array",
            items: { type: "string" },
            description: "Glob prefixes for files allowed to use invoke directly",
          },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      noRawInvoke:
        "invoke() is only allowed in src/services/** (ADR-001). Move this call into a typed service function.",
    },
  },

  create(context) {
    const options = context.options[0] || {};
    const allowedPaths = options.allowedPaths || ["src/services/", "src-tauri/"];

    const filename = context.getFilename();
    const cwd = process.cwd();
    let relativePath = filename;
    if (filename.startsWith(cwd)) {
      relativePath = filename.slice(cwd.length + 1);
    }
    relativePath = relativePath.replace(/\\/g, "/");

    const isAllowed = allowedPaths.some((p) => relativePath.startsWith(p));
    if (isAllowed) return {};

    // Track imported bindings that refer to Tauri's invoke, under any local name.
    const invokeBindings = new Set();

    return {
      ImportDeclaration(node) {
        if (!TAURI_SOURCES.has(node.source.value)) return;
        for (const spec of node.specifiers) {
          if (spec.type === "ImportSpecifier" && spec.imported.name === "invoke") {
            invokeBindings.add(spec.local.name);
          }
        }
      },

      CallExpression(node) {
        // invoke(...)  — direct identifier call
        if (node.callee.type === "Identifier" && invokeBindings.has(node.callee.name)) {
          context.report({ node, messageId: "noRawInvoke" });
          return;
        }
        // Also catch `window.__TAURI__....invoke(...)`-style access patterns if
        // they become relevant; left out for now to keep the rule focused.
      },
    };
  },
};
