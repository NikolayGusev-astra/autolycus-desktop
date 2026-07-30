import type { IntegrationIssue, IntegrationStatus } from "@/services/integrationService";

export function statusLabel(status: string | IntegrationStatus): string {
  const value = typeof status === "string" ? status : Object.keys(status)[0] ?? "unknown";
  if (value === "ready") return "Ready";
  if (value === "disabled") return "Disabled";
  if (value === "configuring" || value === "connecting" || value === "not_configured") return "Configuring";
  if (value === "needs_attention" || value === "degraded") return "Needs attention";
  return "Unavailable";
}

export function statusDescription(status: string | IntegrationStatus): string {
  switch (statusLabel(status)) {
    case "Ready": return "This integration is connected and ready to use.";
    case "Disabled": return "This integration is currently turned off.";
    case "Configuring": return "Setup is required before this integration can be used.";
    case "Needs attention": return "This integration needs an update before it can be used.";
    default: return "This integration is unavailable right now.";
  }
}

export function issueMessage(issue: string | IntegrationIssue): string {
  const messages: Record<string, string> = {
    authentication_required: "Sign in details are required.", authentication_expired: "Sign in details have expired.",
    permission_denied: "Your account does not have permission for this integration.", network_unavailable: "A network connection is required.",
    service_unavailable: "The service is temporarily unavailable.", configuration_invalid: "Review the setup details and try again.",
    runtime_unavailable: "The integration service is unavailable.", health_check_failed: "The connection check did not succeed.", unknown: "An unexpected issue occurred.",
  };
  return messages[issue] ?? messages.unknown;
}

export function IntegrationStatusBadge({ status }: { status: string | IntegrationStatus }) {
  const label = statusLabel(status);
  return <span className="inline-flex rounded-full border border-ac-border bg-ac-surface-2 px-2 py-0.5 text-xs font-medium text-ac-ink" role="status" aria-label={`Status: ${label}`}>{label}</span>;
}
