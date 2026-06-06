import { create } from "zustand";
import type { GatewayClient } from "../lib/gateway-client";
import type { AgentStatus, Session, Message, GatewayEvent } from "../lib/types";

interface GatewayState {
  client: GatewayClient | null;
  connected: boolean;
  port: number | null;
  error: string | null;
  agentStatus: AgentStatus;
  sessions: Session[];
  currentSessionId: string | null;
  messages: Message[];
  events: GatewayEvent[];
}

export const useGatewayStore = create<GatewayState>(() => ({
  client: null,
  connected: false,
  port: null,
  error: null,
  agentStatus: "idle",
  sessions: [],
  currentSessionId: null,
  messages: [],
  events: [],
}));
