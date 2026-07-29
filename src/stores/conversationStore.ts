import { create } from "zustand";
import {
  productConversationService,
  type ProductConversationDto,
  type ProductEvent,
} from "@/services/productConversation";

export interface ConversationMessage {
  role: string;
  content: string;
  id: string;
}

interface ConversationState {
  conversations: ProductConversationDto[];
  currentConversationId: string | null;
  messages: Map<string, ConversationMessage[]>;
  loading: boolean;
  error: string | null;
  createConversation: (mode: string) => Promise<string>;
  sendMessage: (text: string) => Promise<void>;
  loadConversations: () => Promise<void>;
  setCurrentConversation: (id: string) => void;
  handleProductEvent: (event: ProductEvent) => void;
  abort: () => Promise<void>;
  respondApproval: (requestId: string, choice: string, all: boolean) => Promise<void>;
  respondClarification: (requestId: string, answer: string) => Promise<void>;
  respondSecret: (requestId: string, secret: string) => Promise<void>;
  respondSudo: (requestId: string, password: string) => Promise<void>;
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

function appendMessage(
  messages: Map<string, ConversationMessage[]>,
  conversationId: string,
  message: ConversationMessage,
): Map<string, ConversationMessage[]> {
  const next = new Map(messages);
  next.set(conversationId, [...(next.get(conversationId) ?? []), message]);
  return next;
}

function appendAssistantText(
  messages: Map<string, ConversationMessage[]>,
  conversationId: string,
  text: string,
): Map<string, ConversationMessage[]> {
  const existing = messages.get(conversationId) ?? [];
  const lastMessage = existing[existing.length - 1];
  const next = new Map(messages);

  if (lastMessage?.role === "assistant" && lastMessage.id === `stream-${conversationId}`) {
    next.set(conversationId, [
      ...existing.slice(0, -1),
      { ...lastMessage, content: lastMessage.content + text },
    ]);
    return next;
  }

  next.set(conversationId, [
    ...existing,
    { id: `stream-${conversationId}`, role: "assistant", content: text },
  ]);
  return next;
}

export const useConversationStore = create<ConversationState>()((set, get) => ({
  conversations: [],
  currentConversationId: null,
  messages: new Map(),
  loading: false,
  error: null,

  createConversation: async (mode) => {
    set({ loading: true, error: null });
    try {
      const id = await productConversationService.createConversation(mode);
      set((state) => ({
        currentConversationId: id,
        messages: state.messages.has(id) ? state.messages : new Map(state.messages).set(id, []),
      }));
      await get().loadConversations();
      return id;
    } catch (error) {
      const message = String(error);
      set({ error: message });
      throw error;
    } finally {
      set({ loading: false });
    }
  },

  sendMessage: async (text) => {
    const conversationId = get().currentConversationId;
    if (!conversationId) {
      const error = new Error("No active product conversation");
      set({ error: error.message });
      throw error;
    }

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
      const conversations = await productConversationService.getConversations();
      set({ conversations });
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

    switch (event.type) {
      case "MessageDelta":
        set((state) => ({ messages: appendAssistantText(state.messages, conversationId, eventText(event)) }));
        break;
      case "Reasoning":
      case "Thinking":
      case "ToolStarted":
      case "ToolCompleted":
      case "Error":
      case "StatusUpdate":
      case "Progress":
        if (eventText(event)) {
          set((state) => ({
            messages: appendMessage(state.messages, conversationId, {
              id: crypto.randomUUID(),
              role: "assistant",
              content: eventText(event),
            }),
          }));
        }
        break;
      case "MessageCompleted":
      case "ApprovalRequired":
      case "ClarificationRequired":
      case "SecretRequired":
      case "PrivilegeRequired":
      case "InteractionExpired":
        break;
    }
  },

  abort: async () => {
    const conversationId = get().currentConversationId;
    if (!conversationId) return;
    await productConversationService.abortConversation(conversationId);
  },

  respondApproval: async (requestId, choice, all) => {
    const conversationId = get().currentConversationId;
    if (!conversationId) return;
    await productConversationService.respondApproval(conversationId, requestId, choice, all);
  },

  respondClarification: async (requestId, answer) => {
    const conversationId = get().currentConversationId;
    if (!conversationId) return;
    await productConversationService.respondClarification(conversationId, requestId, answer);
  },

  respondSecret: async (requestId, secret) => {
    const conversationId = get().currentConversationId;
    if (!conversationId) return;
    await productConversationService.respondSecret(conversationId, requestId, secret);
  },

  respondSudo: async (requestId, password) => {
    const conversationId = get().currentConversationId;
    if (!conversationId) return;
    await productConversationService.respondSudo(conversationId, requestId, password);
  },
}));
