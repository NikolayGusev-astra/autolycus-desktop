// src/components/profiles/ProfilesScreen.tsx
// Phase 4: Profile list + CRUD + active model config via invoke

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  UserPlus,
  Trash2,
  CheckCircle2,
  Cpu,
  Loader,
  User,
  Terminal,
  FileText,
  FolderOpen,
  Globe,
  Settings2,
} from "lucide-react";
import type { ProfileInfo, ModelConfig } from "../../lib/types";
import { useTranslation } from "../../hooks/useTranslation";

// ── Constants ──────────────────────────────────────────────────────────────

const PROVIDERS = [
  { value: "openrouter", label: "OpenRouter" },
  { value: "openai", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic" },
  { value: "ollama", label: "Ollama" },
  { value: "ollama-cloud", label: "Ollama Cloud" },
  { value: "custom", label: "Custom" },
];

// ── Create Profile Dialog ──────────────────────────────────────────────────

function CreateProfileDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => void;
}) {
  const [name, setName] = useState("");
  const [clone, setClone] = useState(false);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const { t } = useTranslation();

  const handleCreate = async () => {
    // Validate: not empty, no spaces
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Profile name cannot be empty");
      return;
    }
    if (/\s/.test(trimmed)) {
      setError("Profile name must not contain spaces");
      return;
    }

    setSaving(true);
    setError("");
    try {
      await invoke("create_profile_cmd", { name: trimmed, clone });
      onCreated();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-ac-bg border border-ac-border rounded-xl p-5 w-full max-w-sm shadow-2xl">
        <h3 className="text-sm font-semibold text-ac-ivory mb-4">
          {t("create_profile_title")}
        </h3>

        <div className="space-y-3">
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">
              {t("profile_name_label")}
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setError("");
              }}
              onKeyDown={(e) => e.key === "Enter" && handleCreate()}
              placeholder={t("profile_name_placeholder")}
              className="ac-input w-full px-3 py-2 text-sm font-mono"
              autoFocus
            />
          </div>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={clone}
              onChange={(e) => setClone(e.target.checked)}
              className="accent-ac-amber"
            />
            <span className="text-xs text-ac-stone">
              {t("clone_settings")}
            </span>
          </label>

          {error && (
            <p className="text-xs text-ac-red">{error}</p>
          )}
        </div>

        <div className="flex justify-end gap-2 mt-4">
          <button
            onClick={onClose}
            className="ac-btn px-3 py-1.5 text-xs opacity-70"
            disabled={saving}
          >
            {t("cancel")}
          </button>
          <button
            onClick={handleCreate}
            className="ac-btn px-4 py-1.5 text-xs"
            disabled={saving}
          >
            {saving ? t("creating") : t("create")}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Model Config Section ───────────────────────────────────────────────────

