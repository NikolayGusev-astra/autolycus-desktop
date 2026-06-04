import { memo } from "react";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { ThinkingBlock } from "./ThinkingBlock";
import { ToolOutput } from "./ToolOutput";
import { StreamingText } from "./StreamingText";
import type { Message } from "../../lib/types";

interface MessageBubbleProps {
  message: Message;
}

export const MessageBubble = memo(function MessageBubble({
  message,
}: MessageBubbleProps) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end mb-4">
        <div className="bg-blue-600 text-white rounded-2xl rounded-br-md px-4 py-2 max-w-[80%] shadow-sm">
          <div className="text-sm whitespace-pre-wrap">{message.content}</div>
        </div>
      </div>
    );
  }

  if (message.role === "assistant") {
    return (
      <div className="flex justify-start mb-4">
        <div className="bg-gray-100 dark:bg-gray-800 rounded-2xl rounded-bl-md px-4 py-2 max-w-[80%] shadow-sm">
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
    return (
      <div className="flex justify-start mb-2">
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
    );
  }

  return (
    <div className="text-center text-gray-400 text-sm my-2">
      {message.content}
    </div>
  );
});
