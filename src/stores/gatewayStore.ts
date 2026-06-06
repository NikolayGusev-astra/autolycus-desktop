import { create } from "zustand";
import type { AgentClient } from "../lib/agent-client";
import type { AgentStatus, Session, Message, AgentEvent } from "../lib/types";

const MAX_EVENTS = 1000;

interface GatewayState {
  client: AgentClient | null;
  connected: boolean;
  port: number | null;
  error: string | null;
  mode: "local" | "remote";
  agentStatus: AgentStatus;
  sessions: Session[];
  currentSessionId: string | null;
  messages: Message[];
  events: AgentEvent[];

  setClient: (client: AgentClient | null) => void;
  setConnected: (connected: boolean) => void;
  setPort: (port: number | null) => void;
  setError: (error: string | null) => void;
  setMode: (mode: "local" | "remote") => void;
  setAgentStatus: (status: AgentStatus) => void;
  addMessage: (message: Message) => void;
  addEvent: (event: AgentEvent) => void;
  setCurrentSession: (id: string | null) => void;
  setSessions: (sessions: Session[]) => void;
  reset: () => void;
}

export const useGatewayStore = create<GatewayState>()((set) => ({
  client: null,
  connected: false,
  port: null,
  error: null,
  mode: "local",
  agentStatus: "idle",
  sessions: [],
  currentSessionId: null,
  messages: [],
  events: [],

  setClient: (client: AgentClient | null) => set({ client }),
  setConnected: (connected: boolean) => set({ connected }),
  setPort: (port: number | null) => set({ port }),
  setError: (error: string | null) => set({ error }),
  setMode: (mode: "local" | "remote") => set({ mode }),
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

  reset: () =>
    set({
      connected: false,
      port: null,
      error: null,
      agentStatus: "idle" as AgentStatus,
      messages: [],
      events: [],
      currentSessionId: null,
    }),
}));

export type { AgentClient };
