/** Typed IPC boundary for integrations. Keep raw Tauri calls in this module. */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type SetupValue =
  | { kind: "text"; value: string }
  | { kind: "boolean"; value: boolean }
  | { kind: "number"; value: number }
  | { kind: "secret"; value: string };

export type IntegrationView =
  | { kind: "user"; data: UserIntegration }
  | { kind: "admin"; data: AdminIntegration };

export interface UserIntegration {
  id: string; definition_id: string; display_name: string; description: string;
  category: string; status: string; enabled: boolean; capabilities: Capability[];
}
export interface AdminIntegration {
  id: string; definition_id: string; display_name: string; status: IntegrationStatus;
  management: string; setup_schema: SetupField[];
  configured_fields: Array<{ key: string; configured: boolean; value: null }>;
  diagnostics: { last_health_check: string | null; error_count: number; last_error: string | null } | null;
}
export type IntegrationStatus =
  | "not_configured" | "configuring" | "connecting" | "ready" | "disabled"
  | { degraded: { reason: IntegrationIssue } }
  | { needs_attention: { reason: IntegrationIssue } }
  | { unsupported: { reason: string } };
export type IntegrationIssue = "authentication_required" | "authentication_expired" | "permission_denied" | "network_unavailable" | "service_unavailable" | "configuration_invalid" | "runtime_unavailable" | "health_check_failed" | "unknown";
export type IntegrationError =
  | { kind: "definition_not_found" } | { kind: "instance_not_found" }
  | { kind: "already_configured" } | { kind: "configuration_invalid" }
  | { kind: "authentication_required" } | { kind: "permission_denied" }
  | { kind: "runtime_unavailable" } | { kind: "health_check_failed" }
  | { kind: "secret_store_unavailable" } | { kind: "internal_error" };
export type IntegrationEvent =
  | { kind: "status_changed"; details: { status: string } }
  | { kind: "needs_attention"; details: { issue: IntegrationIssue } }
  | { kind: "ready"; details: null }
  | { kind: "removed"; details: null };
export interface IntegrationEventEnvelope { instance_id: string; revision: number; event: IntegrationEvent; }
export interface Capability { id: string; display_name: string; description: string; access: string; }
export interface SetupField { key: string; label: string; description: string | null; field_type: unknown; required: boolean; secret: boolean; default_value: string | null; validation: unknown; }
export interface IntegrationDefinition { id: string; display_name: string; description: string; category: string; icon: string; capabilities: Capability[]; setup_schema: SetupField[]; availability: unknown; }
export interface ConfigureIntegrationRequest { definition_id: string; instance_id?: string; display_name?: string; values: Record<string, SetupValue>; enabled_capabilities: string[]; }
export interface IntegrationTestResult { instance_id: string; status: string; issues: IntegrationIssue[]; tested_at: string; }
export interface RemoveIntegrationResult { instance_id: string; removed: boolean; }

const instanceRequest = (instanceId: string) => ({ request: { instance_id: instanceId } });

export const integrationService = {
  listAvailable: (): Promise<IntegrationDefinition[]> => invoke("list_available_integrations_cmd"),
  listConfigured: (): Promise<IntegrationView[]> => invoke("list_configured_integrations_cmd"),
  get: (instanceId: string): Promise<IntegrationView> => invoke("get_integration_cmd", instanceRequest(instanceId)),
  configure: (request: ConfigureIntegrationRequest): Promise<IntegrationView> => invoke("configure_integration_cmd", { request }),
  enable: (instanceId: string): Promise<IntegrationView> => invoke("enable_integration_cmd", instanceRequest(instanceId)),
  disable: (instanceId: string): Promise<IntegrationView> => invoke("disable_integration_cmd", instanceRequest(instanceId)),
  test: (instanceId: string): Promise<IntegrationTestResult> => invoke("test_integration_cmd", instanceRequest(instanceId)),
  remove: (instanceId: string): Promise<RemoveIntegrationResult> => invoke("remove_integration_cmd", instanceRequest(instanceId)),
  refreshStatus: (instanceId: string): Promise<IntegrationView> => invoke("refresh_integration_status_cmd", instanceRequest(instanceId)),
  subscribe: (handler: (event: IntegrationEventEnvelope) => void): Promise<UnlistenFn> =>
    listen<IntegrationEventEnvelope>("integration-event", ({ payload }) => handler(payload)),
};
