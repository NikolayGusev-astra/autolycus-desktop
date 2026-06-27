"use client";

import { useState, useRef, useEffect, FormEvent, useCallback } from "react";
import { Input } from "../components/ui/input";
import { Button } from "../components/ui/button";
import { Bot, Send, Loader2, Paperclip, Link2, FileText, Mic, CheckSquare } from "lucide-react";
import { clsx as cn } from "../lib/utils";

const SUGGESTIONS = [
  { icon: "📄", text: "Договоры с ООО Ромашка" },
  { icon: "🏢", text: "Какие контрагенты у нас есть?" },
  { icon: "📊", text: "Покажи все акты по договору №123" },
];

function MessageBubble({ message }: { message: any }) {
  const isUser = message.role === "user";
  const [downloading, setDownloading] = useState(false);

  const handleDownloadDocx = async () => {
    if (!message.content) return;
    setDownloading(true);
    try {
      const res = await fetch("/api/kanban/upload", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          source_type: "text",
          text: message.content,
          title: `Отчёт ${new Date().toLocaleDateString("ru")}`,
        }),
      });
      const data = await res.json();
      if (data.protocol?.id) window.open(`/api/kanban/report/${data.protocol.id}/docx`, "_blank");
    } catch {
      /* ok */
    } finally {
      setDownloading(false);
    }
  };

  const handleExtractTasks = async () => {
    if (!message.content) return;
    try {
      const res = await fetch("/api/kanban/upload", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          source_type: "text",
          text: message.content,
          title: `Протокол ${new Date().toLocaleDateString("ru")}`,
        }),
      });
      const data = await res.json();
      if (data.protocol?.id) {
        const poll = async (attempts = 0): Promise<void> => {
          const r = await fetch(`/api/kanban/protocol/${data.protocol!.id}`);
          const d = await r.json();
          if (d.status === "done" && d.assignments_json) {
            const tasks = JSON.parse(d.assignments_json);
            if (tasks.length > 0) console.log(`Извлечено ${tasks.length} задач`);
          } else if (d.status === "processing" && attempts < 20) {
            await new Promise((r) => setTimeout(r, 3000));
            return poll(attempts + 1);
          }
        };
        await poll();
      }
    } catch {
      /* ok */
    }
  };

  return (
    <div className={cn("flex items-start gap-3", isUser && "flex-row-reverse")}>
      <div className="shrink-0 w-8 h-8 rounded-full bg-muted flex items-center justify-center text-xs">
        {isUser ? "Вы" : <Bot size={16} className="text-muted-foreground" />}
      </div>
      <div className="flex flex-col gap-1 max-w-[75%]">
        <div
          className={cn(
            "px-4 py-2.5 text-sm whitespace-pre-wrap break-words rounded-2xl",
            isUser ? "bg-primary text-primary-foreground rounded-tr-sm" : "bg-muted rounded-tl-sm"
          )}
        >
          {message.content}
        </div>
        {!isUser && !message.id.startsWith("welcome") && (
          <div className="flex gap-3 opacity-0 hover:opacity-100">
            <button onClick={handleDownloadDocx} disabled={downloading} className="text-xs text-muted-foreground hover:text-foreground">
              {downloading ? <Loader2 size={12} className="animate-spin" /> : <FileText size={12} />} DOCX
            </button>
            <button onClick={handleExtractTasks} className="text-xs text-muted-foreground hover:text-foreground">
              <CheckSquare size={12} /> Извлечь задачи
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

export function SteersmanScreen() {
  const [messages, setMessages] = useState<any[]>([
    {
      id: "welcome",
      role: "assistant",
      content:
        "Здравствуйте! 👋\n\nЯ — Штурман, ИИ-ассистент компании «Цифровое будущее». Помогаю работать с документами: договоры, письма, акты, счета.\n\nЗадайте вопрос по базе документов или попробуйте:\n• Договоры с ООО Ромашка\n• Акты за май 2026 года\n• Какие контрагенты у нас есть?\n• Покажи все акты по договору №123",
    },
  ]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [showUrlInput, setShowUrlInput] = useState(false);
  const [urlInput, setUrlInput] = useState("");
  const [isVoiceListening, setIsVoiceListening] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const recognitionRef = useRef<any>(null);

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages, isLoading]);

  const doSendMessage = useCallback(async (text: string) => {
    const res = await fetch("/api/kanban/chat", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message: text }),
      signal: AbortSignal.timeout(90000),
    });
    const data = await res.json();
    return data.content || data.reply || null;
  }, []);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const text = input.trim();
    if (!text || isLoading) return;
    setMessages((prev) => [...prev, { id: `u-${Date.now()}`, role: "user", content: text }]);
    setInput("");
    setIsLoading(true);
    try {
      const reply = await doSendMessage(text);
      if (reply) setMessages((prev) => [...prev, { id: `a-${Date.now()}`, role: "assistant", content: reply }]);
    } catch {
      /* ok */
    } finally {
      setIsLoading(false);
    }
  };

  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    if (file.size > 50_000_000) return;
    setUploading(true);
    setMessages((prev) => [...prev, { id: `u-${Date.now()}`, role: "user", content: `📎 ${file.name}` }]);
    try {
      const formData = new FormData();
      formData.append("file", file);
      formData.append("source_type", "file");
      formData.append("title", file.name);
      const res = await fetch("/api/kanban/upload", { method: "POST", body: formData });
      const data = await res.json();
      if (data.protocol?.id) {
        setMessages((prev) => [
          ...prev,
          { id: `a-${Date.now()}`, role: "assistant", content: `✅ Файл «${file.name}» загружен. Индексируется...` },
        ]);
      }
    } catch {
      /* ok */
    } finally {
      setUploading(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const handleUrlSubmit = async () => {
    if (!urlInput.trim()) return;
    setUploading(true);
    setMessages((prev) => [...prev, { id: `u-${Date.now()}`, role: "user", content: `🔗 ${urlInput}` }]);
    try {
      const res = await fetch("/api/kanban/upload", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ source_type: "url", url: urlInput, title: urlInput }),
      });
      const data = await res.json();
      if (data.protocol?.id) {
        setMessages((prev) => [
          ...prev,
          { id: `a-${Date.now()}`, role: "assistant", content: "🔗 Ссылка принята, индексируется..." },
        ]);
        setShowUrlInput(false);
        setUrlInput("");
      }
    } catch {
      /* ok */
    } finally {
      setUploading(false);
    }
  };

  const handleVoiceInput = () => {
    const SpeechRecognition = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    if (!SpeechRecognition) return;
    if (isVoiceListening && recognitionRef.current) {
      recognitionRef.current.stop();
      setIsVoiceListening(false);
      return;
    }
    const recognition = new SpeechRecognition();
    recognition.lang = "ru-RU";
    recognition.continuous = false;
    recognition.onresult = (event: any) => {
      let t = "";
      for (let i = 0; i < event.results.length; i++) t += event.results[i][0].transcript;
      setInput(t);
    };
    recognition.onend = () => setIsVoiceListening(false);
    recognition.onerror = () => setIsVoiceListening(false);
    recognition.start();
    recognitionRef.current = recognition;
    setIsVoiceListening(true);
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-4 py-3 border-b">
        <h2 className="font-semibold">🎯 Штурман</h2>
        <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded">
          📚 35 000+ документов
        </span>
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
        {(isLoading || uploading) && (
          <div className="flex items-start gap-3">
            <Bot size={16} className="text-muted-foreground" />
            <div className="bg-muted rounded-2xl px-4 py-2 text-sm text-muted-foreground">Думаю...</div>
          </div>
        )}
      </div>

      {showUrlInput && (
        <div className="px-4 py-2 border-t bg-muted/50 flex gap-2">
          <Input
            type="url"
            value={urlInput}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => setUrlInput(e.target.value)}
            onKeyDown={(e: React.KeyboardEvent) => e.key === "Enter" && handleUrlSubmit()}
            placeholder="https://youtube.com/..."
            className="flex-1"
          />
          <Button onClick={handleUrlSubmit} size="sm" disabled={uploading}>
            {uploading ? <Loader2 size={14} className="animate-spin" /> : "OK"}
          </Button>
        </div>
      )}

      <form onSubmit={handleSubmit} className="p-4 border-t space-y-3">
        <input
          ref={fileInputRef}
          type="file"
          onChange={handleFileUpload}
          className="hidden"
          accept=".pdf,.txt,.md,.docx,.mp3,.mp4,.wav,.webm"
        />
        <div className="flex gap-2 items-center">
          <Button type="button" variant="ghost" size="icon" onClick={() => fileInputRef.current?.click()} disabled={uploading} title="Загрузить файл">
            <Paperclip size={16} />
          </Button>
          <Button type="button" variant="ghost" size="icon" onClick={() => setShowUrlInput(!showUrlInput)} title="Вставить ссылку">
            <Link2 size={16} />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={handleVoiceInput}
            title="Голосовой ввод"
            className={cn(isVoiceListening && "bg-primary/20 text-primary")}
          >
            <Mic size={16} className={cn(isVoiceListening && "animate-pulse")} />
          </Button>
          <Input
            ref={inputRef}
            value={input}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => setInput(e.target.value)}
            placeholder={isVoiceListening ? "Слушаю..." : "Спросите о документах..."}
            disabled={isLoading}
            className="flex-1"
          />
          <Button type="submit" disabled={isLoading || !input.trim()}>
            {isLoading ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
          </Button>
        </div>
        {messages.length <= 1 && (
          <div className="flex flex-wrap gap-2">
            {SUGGESTIONS.map((s) => (
              <button key={s.text} type="button" onClick={() => setInput(s.text)} className="text-xs px-3 py-1.5 rounded-full bg-muted hover:bg-muted/80">
                {s.icon} {s.text}
              </button>
            ))}
          </div>
        )}
      </form>
    </div>
  );
}
