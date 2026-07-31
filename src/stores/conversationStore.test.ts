import { beforeEach, describe, expect, it } from "vitest";
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
});
