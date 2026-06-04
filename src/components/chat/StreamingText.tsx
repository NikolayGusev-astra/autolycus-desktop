import { useEffect, useState } from "react";
import { MarkdownRenderer } from "./MarkdownRenderer";

interface StreamingTextProps {
  content: string;
}

export function StreamingText({ content }: StreamingTextProps) {
  const [displayed, setDisplayed] = useState("");
  const [cursorVisible, setCursorVisible] = useState(true);

  useEffect(() => {
    setDisplayed(content);
  }, [content]);

  useEffect(() => {
    const interval = setInterval(() => setCursorVisible((v) => !v), 500);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="relative">
      <MarkdownRenderer content={displayed} />
      <span
        className={`inline-block w-0.5 h-4 bg-gray-800 dark:bg-gray-200 ml-0.5 align-middle transition-opacity ${
          cursorVisible ? "opacity-100" : "opacity-0"
        }`}
      />
    </div>
  );
}
