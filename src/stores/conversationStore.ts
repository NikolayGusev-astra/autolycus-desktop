import { create } from "zustand";
import {
  productConversationService,
  type ProductConversationDto,
  type ProductEvent,
} from "@/services/productConversation";

export interface ConversationMessage {
  id: string;
  role: "user" | "assistant" | "tool" | "status";
  content: string;
  thinking?: string;
}

export type InteractionKind = "approval" | "clarification" | "secret" | "privilege";

export interface PendingInteraction {
  conversationId: string;
  requestId: string;
  kind: InteractionKind;
  /** The complete gateway event, preserved for interaction-specific UI. */
  payload: ProductEvent;
  choices: string[];
}

interface ConversationState {
  conversations: ProductConversationDto[];
  currentConversationId: string | null;
  messages: Map<string, ConversationMessage[]>;
  activeStreamIds: Map<string, string>;
  /** Keyed by a stable (conversationId, requestId) tuple. */
  pendingInteractions: Map<string, PendingInteraction>;
  loading: boolean;
  error: string | null;
  createConversation: () => Promise<string>;
  sendMessage: (conversationId: string, text: string) => Promise<void>;
  loadConversations: () => Promise<void>;
  setCurrentConversation: (id: string) => void;
  handleProductEvent: (event: ProductEvent) => void;
  removePendingInteraction: (conversationId: string, requestId: string) => void;
  abort: () => Promise<void>;
  respondApproval: (conversationId: string, requestId: string, choice: string, all: boolean) => Promise<void>;
  respondClarification: (conversationId: string, requestId: string, answer: string) => Promise<void>;
  respondSecret: (conversationId: string, requestId: string, secret: string) => Promise<void>;
  respondSudo: (conversationId: string, requestId: string, password: string) => Promise<void>;
}

function eventText(event: ProductEvent): string {
  return typeof event.text === "string"
    ? event.text
    : typeof event.message === "string"
      ? event.message
      : typeof event.result === "string"
        ? event.result
        : "";
}

function appendMessage(messages: Map<string, ConversationMessage[]>, conversationId: string, message: ConversationMessage) {
  const next = new Map(messages);
  next.set(conversationId, [...(next.get(conversationId) ?? []), message]);
  return next;
}

function appendAssistantContent(messages: Map<string, ConversationMessage[]>, conversationId: string, streamId: string, content: string) {
  const existing = messages.get(conversationId) ?? [];
  const messageIndex = existing.findIndex((message) => message.id === streamId);
  if (messageIndex < 0) return appendMessage(messages, conversationId, { id: streamId, role: "assistant", content });

  const next = new Map(messages);
  next.set(conversationId, existing.map((message, index) =>
    index === messageIndex ? { ...message, content: message.content + content } : message,
  ));
  return next;
}

function appendThinking(messages: Map<string, ConversationMessage[]>, conversationId: string, streamId: string, thinking: string) {
  const existing = messages.get(conversationId) ?? [];
  const messageIndex = existing.findIndex((message) => message.id === streamId);
  if (messageIndex < 0) {
    return appendMessage(messages, conversationId, { id: streamId, role: "assistant", content: "", thinking });
  }

  const next = new Map(messages);
  next.set(conversationId, existing.map((message, index) =>
    index === messageIndex ? { ...message, thinking: (message.thinking ?? "") + thinking } : message,
  ));
  return next;
}

function getOrCreateStreamId(activeStreamIds: Map<string, string>, conversationId: string) {
  const streamId = activeStreamIds.get(conversationId) ?? crypto.randomUUID();
  activeStreamIds.set(conversationId, streamId);
  return streamId;
}

function interactionKey(conversationId: string, requestId: string): string {
  return JSON.stringify([conversationId, requestId]);
}

function interactionFromEvent(event: ProductEvent): PendingInteraction | null {
  const requestId = typeof event.request_id === "string" ? event.request_id : null;
  if (!requestId) return null;

  const kindByEvent: Partial<Record<ProductEvent["type"], InteractionKind>> = {
    ApprovalRequired: "approval",
    ClarificationRequired: "clarification",
    SecretRequired: "secret",
    PrivilegeRequired: "privilege",
  };
  const kind = kindByEvent[event.type];
  if (!kind) return null;

  const choices = Array.isArray(event.choices)
    ? event.choices.filter((choice): choice is string => typeof choice === "string")
    : [];
  return { conversationId: event.conversation_id, requestId, kind, payload: event, choices };
}