function ModelConfigSection({
  profile,
  onUpdated,
}: {
  profile: string | null;
  onUpdated: () => void;
}) {
  const [config, setConfig] = useState<ModelConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [editMode, setEditMode] = useState(false);

  // Edit form state
  const [editProvider, setEditProvider] = useState("openrouter");
  const [editModel, setEditModel] = useState("");
  const [editBaseUrl, setEditBaseUrl] = useState("");
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState("");
  const { t } = useTranslation();

  const loadConfig = useCallback(async () => {
    try {
      setLoading(true);
      const cfg = await invoke<ModelConfig>("get_model_config_cmd", {
        profile: profile as string | null,
      });
      setConfig(cfg);
      setEditProvider(cfg.provider || "openrouter");
      setEditModel(cfg.model || "");
      setEditBaseUrl(cfg.base_url || "");
    } catch (err) {
      console.error("Failed to load model config:", err);
    } finally {
      setLoading(false);
    }
  }, [profile]);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const handleSave = async () => {
    if (!editModel.trim()) {
      setStatus(t("model_required"));
      return;
    }
    setSaving(true);
    setStatus("");
    try {
      await invoke("set_model_config_cmd", {
        provider: editProvider,
        model: editModel.trim(),
        baseUrl: editBaseUrl.trim(),
        profile: profile as string | null,
      });
      setConfig({
        provider: editProvider,
        model: editModel.trim(),
        base_url: editBaseUrl.trim(),
      });
      setEditMode(false);
      setStatus(t("config_saved"));
      onUpdated();
    } catch (err) {
      setStatus(`✗ ${String(err)}`);
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-xs text-ac-stone py-2">
        <Loader className="w-3 h-3 animate-spin" />
        {t("loading_content")}
      </div>
    );
  }

  return (
    <div className="border border-ac-border rounded-lg p-3 space-y-3">
      <div className="flex items-center justify-between">
        <label className="ac-section-title text-xs flex items-center gap-1.5">
          <Cpu className="w-3.5 h-3.5 text-ac-amber" />
          {t("active_model_config")}
        </label>
        <button
          onClick={() => setEditMode(!editMode)}
          className="text-[11px] text-ac-amber hover:text-ac-amber/80 flex items-center gap-1"
        >
          <Settings2 className="w-3 h-3" />
          {editMode ? t("cancel") : t("change")}
        </button>
      </div>

      {editMode ? (
        <div className="space-y-2.5">
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("provider_label")}</label>
            <select
              value={editProvider}
              onChange={(e) => setEditProvider(e.target.value)}
              className="ac-input w-full px-3 py-2 text-sm"
            >
              {PROVIDERS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("model_label")}</label>
            <input
              type="text"
              value={editModel}
              onChange={(e) => setEditModel(e.target.value)}
              placeholder={t("model_placeholder")}
              className="ac-input w-full px-3 py-2 text-sm font-mono"
            />
          </div>
          <div>
            <label className="text-[11px] text-ac-stone mb-1 block">{t("base_url_label")}</label>
            <input
              type="text"
              value={editBaseUrl}
              onChange={(e) => setEditBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
              className="ac-input w-full px-3 py-2 text-sm font-mono"
            />
          </div>
          <div className="flex justify-end">
            <button
              onClick={handleSave}
              className="ac-btn px-3 py-1.5 text-xs"
              disabled={saving}
            >
              {saving ? t("saving") : t("save_config")}
            </button>
          </div>
          {status && (
            <p
              className={`text-xs ${
                status.startsWith("✓") ? "text-green-400" : "text-ac-red"
              }`}
            >
              {status}
            </p>
          )}
        </div>
      ) : config ? (
        <div className="space-y-1">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-ac-ivory font-medium">
              {config.provider || "—"}
            </span>
            {config.model && (
              <>
                <span className="text-ac-stone/50">/</span>
                <span className="text-ac-ivory font-mono">{config.model}</span>
              </>
            )}
          </div>
          {config.base_url && (
            <p className="text-[11px] text-ac-stone font-mono truncate">
              {config.base_url}
            </p>
          )}
        </div>
      ) : (
        <p className="text-xs text-ac-stone">{t("no_model_configured")}</p>
      )}
    </div>
  );
}

// ── Main Component ─────────────────────────────────────────────────────────

