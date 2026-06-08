// src/stores/gatewayStore.ts
import { create } from "zustand";
import type { AgentClient } from "../lib/agent-client";
import type {
  AgentStatus,
  Session,
  Message,
  AgentEvent,
  ApprovalRequest,
  PipelineStatus,
} from "../lib/types";

const MAX_EVENTS = 1000;

interface GatewayState {
  client: AgentClient | null;
  connected: boolean;
  error: string | null;
  agentStatus: AgentStatus;
  sessions: Session[];
  currentSessionId: string | null;
  messages: Message[];
  events: AgentEvent[];

  // v0.4.0 new state
  pendingApproval: ApprovalRequest | null;
  pipelineStatus: PipelineStatus;
  hermesHome: string | null;
  gatewayVersion: string | null;

  setClient: (client: AgentClient | null) => void;
  setConnected: (connected: boolean) => void;
  setError: (error: string | null) => void;
  setAgentStatus: (status: AgentStatus) => void;
  addMessage: (message: Message) => void;
  addEvent: (event: AgentEvent) => void;
  setCurrentSession: (id: string | null) => void;
  setSessions: (sessions: Session[]) => void;
  reset: () => void;

  // v0.4.0 new actions
  setPendingApproval: (approval: ApprovalRequest | null) => void;
  setPipelineStatus: (status: PipelineStatus) => void;
  setHermesHome: (home: string | null) => void;
  setGatewayVersion: (version: string | null) => void;
}

const INITIAL_PIPELINE_STATUS: PipelineStatus = {
  backend: "disconnected",
  model: undefined,
  tokensUsed: undefined,
  tokensLimit: undefined,
  costUsd: undefined,
};

export const useGatewayStore = create<GatewayState>()((set) => ({
  client: null,
  connected: false,
  error: null,
  agentStatus: "idle",
  sessions: [],
  currentSessionId: null,
  messages: [],
  events: [],

  pendingApproval: null,
  pipelineStatus: INITIAL_PIPELINE_STATUS,
  hermesHome: null,
  gatewayVersion: null,

  setClient: (client: AgentClient | null) => set({ client }),
  setConnected: (connected: boolean) => set({ connected }),
  setError: (error: string | null) => set({ error }),
  setAgentStatus: (agentStatus: AgentStatus) => set({ agentStatus }),

  addMessage: (message: Message) =>
    set((s) => ({ messages: [...s.messages, message] })),

  addEvent: (event: AgentEvent) =>
    set((s) => {
      const events = [...s.events, event];
      if (events.length > MAX_EVENTS) {
        events.splice(0, events.length - MAX_EVENTS);
      }
      return { events };
    }),

  setCurrentSession: (id: string | null) => set({ currentSessionId: id }),
  setSessions: (sessions: Session[]) => set({ sessions }),

  setPendingApproval: (approval: ApprovalRequest | null) =>
    set({ pendingApproval: approval }),

  setPipelineStatus: (status: PipelineStatus) =>
    set({ pipelineStatus: status }),

  setHermesHome: (home: string | null) => set({ hermesHome: home }),
  setGatewayVersion: (version: string | null) => set({ gatewayVersion: version }),

  reset: () =>
    set({
      connected: false,
      error: null,
      agentStatus: "idle" as AgentStatus,
      messages: [],
      events: [],
      currentSessionId: null,
      pendingApproval: null,
      pipelineStatus: INITIAL_PIPELINE_STATUS,
    }),
}));

export type { AgentClient };
