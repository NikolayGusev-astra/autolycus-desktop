export type MessageRole = "user" | "assistant" | "tool" | "system";

export interface Message {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  thinking?: string;
  tools?: ToolCall[];
  isStreaming?: boolean;
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

export type GatewayEvent = {
  type: "tool_start" | "tool_end" | "streaming" | "streaming_end" | "status" | "error" | "thinking";
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
