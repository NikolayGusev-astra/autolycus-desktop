// src/components/views/ProtocolsView.tsx
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, FileText, Loader } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";

interface Protocol { id: number; title: string; participants: string; meeting_date: string | null; decisions: string; risks: string; }

export function ProtocolsView() {
  const { t } = useTranslation();
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [title, setTitle] = useState("");
  const [participants, setParticipants] = useState("");
  const [date, setDate] = useState("");
  const [decisions, setDecisions] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try { setProtocols(await invoke<Protocol[]>("list_protocols_cmd", { profile: null })); }
    catch (e) { console.error(e); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const create = async () => {
    if (!title.trim()) return;
    await invoke("create_protocol_cmd", { title: title.trim(), participants, meetingDate: date || null, decisions, risks: "", profile: null });
    setTitle(""); setParticipants(""); setDate(""); setDecisions(""); setShowForm(false); void load();
  };
  const remove = async (id: number) => { await invoke("delete_protocol_cmd", { id, profile: null }); void load(); };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-semibold text-ac-ink">{t("nav.protocols")}</h2>
        <button onClick={() => setShowForm(!showForm)} className="ac-btn px-3 py-2 text-sm flex items-center gap-1.5">
          <Plus className="w-4 h-4" /> {t("protocols.new")}
        </button>
      </div>
      {showForm && (
        <div className="mb-4 p-4 rounded-lg border border-ac-border bg-ac-surface space-y-3">
          <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("protocols.title")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <div className="flex gap-3">
            <input className="ac-input flex-1 px-3 py-2 text-sm" placeholder={t("protocols.participants")} value={participants} onChange={(e) => setParticipants(e.target.value)} />
            <input type="date" className="ac-input px-3 py-2 text-sm" value={date} onChange={(e) => setDate(e.target.value)} />
          </div>
          <textarea className="ac-input w-full px-3 py-2 text-sm" rows={3} placeholder={t("protocols.decisions")} value={decisions} onChange={(e) => setDecisions(e.target.value)} />
          <button onClick={() => void create()} className="ac-btn px-4 py-2 text-sm">{t("btn.add")}</button>
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
                  {p.participants && <p className="text-xs text-ac-muted mt-0.5">{p.participants}</p>}
                  {p.meeting_date && <p className="text-xs text-ac-faint mt-0.5">{p.meeting_date}</p>}
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
