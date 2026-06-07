import { memo } from "react";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { ThinkingBlock } from "./ThinkingBlock";
import { ToolOutput } from "./ToolOutput";
import { ToolResultCard } from "./ToolResult";
import { StreamingText } from "./StreamingText";
import type { Message } from "../../lib/types";

interface MessageBubbleProps {
  message: Message;
}

/** Check if message has full ToolResult data (v0.3.0+) vs legacy tool_progress */
function hasToolResult(message: Message): boolean {
  if (message.role !== "tool") return false;
  const msg = message as Message & { toolResult?: unknown };
  return !!(msg.toolResult && typeof msg.toolResult === "object");
}

export const MessageBubble = memo(function MessageBubble({
  message,
}: MessageBubbleProps) {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";

  if (isUser) {
    return (
      <div className="ac-msg ac-msg-user">
        <div className="ac-msg-avatar ac-msg-avatar-user">Я</div>
        <div className="ac-msg-body ac-msg-body-user">{message.content}</div>
      </div>
    );
  }

  if (isAssistant) {
    return (
      <div className="ac-msg">
        <div className="ac-msg-avatar ac-msg-avatar-assistant">A</div>
        <div className="ac-msg-body ac-msg-body-assistant">
          {message.thinking && <ThinkingBlock content={message.thinking} />}
          {message.isStreaming ? (
            <StreamingText content={message.content} />
          ) : (
            <MarkdownRenderer content={message.content} />
          )}
          {message.tools?.map((tool) => (
            <ToolOutput key={tool.id} tool={tool} />
          ))}
        </div>
      </div>
    );
  }

  if (message.role === "tool") {
    // v0.3.0+ structured tool result
    if (hasToolResult(message)) {
      const msg = message as Message & { toolResult: any };
      return (
        <div className="ac-msg">
          <div className="ac-msg-avatar ac-msg-avatar-assistant">T</div>
          <div className="ac-msg-body ac-msg-body-assistant">
            <ToolResultCard
              result={{
                tool_call_id: msg.toolResult.tool_call_id || message.id,
                name: msg.toolResult.name || "tool",
                input: msg.toolResult.input || "",
                output: msg.toolResult.output || message.content,
                durationMs: msg.toolResult.durationMs || 0,
                status: msg.toolResult.status || "ok",
              }}
            />
          </div>
        </div>
      );
    }

    // Legacy tool_progress fallback
    return (
      <div className="ac-msg">
        <div className="ac-msg-avatar ac-msg-avatar-assistant">T</div>
        <div className="ac-msg-body ac-msg-body-assistant">
          <ToolOutput
            tool={{
              id: message.id,
              name: "tool",
              input: "",
              output: message.content,
              status: "completed",
            }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="text-center text-ac-stone text-xs my-3">
      {message.content}
    </div>
  );
});
