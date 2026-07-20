export type MessageRole = "user" | "assistant" | "tool" | "system";

export interface Message {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  thinking?: string;
  tools?: ToolCall[];
  isStreaming?: boolean;
  /** Attachments sent with the message (images, voice clips, files). */
  attachments?: MessageAttachment[];
}

export interface MessageAttachment {
  kind: "image" | "audio" | "video" | "file" | "url";
  path?: string;
  name: string;
  mime?: string;
  /** Resolved data URL for inline preview (filled lazily). */
  dataUrl?: string;
}

export interface ToolCall {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: "running" | "completed" | "error";
  startedAt?: number;
  durationMs?: number;
}

export interface Session {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
  lastMessage?: string;
  model?: string;
}

export type AgentStatus = "idle" | "thinking" | "streaming" | "tool_calling" | "error";

export interface Usage {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  costUsd?: number;
}

// ── v0.3.0 new types ──

export interface ToolResult {
  tool_call_id: string;
  name: string;
  input: string;
  output: string;
  durationMs: number;
  status: "ok" | "error";
}

export type CommandClass = "read" | "write" | "network" | "install" | "destructive";

export interface ApprovalRequest {
  requestId: string;
  toolName: string;
  toolInput: string;
  action: string;
  commandClass: CommandClass;
}

export interface ApprovalDecision {
  requestId: string;
  decision: "approved" | "denied" | "approved_always";
}

// ── Profile types ─────────────────────────────────────────────────────────

export interface ProfileInfo {
  name: string;
  path: string;
  is_default: boolean;
  is_active: boolean;
  model: string;
  provider: string;
  has_env: boolean;
  has_soul: boolean;
  skill_count: number;
  gateway_running: boolean;
}

export interface ProxySettings {
  use_proxy: boolean;
  proxy_url: string;
}

export interface ModelConfig {
  provider: string;
  model: string;
  base_url: string;
  proxy?: ProxySettings;
}

export interface PipelineStatus {
  backend: "connected" | "disconnected" | "error";
  model?: string;
  tokensUsed?: number;
  tokensLimit?: number;
  costUsd?: number;
}

export type GatewayEvent = {
  type: "tool_start" | "tool_end" | "streaming" | "streaming_end" | "status" | "error" | "thinking" | "tool_result" | "approval_request" | "approval_decision" | "pipeline_status";
  [key: string]: unknown;
};

// ── Tauri IPC types ───────────────────────────────────────────────────

export interface AgentConfig {
  mode: "local" | "remote";
  python_path?: string;
  script_path?: string;
  remote_host?: string;
  remote_port?: number;
}

export interface AgentEvent {
  event_type: string;
  payload: Record<string, unknown>;
  session_id?: string;
}

// ── ADR-009: Cross-linking types (external_refs + session_links) ─────────

export interface ExternalRef {
  id: number;
  source: "jira" | "email" | "confluence";
  external_id: string;
  external_url: string;
  title: string;
  task_id: number | null;
  project_id: number | null;
  goal_id: number | null;
  created_at: number | null;
}

export interface SessionLink {
  id: number;
  session_id: string;
  task_id: number | null;
  project_id: number | null;
  goal_id: number | null;
  linked_at: number | null;
  linked_by: string;
  note: string;
}

export interface CreateTaskFromExternalInput {
  source: "jira" | "email" | "confluence";
  external_id: string;
  external_url?: string;
  title: string;
  priority: number;
  due_date?: string;
  project_id?: number;
  goal_id?: number;
  assignee: string;
}

export interface LinkSessionInput {
  session_id: string;
  task_id?: number;
  project_id?: number;
  goal_id?: number;
  linked_by?: "manual" | "agent";
  note?: string;
}

// ── Tauri IPC request/response types ────────────────────────────────────────

export interface SendMessageRequest {
  text: string;
  session_id?: string;
  history?: Array<{ role: string; content: string }>;
  [key: string]: unknown;
}

export interface SshConfig {
  host: string;
  port: number;
  username: string;
  key_path: string;
  remote_port: number;
  local_port: number;
  [key: string]: unknown;
}

export interface ConnectionConfig {
  connectionMode: "local" | "remote" | "ssh";
  remoteUrl: string;
  remoteApiKey: string;
  ssh: SshConfig;
  [key: string]: unknown;
}
