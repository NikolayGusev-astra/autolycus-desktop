// src/components/SelfDiagModal.tsx
// Mood/energy check-in modal. Calls add_self_check_cmd to record data.
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Zap } from "lucide-react";
import { useTranslation } from "../hooks/useTranslation";

export function SelfDiagModal({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [energy, setEnergy] = useState(3);
  const [joy, setJoy] = useState(3);
  const [mood, setMood] = useState("");
  const [notes, setNotes] = useState("");
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState("");

  const submit = async () => {
    setSaving(true);
    try {
      await invoke("add_self_check_cmd", { energy, joy, mood, notes, profile: null });
      setStatus("✓ " + t("saved"));
      setTimeout(onClose, 1200);
    } catch (e) {
      setStatus("✗ " + String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="ac-modal-overlay" onClick={onClose}>
      <div className="ac-modal" style={{ maxWidth: 420 }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 mb-4">
          <Zap className="w-4 h-4 text-ac-brand" />
          <p className="text-sm font-semibold text-ac-ink">{t("nav.selfDiagnosis")}</p>
        </div>
        <p className="text-xs text-ac-muted mb-4">{t("selfDiag.hint")}</p>

        <div className="space-y-4">
          <div>
            <label className="text-xs text-ac-muted">{t("stats.energy")}: {energy}/5</label>
            <input type="range" min={1} max={5} value={energy} onChange={(e) => setEnergy(Number(e.target.value))} className="w-full" />
          </div>
          <div>
            <label className="text-xs text-ac-muted">{t("stats.joy")}: {joy}/5</label>
            <input type="range" min={1} max={5} value={joy} onChange={(e) => setJoy(Number(e.target.value))} className="w-full" />
          </div>
          <input className="ac-input w-full px-3 py-2 text-sm" placeholder={t("stats.mood")} value={mood} onChange={(e) => setMood(e.target.value)} />
          <textarea className="ac-input w-full px-3 py-2 text-sm" rows={2} placeholder={t("selfDiag.placeholder")} value={notes} onChange={(e) => setNotes(e.target.value)} />
        </div>

        {status && <p className={`text-xs mt-2 ${status.startsWith("✓") ? "text-green-400" : "text-ac-red"}`}>{status}</p>}

        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-4 py-2 text-sm border border-ac-border text-ac-muted rounded-md">
            {t("btn.close")}
          </button>
          <button onClick={() => void submit()} disabled={saving} className="ac-btn px-4 py-2 text-sm disabled:opacity-40">
            {t("stats.saveCheck")}
          </button>
        </div>
      </div>
    </div>
  );
}
