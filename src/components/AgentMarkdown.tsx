// src/components/AgentMarkdown.tsx
import { useState, memo } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { Copy, Check } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

// Diff viewer with colored +/- lines
function DiffView({ code }: { code: string }): React.JSX.Element {
  const lines = code.split("\n");
  return (
    <div className="chat-diff-content">
      {lines.map((line, i) => {
        let cls = "chat-diff-line";
        if (line.startsWith("+")) cls += " chat-diff-add";
        else if (line.startsWith("-")) cls += " chat-diff-remove";
        else if (line.startsWith("@@")) cls += " chat-diff-hunk";
        return (
          <div key={i} className={cls}>
            {line || "\u00A0"}
          </div>
        );
      })}
    </div>
  );
}

// Code block with syntax highlighting and copy button
function CodeBlock({
  className,
  children,
}: {
  className?: string;
  children?: React.ReactNode;
}): React.JSX.Element {
  const [copied, setCopied] = useState(false);
  const code = String(children).replace(/\n$/, "");
  const match = /language-(\w+)/.exec(className || "");
  const language = match ? match[1] : "";
  const isDiff = language === "diff";

  function handleCopy(): void {
    void navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="chat-code-block">
      <div className="chat-code-header">
        <span className="chat-code-lang">
          {isDiff ? "diff" : language || "code"}
        </span>
        <button className="chat-code-copy" onClick={handleCopy}>
          {copied ? <Check size={13} /> : <Copy size={13} />}
        </button>
      </div>
      {isDiff ? (
        <DiffView code={code} />
      ) : (
        <SyntaxHighlighter
          style={oneDark}
          language={language || "text"}
          PreTag="div"
          customStyle={{
            margin: 0,
            borderRadius: 0,
            fontSize: "13px",
            padding: "12px",
            background: "transparent",
          }}
        >
          {code}
        </SyntaxHighlighter>
      )}
    </div>
  );
}

// Shared Markdown renderer that opens links externally
const AgentMarkdown = memo(function AgentMarkdown({
  children,
}: {
  children: string;
}): React.JSX.Element {
  return (
    <Markdown
      remarkPlugins={[remarkGfm]}
      components={{
        a: ({ href, children }) => (
          <a
            href={href}
            onClick={(e) => {
              e.preventDefault();
              if (!href) return;
              try {
                const url = new URL(href, "https://placeholder.invalid");
                if (!["http:", "https:", "mailto:"].includes(url.protocol)) {
                  return;
                }
              } catch {
                return;
              }
              void openUrl(href);
            }}
          >
            {children}
          </a>
        ),
        code: ({ className, children, ...props }) => {
          const isInline =
            !className &&
            typeof children === "string" &&
            !children.includes("\n");
          if (isInline) {
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          }
          return <CodeBlock className={className}>{children}</CodeBlock>;
        },
      }}
    >
      {children}
    </Markdown>
  );
});

export { AgentMarkdown };
export default AgentMarkdown;