export function ProfilesScreen() {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [, setStatus] = useState("");
  const { t } = useTranslation();

  const _activeProfile = profiles.find((p) => p.is_active);
  void _activeProfile;

  const loadProfiles = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<ProfileInfo[]>("list_profiles_cmd");
      setProfiles(result);
    } catch (err) {
      console.error("Failed to load profiles:", err);
      setStatus(`✗ Failed to load profiles: ${String(err)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadProfiles();
  }, [loadProfiles]);

  const handleActivate = async (name: string) => {
    try {
      await invoke("set_active_profile_cmd", { name });
      setStatus(`✓ Activated profile: ${name}`);
      loadProfiles();
    } catch (err) {
      setStatus(`✗ ${String(err)}`);
    }
  };

  const handleDelete = async (name: string) => {
    try {
      await invoke("delete_profile_cmd", { name });
      setStatus(`✓ Deleted profile: ${name}`);
      setDeleteConfirm(null);
      loadProfiles();
    } catch (err) {
      setStatus(`✗ ${String(err)}`);
      setDeleteConfirm(null);
    }
  };

  const handleModelUpdated = () => {
    loadProfiles();
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="px-4 py-3 border-b border-ac-border flex items-center justify-between">
        <h2 className="text-sm font-semibold text-ac-ivory flex items-center gap-2">
          <User className="w-4 h-4 text-ac-amber" />
          {t("profiles_title")}
        </h2>
        <button
          onClick={() => setShowCreate(true)}
          className="ac-btn px-3 py-1.5 text-xs flex items-center gap-1.5"
        >
          <UserPlus className="w-3.5 h-3.5" />
          {t("new_profile")}
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {loading ? (
          <div className="flex items-center justify-center h-32">
            <Loader className="w-5 h-5 text-ac-amber animate-spin" />
          </div>
        ) : profiles.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-ac-stone text-sm">
            <User className="w-8 h-8 mb-2 opacity-30" />
            <p>{t("no_profiles")}</p>
          </div>
        ) : (
          <div className="space-y-2">
            {profiles.map((profile) => {
              const canDelete = !profile.is_default;
              const canActivate = !profile.is_active;

              return (
                <div
                  key={profile.name}
                  className={`relative rounded-lg border px-4 py-3 transition-colors ${
                    profile.is_active
                      ? "bg-ac-amber/5 border-ac-amber/40"
                      : "bg-ac-bg border-ac-border hover:border-ac-border/70"
                  }`}
                >
                  {/* Delete confirm overlay */}
                  {deleteConfirm === profile.name && (
                   <div className="absolute inset-0 z-10 bg-ac-pitch/90 rounded-lg flex items-center justify-center">
                     <div className="text-center px-4">
                       <p className="text-xs text-ac-ivory mb-3">
                          {t("delete_confirm").replace("{name}", profile.name)}
                       </p>
                        <div className="flex justify-center gap-2">
                          <button
                            onClick={() => setDeleteConfirm(null)}
                            className="ac-btn px-3 py-1 text-[11px] opacity-70"
                          >
                            {t("cancel")}
                          </button>
                          <button
                            onClick={() => handleDelete(profile.name)}
                            className="ac-btn px-3 py-1 text-[11px] bg-ac-red/20 text-ac-red border-ac-red/30"
                          >
                            {t("delete")}
                          </button>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Profile header */}
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium text-ac-ivory">
                          {profile.name}
                        </span>
                        {profile.is_default && (
                          <span className="text-[10px] px-1.5 py-0.5 bg-ac-stone/20 text-ac-stone rounded">
                            {t("default_badge")}
                          </span>
                        )}
                        {profile.is_active && (
                          <span className="text-[10px] px-1.5 py-0.5 bg-ac-amber/20 text-ac-amber rounded flex items-center gap-1">
                            <CheckCircle2 className="w-2.5 h-2.5" />
                            {t("active_badge")}
                          </span>
                        )}
                      </div>

                      <div className="flex flex-wrap gap-x-4 gap-y-1 mt-1.5 text-[11px] text-ac-stone">
                        {profile.provider && (
                          <span className="flex items-center gap-1">
                            <Globe className="w-3 h-3" />
                            {profile.provider}
                          </span>
                        )}
                        {profile.model && (
                          <span className="font-mono flex items-center gap-1">
                            <Cpu className="w-3 h-3" />
                            {profile.model}
                          </span>
                        )}
                        <span className="flex items-center gap-1">
                          <FolderOpen className="w-3 h-3" />
                          {profile.skill_count} {t("skills_count")}
                        </span>
                        {profile.has_env && (
                          <span className="flex items-center gap-1 text-green-500/70">
                            <FileText className="w-3 h-3" />
                            .env
                          </span>
                        )}
                        {profile.has_soul && (
                          <span className="flex items-center gap-1">
                            <Terminal className="w-3 h-3" />
                            soul.md
                          </span>
                        )}
                        {profile.gateway_running && (
                          <span className="text-green-500 flex items-center gap-1">
                            <span className="w-1.5 h-1.5 rounded-full bg-green-500" />
                            gateway
                          </span>
                        )}
                      </div>
                    </div>

                    {/* Actions */}
                    <div className="flex items-center gap-1 shrink-0">
                      {canActivate && (
                        <button
                          onClick={() => handleActivate(profile.name)}
                          className="text-[11px] text-ac-amber hover:text-ac-amber/80 px-2 py-1"
                          title={t("activate_profile")}
                        >
                          {t("activate")}
                        </button>
                      )}
                      {canDelete && (
                        <button
                          onClick={() => setDeleteConfirm(profile.name)}
                          className="text-[11px] text-ac-stone hover:text-ac-red px-2 py-1"
                          title={t("delete_profile")}
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                  </div>

                  {/* Model Config Section */}
                  {profile.is_active && (
                    <div className="mt-3">
                      <ModelConfigSection
                        profile={profile.is_default ? null : profile.name}
                        onUpdated={handleModelUpdated}
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Create dialog */}
      {showCreate && (
        <CreateProfileDialog
          onClose={() => setShowCreate(false)}
          onCreated={loadProfiles}
        />
      )}
    </div>
  );
}