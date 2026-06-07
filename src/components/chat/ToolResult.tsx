import { useState } from "react";
import {
  ChevronRight,
  Terminal,
  FileText,
  Globe,
  Search,
  Code,
  CheckCircle,
  XCircle,
  Copy,
  Check,
} from "lucide-react";
import { MarkdownRenderer } from "./MarkdownRenderer";
import type { ToolResult } from "../../lib/types";

interface ToolResultCardProps {
  result: ToolResult;
}

function getToolIcon(name: string) {
  const n = name.toLowerCase();
  if (n.includes("read") || n.includes("file") || n.includes("search_files"))
    return FileText;
  if (n.includes("web") || n.includes("fetch") || n.includes("http"))
    return Globe;
  if (n.includes("search") || n.includes("grep") || n.includes("find"))
    return Search;
  if (n.includes("code") || n.includes("exec") || n.includes("terminal"))
    return Code;
  return Terminal;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function ToolResultCard({ result }: ToolResultCardProps) {
  const [expanded, setExpanded] = useState(result.status === "error");
  const [copiedInput, setCopiedInput] = useState(false);
  const [copiedOutput, setCopiedOutput] = useState(false);

  const Icon = getToolIcon(result.name);
  const isError = result.status === "error";

  const handleCopy = async (text: string, setter: (v: boolean) => void) => {
    try {
      await navigator.clipboard.writeText(text);
      setter(true);
      setTimeout(() => setter(false), 1500);
    } catch {
      // fallback
    }
  };

  return (
    <div
      className={`mb-2 border rounded-lg overflow-hidden ${
        isError
          ? "border-red-300 dark:border-red-800"
          : "border-gray-200 dark:border-gray-700"
      }`}
    >
      {/* Header */}
      <button
        onClick={() => setExpanded(!expanded)}
        className={`flex items-center gap-2 px-3 py-1.5 w-full text-left transition-colors ${
          isError
            ? "hover:bg-red-50 dark:hover:bg-red-950/30"
            : "hover:bg-gray-50 dark:hover:bg-gray-800"
        }`}
      >
        <ChevronRight
          className={`w-4 h-4 transition-transform flex-shrink-0 ${
            expanded ? "rotate-90" : ""
          }`}
        />
        <Icon className="w-4 h-4 text-gray-400 flex-shrink-0" />
        <span className="text-sm font-mono font-medium truncate">
          {result.name}
        </span>
        <span className="text-xs text-gray-400 ml-auto flex-shrink-0">
          {formatDuration(result.durationMs)}
        </span>
        {isError ? (
          <XCircle className="w-4 h-4 text-red-500 flex-shrink-0" />
        ) : (
          <CheckCircle className="w-4 h-4 text-green-500 flex-shrink-0" />
        )}
      </button>

      {/* Body */}
      {expanded && (
        <div className="border-t border-gray-200 dark:border-gray-700">
          {/* Input */}
          {result.input && (
            <div className="px-3 py-2 bg-gray-50 dark:bg-gray-900/50">
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs text-gray-400 font-medium">
                  Input
                </span>
                <button
                  onClick={() => handleCopy(result.input, setCopiedInput)}
                  className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                  title="Copy input"
                >
                  {copiedInput ? (
                    <Check className="w-3 h-3 text-green-500" />
                  ) : (
                    <Copy className="w-3 h-3" />
                  )}
                </button>
              </div>
              <pre className="text-xs font-mono overflow-x-auto whitespace-pre-wrap text-gray-600 dark:text-gray-400">
                {result.input}
              </pre>
            </div>
          )}

          {/* Output */}
          {result.output && (
            <div className="px-3 py-2">
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs text-gray-400 font-medium">
                  Output
                </span>
                <button
                  onClick={() => handleCopy(result.output, setCopiedOutput)}
                  className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
                  title="Copy output"
                >
                  {copiedOutput ? (
                    <Check className="w-3 h-3 text-green-500" />
                  ) : (
                    <Copy className="w-3 h-3" />
                  )}
                </button>
              </div>
              <div className="text-sm overflow-x-auto">
                {result.output.includes("\n") ||
                result.output.startsWith("LINE_") ? (
                  <pre className="text-xs font-mono whitespace-pre-wrap text-gray-700 dark:text-gray-300">
                    {result.output}
                  </pre>
                ) : (
                  <MarkdownRenderer content={result.output} />
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
