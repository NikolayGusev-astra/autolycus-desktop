// src/components/views/ProtocolsView.tsx
// Protocols with media/URL/document upload → agent processing (like kanban.gen-ii.ru).
// User uploads audio/video/doc/URL → the content is sent to the agent with a
// prompt to extract a structured protocol + tasks → parsed and saved.
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, FileText, Loader, Upload, Link as LinkIcon, FileUp } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Protocol {
  id: number;
  title: string;
  participants: string;
  meeting_date: string | null;
  decisions: string;
  risks: string;
  notes: string;
}

export function ProtocolsView() {
  const { t } = useTranslation();
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  // manual form
  const [title, setTitle] = useState("");
  const [participants, setParticipants] = useState("");
  const [date, setDate] = useState("");
  const [decisions, setDecisions] = useState("");
  // upload form
  const [uploadMode, setUploadMode] = useState<"file" | "url" | "text">("file");
  const [url, setUrl] = useState("");
  const [pasteText, setPasteText] = useState("");
  const [processing, setProcessing] = useState(false);
  const [statusMsg, setStatusMsg] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try { setProtocols(await invoke<Protocol[]>("list_protocols_cmd", { profile: null })); }
    catch (e) { console.error(e); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!title.trim()) return;
    await invoke("create_protocol_cmd", {
      title: title.trim(), participants, meetingDate: date || null, decisions, risks: "", profile: null,
    });
    setTitle(""); setParticipants(""); setDate(""); setDecisions(""); setShowForm(false); void load();
  };
  const remove = async (id: number) => { await invoke("delete_protocol_cmd", { id, profile: null }); void load(); };

  // ── Upload + agent processing ─────────────────────────────────────────────
  const handleUpload = async () => {
    setProcessing(true);
    setStatusMsg(t("protocols.processing"));
    try {
      let sourceContent = "";
      let sourceTitle = "";

      if (uploadMode === "url") {
        if (!url.trim()) return;
        sourceContent = `[Источник: ${url}]\n\nДай структурированный протокол по содержимому этой ссылки: ${url}`;
        sourceTitle = `Протокол из ${url.slice(0, 50)}`;
      } else if (uploadMode === "text") {
        if (!pasteText.trim()) return;
        sourceContent = pasteText;
        sourceTitle = `Протокол ${new Date().toLocaleDateString("ru")}`;
      } else {
        // file mode — handled by handleFileSelect
        return;
      }

      await processWithAgent(sourceContent, sourceTitle);
    } catch (e) {
      setStatusMsg("✗ " + String(e));
    } finally {
      setProcessing(false);
    }
  };

  const handleFileSelect = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    const file = files[0];
    setProcessing(true);
    setStatusMsg(t("protocols.processing"));
    try {
      // Save file to media cache, then ask the agent to process it.
      const buf = new Uint8Array(await file.arrayBuffer());
      const ext = file.name.split(".").pop() || "bin";
      const path = await invoke<string>("save_media_blob_cmd", { data: Array.from(buf), ext });
      const isMedia = file.type.startsWith("audio/") || file.type.startsWith("video/");
      sourceContentGlobal = isMedia
        ? `[Медиа-файл: ${file.name} (${file.type}). Путь: ${path}]. Транскрибируй и составь протокол встречи.]`
        : `[Документ: ${file.name}]. Содержимое ниже:\n\n${new TextDecoder().decode(buf.slice(0, 50000))}`;
      await processWithAgent(sourceContentGlobal, `Протокол из ${file.name}`);
    } catch (e) {
      setStatusMsg("✗ " + String(e));
    } finally {
      setProcessing(false);
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  let sourceContentGlobal = "";

  const processWithAgent = async (content: string, defaultTitle: string) => {
    // Send to agent with a protocol-extraction prompt. The agent transcribes
    // media, reads docs, and returns structured JSON.
    const prompt = `Ты — помощник для создания протоколов встреч. Обработай следующий источник и верни ТОЛЬКО JSON:\n{"title":"...","participants":"Имя1, Имя2","meeting_date":"YYYY-MM-DD","decisions":"Решение 1; Решение 2","risks":"Риск 1","tasks":[{"title":"Задача 1"},{"title":"Задача 2"}]}\n\nИсточник:\n${content}`;
    const result = await invoke<string>("send_message_cmd", {
      request: { text: prompt, session_id: null, history: null },
    });
    // Parse JSON from the response.
    const match = result.match(/\{[\s\S]*\}/);
    if (match) {
      const parsed = JSON.parse(match[0]);
      const pid = await invoke<number>("create_protocol_cmd", {
        title: parsed.title || defaultTitle,
        participants: parsed.participants || "",
        meetingDate: parsed.meeting_date || null,
        decisions: parsed.decisions || "",
        risks: parsed.risks || "",
        profile: null,
      });
      // Create extracted tasks.
      if (Array.isArray(parsed.tasks)) {
        for (const tk of parsed.tasks.slice(0, 20)) {
          if (tk.title) await invoke("create_task_cmd", { title: tk.title, profile: null });
        }
      }
      setStatusMsg(`✓ ${t("protocols.created")} (ID: ${pid})`);
    } else {
      // No JSON — save raw as protocol.
      await invoke("create_protocol_cmd", {
        title: defaultTitle, participants: "", meetingDate: null, decisions: result.slice(0, 2000), risks: "", profile: null,
      });
      setStatusMsg(`✓ ${t("protocols.createdRaw")}`);
    }
    void load();
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-semibold text-ac-ink">{t("nav.protocols")}</h2>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
          <Plus className="w-4 h-4" /> {t("protocols.new")}
        </button>
      </div>

      {showForm && (
        <div className="mb-6 space-y-4">
          {/* Upload source */}
          <div className="p-4 rounded-lg border border-ac-border bg-ac-surface">
            <p className="text-sm font-medium text-ac-ink mb-3">{t("protocols.uploadSource")}</p>
            <div className="flex gap-2 mb-3">
              <button onClick={() => setUploadMode("file")} className={`px-3 py-1.5 text-xs rounded-md ${uploadMode === "file" ? "bg-ac-brand text-white" : "border border-ac-border text-ac-muted"}`}>
                <FileUp className="w-3.5 h-3.5 inline" /> {t("protocols.file")}
              </button>
              <button onClick={() => setUploadMode("url")} className={`px-3 py-1.5 text-xs rounded-md ${uploadMode === "url" ? "bg-ac-brand text-white" : "border border-ac-border text-ac-muted"}`}>
                <LinkIcon className="w-3.5 h-3.5 inline" /> URL
              </button>
              <button onClick={() => setUploadMode("text")} className={`px-3 py-1.5 text-xs rounded-md ${uploadMode === "text" ? "bg-ac-brand text-white" : "border border-ac-border text-ac-muted"}`}>
                <FileText className="w-3.5 h-3.5 inline" /> {t("protocols.pasteText")}
              </button>
            </div>

            {uploadMode === "file" && (
              <input ref={fileRef} type="file" accept="audio/*,video/*,.txt,.md,.json,.docx,.pdf" onChange={(e) => void handleFileSelect(e.target.files)} className="text-xs text-ac-muted" />
            )}
            {uploadMode === "url" && (
              <div className="flex gap-2">
                <input className="ac-input flex-1 px-3 py-2 text-sm" placeholder="https://..." value={url} onChange={(e) => setUrl(e.target.value)} />
                <button onClick={() => void handleUpload()} disabled={processing || !url.trim()} className="ac-btn px-4 py-2 text-sm flex items-center gap-1.5">
                  {processing ? <Loader className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />} {t("protocols.process")}
                </button>
              </div>
            )}
            {uploadMode === "text" && (
              <div className="space-y-2">
                <textarea className="ac-input w-full px-3 py-2 text-sm" rows={5} placeholder={t("protocols.pasteHere")} value={pasteText} onChange={(e) => setPasteText(e.target.value)} />
                <button onClick={() => void handleUpload()} disabled={processing || !pasteText.trim()} className="ac-btn px-4 py-2 text-sm flex items-center gap-1.5">
                  {processing ? <Loader className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />} {t("protocols.process")}
                </button>
              </div>
            )}
            {statusMsg && <p className={`text-xs mt-2 ${statusMsg.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>{statusMsg}</p>}
          </div>

          {/* Manual form */}
          <details className="p-4 rounded-lg border border-ac-border bg-ac-surface">
            <summary className="text-sm font-medium text-ac-muted cursor-pointer">{t("protocols.manual")}</summary>
            <div className="mt-3 space-y-3">
              <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("protocols.title")} value={title} onChange={(e) => setTitle(e.target.value)} />
              <div className="flex gap-3">
                <input className="ac-input flex-1 px-3 py-2 text-sm" placeholder={t("protocols.participants")} value={participants} onChange={(e) => setParticipants(e.target.value)} />
                <input type="date" className="ac-input px-3 py-2 text-sm" value={date} onChange={(e) => setDate(e.target.value)} />
              </div>
              <textarea className="ac-input w-full px-3 py-2 text-sm" rows={3} placeholder={t("protocols.decisions")} value={decisions} onChange={(e) => setDecisions(e.target.value)} />
              <button onClick={() => void create()} className="ac-btn px-4 py-2 text-sm">{t("btn.add")}</button>
            </div>
          </details>
        </div>
      )}

      {loading ? (<div className="flex justify-center py-8"><Loader className="w-5 h-5 animate-spin text-ac-muted" /></div>)
        : protocols.length === 0 ? (<p className="text-sm text-ac-muted text-center py-12">{t("protocols.empty")}</p>)
        : (<div className="space-y-2">
          {protocols.map((p) => (
            <div key={p.id} className="group p-4 rounded-lg border border-ac-border bg-ac-surface">
              <div className="flex items-start gap-3">
                <FileText className="w-4 h-4 text-ac-brand mt-0.5 shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-ac-ink">{p.title}</p>
                  {p.participants && <p className="text-xs text-ac-muted mt-0.5">👥 {p.participants}</p>}
                  {p.meeting_date && <p className="text-xs text-ac-faint mt-0.5">📅 {p.meeting_date}</p>}
                  {p.decisions && <p className="text-xs text-ac-muted mt-1 whitespace-pre-wrap">{p.decisions.slice(0, 300)}{p.decisions.length > 300 ? "…" : ""}</p>}
                </div>
                <button onClick={() => void remove(p.id)} className="opacity-0 group-hover:opacity-100 text-ac-faint hover:text-ac-red"><Trash2 className="w-3.5 h-3.5" /></button>
              </div>
            </div>
          ))}
        </div>)}
    </div>
  );
}
export default ProtocolsView;
