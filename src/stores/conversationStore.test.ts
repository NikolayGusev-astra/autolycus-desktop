import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { productConversationService } from "@/services/productConversation";
import { NO_MESSAGES, selectProductMessages, useConversationStore } from "./conversationStore";

describe("conversationStore", () => {
  beforeEach(() => {
    useConversationStore.setState({
      currentConversationId: null,
      messagesByConversation: new Map(),
      activeStreamIds: new Map(),
      pendingInteractions: new Map(),
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns the stable empty messages reference for conversations without messages", () => {
    const { getMessages } = useConversationStore.getState();

    expect(getMessages("missing")).toBe(NO_MESSAGES);
    expect(getMessages("missing")).toBe(getMessages("missing"));
  });

  it("keeps the MessageList null-conversation selector stable", () => {
    const messagesByConversation = new Map();

    expect(selectProductMessages(messagesByConversation, null)).toBe(NO_MESSAGES);
    expect(selectProductMessages(messagesByConversation, null)).toBe(
      selectProductMessages(messagesByConversation, null),
    );
  });

  it("returns messages stored for the selected conversation", () => {
    const messages = [{ id: "message-1", role: "assistant" as const, content: "Hello" }];
    useConversationStore.setState({ messagesByConversation: new Map([["conversation-1", messages]]) });

    expect(useConversationStore.getState().getMessages("conversation-1")).toBe(messages);
  });

  it("ignores malformed product events", () => {
    const { handleProductEvent } = useConversationStore.getState();

    expect(() => handleProductEvent(null)).not.toThrow();
    expect(() => handleProductEvent({ type: "Error" })).not.toThrow();
    expect(useConversationStore.getState().messagesByConversation).toEqual(new Map());
  });

  it("processes message and error events for their conversation", () => {
    const { handleProductEvent } = useConversationStore.getState();

    handleProductEvent({ type: "MessageDelta", conversation_id: "conversation-1", text: "Hello" });
    handleProductEvent({ type: "Error", conversation_id: "conversation-1", message: "Backend failed" });

    expect(useConversationStore.getState().getMessages("conversation-1")).toMatchObject([
      { role: "assistant", content: "Hello" },
      { role: "status", content: "Backend failed" },
    ]);
  });

  it("handles every product event type", () => {
    useConversationStore.setState({ currentConversationId: "conversation-1" });
    const { handleProductEvent } = useConversationStore.getState();

    handleProductEvent({ type: "MessageDelta", conversation_id: "conversation-1", text: "Hello" });
    handleProductEvent({ type: "Reasoning", conversation_id: "conversation-1", text: "reason" });
    handleProductEvent({ type: "Thinking", conversation_id: "conversation-1", text: "ing" });
    handleProductEvent({ type: "ToolStarted", conversation_id: "conversation-1", text: "tool started" });
    handleProductEvent({ type: "ToolCompleted", conversation_id: "conversation-1", result: "tool completed" });
    handleProductEvent({ type: "StatusUpdate", conversation_id: "conversation-1", message: "working" });
    handleProductEvent({ type: "Progress", conversation_id: "conversation-1", text: "halfway" });
    handleProductEvent({ type: "Error", conversation_id: "conversation-1", message: "failed" });
    for (const [type, requestId] of [
      ["ApprovalRequired", "approval"],
      ["ClarificationRequired", "clarification"],
      ["SecretRequired", "secret"],
      ["PrivilegeRequired", "privilege"],
    ] as const) {
      handleProductEvent({ type, conversation_id: "conversation-1", request_id: requestId, choices: ["yes"] });
    }
    handleProductEvent({ type: "InteractionExpired", conversation_id: "conversation-1", request_id: "approval" });
    handleProductEvent({ type: "MessageCompleted", conversation_id: "conversation-1" });

    const state = useConversationStore.getState();
    expect(state.getMessages("conversation-1")).toMatchObject([
      { role: "assistant", content: "Hello", thinking: "reasoning" },
      { role: "tool", content: "tool started" },
      { role: "tool", content: "tool completed" },
      { role: "status", content: "working" },
      { role: "status", content: "halfway" },
      { role: "status", content: "failed" },
    ]);
    expect([...state.pendingInteractions.values()].map(({ kind }) => kind)).toEqual([
      "clarification",
      "secret",
      "privilege",
    ]);
    expect(state.activeStreamIds.has("conversation-1")).toBe(false);
  });

  it("ignores events for inactive conversations", () => {
    useConversationStore.setState({ currentConversationId: "active" });

    useConversationStore.getState().handleProductEvent({
      type: "MessageDelta",
      conversation_id: "inactive",
      text: "must be ignored",
    });

    expect(useConversationStore.getState().messagesByConversation).toEqual(new Map());
  });

  it("creates one UUID-backed stream and appends later deltas", () => {
    vi.spyOn(crypto, "randomUUID").mockReturnValue("stream-uuid");
    useConversationStore.setState({ currentConversationId: "conversation-1" });
    const { handleProductEvent } = useConversationStore.getState();

    handleProductEvent({ type: "MessageDelta", conversation_id: "conversation-1", text: "Hel" });
    handleProductEvent({ type: "MessageDelta", conversation_id: "conversation-1", text: "lo" });

    expect(useConversationStore.getState().getMessages("conversation-1")).toEqual([
      { id: "stream-uuid", role: "assistant", content: "Hello" },
    ]);
    expect(crypto.randomUUID).toHaveBeenCalledTimes(1);
  });

  it("finalizes a stream when MessageCompleted arrives", () => {
    useConversationStore.setState({
      currentConversationId: "conversation-1",
      activeStreamIds: new Map([["conversation-1", "stream-uuid"]]),
    });

    useConversationStore.getState().handleProductEvent({ type: "MessageCompleted", conversation_id: "conversation-1" });

    expect(useConversationStore.getState().activeStreamIds.has("conversation-1")).toBe(false);
  });

  it("cancels the interaction through the service and removes its pending entry", async () => {
    const cancelInteraction = vi
      .spyOn(productConversationService, "cancelInteraction")
      .mockResolvedValue("cancelled");
    useConversationStore.setState({ currentConversationId: "conversation-1" });
    useConversationStore.getState().handleProductEvent({
      type: "ApprovalRequired",
      conversation_id: "conversation-1",
      request_id: "request-1",
    });

    await useConversationStore.getState().cancelInteraction("conversation-1", "request-1", "approval");

    expect(cancelInteraction).toHaveBeenCalledWith("conversation-1", "request-1", "approval");
    expect(useConversationStore.getState().pendingInteractions.size).toBe(0);
  });
});
