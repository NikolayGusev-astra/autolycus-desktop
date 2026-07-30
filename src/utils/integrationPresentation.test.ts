import { describe, expect, it } from "vitest";
import { issueMessage, statusDescription, statusLabel } from "./integrationPresentation";

describe("integration presentation", () => {
  const statuses = [
    ["ready", "Ready", "This integration is connected and ready to use."],
    ["disabled", "Disabled", "This integration is currently turned off."],
    ["needs_attention", "Needs attention", "This integration needs an update before it can be used."],
    ["unknown", "Unavailable", "This integration is unavailable right now."],
    ["configuring", "Configuring", "Setup is required before this integration can be used."],
  ] as const;
  it.each(statuses)("labels %s", (status, label) => expect(statusLabel(status)).toBe(label));
  it.each(statuses)("describes %s", (status, _label, description) => expect(statusDescription(status)).toBe(description));
  it.each(["authentication_required", "authentication_expired", "permission_denied", "network_unavailable", "service_unavailable", "configuration_invalid", "runtime_unavailable", "health_check_failed", "unknown"])("normalizes %s", (issue) => expect(issueMessage(issue)).not.toMatch(issue));
  it.each(["authentication_required", "authentication_expired", "permission_denied", "network_unavailable", "service_unavailable", "configuration_invalid", "runtime_unavailable", "health_check_failed", "unknown"])("has actionable copy for %s", (issue) => expect(issueMessage(issue).endsWith(".")).toBe(true));
  it.each(["ready", "disabled", "needs_attention", "unknown", "configuring"])("status output is nonempty for %s", (status) => expect(statusLabel(status).length).toBeGreaterThan(0));
  it.each(["ready", "disabled", "needs_attention", "unknown", "configuring"])("status description is nonempty for %s", (status) => expect(statusDescription(status).length).toBeGreaterThan(0));
  it("handles structured attention statuses", () => expect(statusLabel({ needs_attention: { reason: "network_unavailable" } })).toBe("Needs attention"));
  it("handles structured unavailable statuses", () => expect(statusLabel({ unsupported: { reason: "not supported" } })).toBe("Unavailable"));
});
