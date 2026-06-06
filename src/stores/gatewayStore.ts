import { create } from "zustand";
import type { AgentClient } from "../lib/agent-client";
import type { AgentStatus, Session, Message, AgentEvent } from "../lib/types";

interface GatewayState {
  client: AgentClient | null;
  connected: boolean;
  error: string | null;
  agentStatus: AgentStatus;
  sessions: Session[];
  currentSessionId: string | null;
  messages: Message[];
  events: AgentEvent[];
}

export const useGatewayStore = create<GatewayState>()(() => ({
  client: null,
  connected: false,
  error: null,
  agentStatus: "idle",
  sessions: [],
  currentSessionId: null,
  messages: [],
  events: [],
}));
