/** Typed IPC boundary for curated assistant capabilities. */
import { invoke } from "@tauri-apps/api/core";

export type CapabilityRisk = "read_only" | "sensitive_read" | "external_mutation" | "privileged";
export type CapabilityInvocationMode = "search" | "read";

export interface CapabilityDefinition {
  id: string;
  description: string;
  input_schema: Record<string, unknown>;
  risk: CapabilityRisk;
  invocation_mode: CapabilityInvocationMode;
}

export interface CapabilityRoute { instance_id: string; integration_label: string; capability_id: string; revision: string; }
export interface CapabilityRouteChoice { instance_id: string; label: string; description: string; }
export interface CapabilityClarification { request_id: string; prompt: string; choices: CapabilityRouteChoice[]; }
export type CapabilityRoutingOutcome =
  | { kind: "selected"; route: CapabilityRoute }
  | { kind: "clarification_required"; clarification: CapabilityClarification }
  | { kind: "unavailable" };
export interface CapabilityRouteClarificationEvent {
  conversation_id: string;
  capability_id: string;
  clarification: CapabilityClarification;
}
export interface CapabilityResultProvenance {
  instance_id: string; integration_label: string; capability_id: string; retrieved_at: string;
}

export const capabilityRouter = {
  listCapabilities: (): Promise<CapabilityDefinition[]> => invoke("list_assistant_capabilities_cmd"),
  resolveRoute: (request: { conversation_id: string; capability_id: string; input: { values: Record<string, unknown> } }): Promise<CapabilityRoutingOutcome> =>
    invoke("resolve_capability_route_cmd", { request }),
  submitRouteChoice: (request: { conversation_id: string; capability_id: string; instance_id: string }): Promise<void> =>
    invoke("submit_capability_route_choice_cmd", { request }),
  clearPreference: (request: { conversation_id: string; capability_id: string }): Promise<void> =>
    invoke("clear_capability_route_preference_cmd", { request }),
};
