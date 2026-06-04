import { useState } from "react";
import { ChevronRight, Brain } from "lucide-react";
import { MarkdownRenderer } from "./MarkdownRenderer";

interface ThinkingBlockProps {
  content: string;
}

export function ThinkingBlock({ content }: ThinkingBlockProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="mb-2 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 px-3 py-1.5 w-full text-left text-gray-500 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        <ChevronRight
          className={`w-4 h-4 transition-transform ${expanded ? "rotate-90" : ""}`}
        />
        <Brain className="w-4 h-4" />
        <span className="text-sm font-medium">Thinking</span>
      </button>
      {expanded && (
        <div className="px-3 py-2 text-sm text-gray-500 border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/50">
          <MarkdownRenderer content={content} />
        </div>
      )}
    </div>
  );
}
