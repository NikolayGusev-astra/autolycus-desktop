import type { CapabilityResultProvenance } from "@/services/capabilityRouter";

export function ProvenanceBadge({ provenance }: { provenance: CapabilityResultProvenance }) {
  return <span className="inline-flex rounded bg-ac-surface px-1.5 py-0.5 text-xs text-ac-muted">Source: {provenance.integration_label}</span>;
}