export const useConversationStore = create<ConversationState>()((set, get) => ({
  conversations: [],
  currentConversationId: null,
  messages: new Map(),
  activeStreamIds: new Map(),
  pendingInteractions: new Map(),
  loading: false,
  error: null,

  createConversation: async () => {
    set({ loading: true, error: null });
    try {
      const id = await productConversationService.createConversation();
      set((state) => ({
        currentConversationId: id,
        messages: state.messages.has(id) ? state.messages : new Map(state.messages).set(id, []),
      }));
      await get().loadConversations();
      return id;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    } finally {
      set({ loading: false });
    }
  },

  sendMessage: async (conversationId, text) => {
    set((state) => ({
      error: null,
      messages: appendMessage(state.messages, conversationId, {
        id: crypto.randomUUID(),
        role: "user",
        content: text,
      }),
    }));

    try {
      await productConversationService.sendMessage(conversationId, text);
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  loadConversations: async () => {
    set({ loading: true, error: null });
    try {
      set({ conversations: await productConversationService.getConversations() });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    } finally {
      set({ loading: false });
    }
  },

  setCurrentConversation: (id) => set({ currentConversationId: id }),

  handleProductEvent: (event) => {
    const conversationId = event.conversation_id || get().currentConversationId;
    if (!conversationId) return;
    const text = eventText(event);

    switch (event.type) {
      case "MessageDelta":
        set((state) => {
          const activeStreamIds = new Map(state.activeStreamIds);
          const streamId = getOrCreateStreamId(activeStreamIds, conversationId);
          return { activeStreamIds, messages: appendAssistantContent(state.messages, conversationId, streamId, text) };
        });
        break;
      case "Reasoning":
      case "Thinking":
        set((state) => {
          const activeStreamIds = new Map(state.activeStreamIds);
          const streamId = getOrCreateStreamId(activeStreamIds, conversationId);
          return { activeStreamIds, messages: appendThinking(state.messages, conversationId, streamId, text) };
        });
        break;
      case "MessageCompleted":
        set((state) => {
          const activeStreamIds = new Map(state.activeStreamIds);
          activeStreamIds.delete(conversationId);
          return { activeStreamIds };
        });
        break;
      case "ToolStarted":
      case "ToolCompleted":
        if (text) set((state) => ({ messages: appendMessage(state.messages, conversationId, { id: crypto.randomUUID(), role: "tool", content: text }) }));
        break;
      case "StatusUpdate":
      case "Progress":
      case "Error":
        if (text) set((state) => ({ messages: appendMessage(state.messages, conversationId, { id: crypto.randomUUID(), role: "status", content: text }) }));
        break;
      case "ApprovalRequired":
      case "ClarificationRequired":
      case "SecretRequired":
      case "PrivilegeRequired": {
        const interaction = interactionFromEvent(event);
        if (interaction) set((state) => ({
          pendingInteractions: new Map(state.pendingInteractions).set(
            interactionKey(interaction.conversationId, interaction.requestId),
            interaction,
          ),
        }));
        break;
      }
      case "InteractionExpired": {
        const requestId = typeof event.request_id === "string" ? event.request_id : null;
        if (requestId) set((state) => {
          const pendingInteractions = new Map(state.pendingInteractions);
          pendingInteractions.delete(interactionKey(conversationId, requestId));
          return { pendingInteractions };
        });
        break;
      }
    }
  },

  removePendingInteraction: (conversationId, requestId) => set((state) => {
    const pendingInteractions = new Map(state.pendingInteractions);
    pendingInteractions.delete(interactionKey(conversationId, requestId));
    return { pendingInteractions };
  }),

  abort: async () => {
    const conversationId = get().currentConversationId;
    if (conversationId) await productConversationService.abortConversation(conversationId);
  },
  respondApproval: async (conversationId, requestId, choice, all) => {
    await productConversationService.respondApproval(conversationId, requestId, choice, all);
  },
  respondClarification: async (conversationId, requestId, answer) => {
    await productConversationService.respondClarification(conversationId, requestId, answer);
  },
  respondSecret: async (conversationId, requestId, secret) => {
    await productConversationService.respondSecret(conversationId, requestId, secret);
  },
  respondSudo: async (conversationId, requestId, password) => {
    await productConversationService.respondSudo(conversationId, requestId, password);
  },
}));
