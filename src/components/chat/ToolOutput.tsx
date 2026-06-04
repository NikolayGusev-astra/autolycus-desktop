import { useState } from "react";
import { ChevronRight, Terminal, CheckCircle, XCircle, Loader } from "lucide-react";
import { MarkdownRenderer } from "./MarkdownRenderer";
import type { ToolCall } from "../../lib/types";

interface ToolOutputProps {
  tool: ToolCall;
}

export function ToolOutput({ tool }: ToolOutputProps) {
  const [expanded, setExpanded] = useState(tool.status === "running");

  const statusIcon = {
    running: <Loader className="w-4 h-4 animate-spin text-blue-500" />,
    completed: <CheckCircle className="w-4 h-4 text-green-500" />,
    error: <XCircle className="w-4 h-4 text-red-500" />,
  }[tool.status];

  return (
    <div className="mb-2 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 px-3 py-1.5 w-full text-left hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        <ChevronRight
          className={`w-4 h-4 transition-transform ${expanded ? "rotate-90" : ""}`}
        />
        <Terminal className="w-4 h-4 text-gray-400" />
        <span className="text-sm font-mono font-medium">{tool.name}</span>
        {statusIcon}
        {tool.durationMs && (
          <span className="text-xs text-gray-400 ml-auto">{tool.durationMs}ms</span>
        )}
      </button>
      {expanded && (
        <div className="border-t border-gray-200 dark:border-gray-700">
          {tool.input && (
            <div className="px-3 py-2 bg-gray-50 dark:bg-gray-900/50">
              <div className="text-xs text-gray-400 mb-1">Input</div>
              <pre className="text-xs font-mono overflow-x-auto whitespace-pre-wrap">
                {tool.input}
              </pre>
            </div>
          )}
          {tool.output && (
            <div className="px-3 py-2">
              <div className="text-xs text-gray-400 mb-1">Output</div>
              <MarkdownRenderer content={tool.output} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
