// src/components/providers/ProvidersScreen.tsx
// Providers management screen — ported from fathah/hermes-desktop
// Uses Tauri invoke instead of Electron IPC

import { useState, useEffect, useRef, useCallback } from "react";
import { KeyRound, Plus, Trash2, Eye, EyeOff, Check } from "lucide-react";
import { useTranslation } from "../../hooks/useTranslation";
import { invoke } from "@tauri-apps/api/core";
import OAuthLoginModal from "../auth/OAuthLoginModal";

interface CredentialPoolEntry {
  id?: string;
  label?: string;
  auth_type?: "api_key" | "oauth_device_code" | string;
  priority?: number;
  source?: string;
  access_token?: string;
  refresh_token?: string;
  api_key?: string;
  base_url?: string;
  request_count?: number;
  key?: string;
}

interface ModelConfig {
  provider: string;
  model: string;
  baseUrl: string;
}

const OAUTH_PROVIDERS = [
  "openai-codex",
  "xai-oauth",
  "qwen-oauth",
  "google-gemini-cli",
  "minimax-oauth",
  "nous",
] as const;

const PROVIDERS_OPTIONS = [
  { value: "auto", label: "providers.auto" },
  { value: "openai", label: "providers.openai" },
  { value: "openrouter", label: "providers.openrouter" },
  { value: "anthropic", label: "providers.anthropic" },
  { value: "deepseek", label: "providers.deepseek" },
  { value: "groq", label: "providers.groq" },
  { value: "mistral", label: "providers.mistral" },
  { value: "together", label: "providers.together" },
  { value: "fireworks", label: "providers.fireworks" },
  { value: "ollama", label: "providers.ollama" },
  { value: "lmstudio", label: "providers.lmstudio" },
  { value: "vllm", label: "providers.vllm" },
  { value: "llamacpp", label: "providers.llamacpp" },
  { value: "huggingface", label: "providers.huggingface" },
  { value: "perplexity", label: "providers.perplexity" },
  { value: "cerebras", label: "providers.cerebras" },
  { value: "custom", label: "providers.custom" },
];

function ProvidersScreen({ profile }: { profile?: string }): React.JSX.Element {
  const { t } = useTranslation();

  // Env / API keys
  const [env, setEnv] = useState<Record<string, string>>({});
  const [savedKey, setSavedKey] = useState<string | null>(null);
  const [visibleKeys, setVisibleKeys] = useState<Set<string>>(new Set());

  // Model config
  const [modelProvider, setModelProvider] = useState("auto");
  const [modelName, setModelName] = useState("");
  const [modelBaseUrl, setModelBaseUrl] = useState("");
  const [modelSaved, setModelSaved] = useState(false);
  const modelLoaded = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Credential pool
  const [credPool, setCredPool] = useState<Record<string, CredentialPoolEntry[]>>({});
  const [poolProvider, setPoolProvider] = useState("");
  const [poolNewKey, setPoolNewKey] = useState("");
  const [poolNewLabel, setPoolNewLabel] = useState("");

  // OAuth modal
  const [oauthModal, setOauthModal] = useState<string | null>(null);

  // Env save timers
  const envSaveTimers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  const envRef = useRef<Record<string, string>>({});

  const loadConfig = useCallback(async () => {
    try {
      const [envData, mc] = await Promise.all([
        invoke<Record<string, string>>("get_env_cmd", { profile }),
        invoke<ModelConfig>("get_model_config_cmd", { profile }),
      ]);
      setEnv(envData);
      setModelProvider(mc.provider);
      setModelName(mc.model);
      setModelBaseUrl(mc.baseUrl);
      requestAnimationFrame(() => { modelLoaded.current = true; });
    } catch (err) {
      console.error("Failed to load config:", err);
    }
  }, [profile]);

  useEffect(() => {
    modelLoaded.current = false;
    loadConfig();
  }, [loadConfig]);

  // Auto-save model config
  const saveModelConfig = useCallback(async () => {
    if (!modelLoaded.current) return;
    try {
      await invoke("set_model_config_cmd", {
        profile,
        provider: modelProvider,
        model: modelName,
        baseUrl: modelBaseUrl,
      });
      setModelSaved(true);
      setTimeout(() => setModelSaved(false), 2000);
    } catch (err) {
      console.error("Failed to save model config:", err);
    }
  }, [modelProvider, modelName, modelBaseUrl, profile]);

  useEffect(() => {
    if (!modelLoaded.current) return;
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(saveModelConfig, 500);
    return () => { if (saveTimer.current) clearTimeout(saveTimer.current); };
  }, [modelProvider, modelName, modelBaseUrl, saveModelConfig]);

  // Env handlers
  async function handleBlur(key: string) {
    const pending = envSaveTimers.current.get(key);
    if (pending) { clearTimeout(pending); envSaveTimers.current.delete(key); }
    const value = env[key] || "";
    try {
      await invoke("set_env_cmd", { profile, key, value });
      setSavedKey(key);
      setTimeout(() => setSavedKey(null), 2000);
    } catch (err) {
      console.error("Failed to save env:", err);
    }
  }

  function handleChange(key: string, value: string) {
    setEnv((prev) => ({ ...prev, [key]: value }));
    const pending = envSaveTimers.current.get(key);
    if (pending) clearTimeout(pending);
    const timer = setTimeout(() => {
      envSaveTimers.current.delete(key);
      void invoke("set_env_cmd", { profile, key, value: env[key] || "" }).catch(() => {});
    }, 400);
    envSaveTimers.current.set(key, timer);
  }

  useEffect(() => { envRef.current = env; }, [env]);

  useEffect(() => {
    const timers = envSaveTimers.current;
    return () => {
      for (const [key, timer] of timers) {
        clearTimeout(timer);
        void invoke("set_env_cmd", { profile, key, value: envRef.current[key] || "" }).catch(() => {});
      }
      timers.clear();
    };
  }, [profile]);

  // Credential pool handlers
  async function handleAddPoolKey() {
    if (!poolProvider || !poolNewKey.trim()) return;
    try {
      await invoke("add_credential_pool_entry_cmd", {
        provider: poolProvider,
        key: poolNewKey.trim(),
        label: poolNewLabel.trim(),
      });
      // Reload pool
      const pool = await invoke<Record<string, CredentialPoolEntry[]>>("get_credential_pool_cmd");
      setCredPool(pool);
      setPoolNewKey("");
      setPoolNewLabel("");
    } catch (err) {
      console.error("Failed to add pool entry:", err);
    }
  }

  async function handleRemovePoolKey(provider: string, index: number) {
    const entries = [...(credPool[provider] || [])];
    entries.splice(index, 1);
    try {
      await invoke("set_credential_pool_cmd", { provider, entries });
      setCredPool((prev) => ({ ...prev, [provider]: entries }));
    } catch (err) {
      console.error("Failed to remove pool entry:", err);
    }
  }

  function toggleVisibility(key: string) {
    setVisibleKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key); else next.add(key);
      return next;
    });
  }

  const isCustomProvider = modelProvider === "custom";

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <h1 className="text-2xl font-bold text-ac-text mb-2">{t("providers.title")}</h1>
      <p className="text-ac-muted mb-6">{t("providers.subtitle")}</p>

      {/* Model Config */}
      <div className="mb-8">
        <div className="flex items-center gap-2 mb-4">
          <h2 className="text-lg font-semibold text-ac-text">{t("common.model")}</h2>
          {modelSaved && (
            <span className="text-xs text-green-500 flex items-center gap-1">
              <Check className="w-3 h-3" /> {t("common.saved")}
            </span>
          )}
        </div>

        <div className="space-y-4">
          {/* Provider */}
          <div>
            <label className="block text-sm font-medium text-ac-muted mb-1.5">
              {t("common.provider")}
            </label>
            <select
              className="w-full px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text"
              value={modelProvider}
              onChange={(e) => {
                const v = e.target.value;
                setModelProvider(v);
                if (v === "custom" && !modelBaseUrl) {
                  setModelBaseUrl("http://localhost:1234/v1");
                } else if (v !== "custom") {
                  setModelBaseUrl("");
                }
              }}
            >
              {PROVIDERS_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(opt.label)}
                </option>
              ))}
            </select>
            <p className="text-xs text-ac-muted mt-1">
              {isCustomProvider ? t("settings.customProviderHint") : t("settings.providerHint")}
            </p>
          </div>

          {/* Model name */}
          <div>
            <label className="block text-sm font-medium text-ac-muted mb-1.5">
              {t("common.model")}
            </label>
            <input
              className="w-full px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text"
              type="text"
              value={modelName}
              onChange={(e) => setModelName(e.target.value)}
              placeholder={t("settings.modelNamePlaceholder")}
            />
          </div>

          {/* Base URL (custom only) */}
          {isCustomProvider && (
            <div>
              <label className="block text-sm font-medium text-ac-muted mb-1.5">
                {t("common.baseUrl")}
              </label>
              <input
                className="w-full px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text"
                type="text"
                value={modelBaseUrl}
                onChange={(e) => setModelBaseUrl(e.target.value)}
                placeholder="http://localhost:1234/v1"
              />
            </div>
          )}
        </div>
      </div>

      {/* API Keys */}
      <div className="mb-8">
        <h2 className="text-lg font-semibold text-ac-text mb-4">{t("settings.sections.apiKeys")}</h2>
        <div className="space-y-3">
          {Object.entries(env).map(([key, value]) => (
            <div key={key} className="flex items-center gap-3">
              <label className="w-48 text-sm text-ac-muted font-mono">{key}</label>
              <div className="flex-1 relative">
                <input
                  className="w-full px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text pr-10"
                  type={visibleKeys.has(key) ? "text" : "password"}
                  value={value}
                  onChange={(e) => handleChange(key, e.target.value)}
                  onBlur={() => handleBlur(key)}
                />
                <button
                  className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-ac-muted hover:text-ac-text"
                  onClick={() => toggleVisibility(key)}
                >
                  {visibleKeys.has(key) ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                </button>
              </div>
              {savedKey === key && (
                <span className="text-xs text-green-500">{t("common.saved")}</span>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Credential Pool */}
      <div className="mb-8">
        <h2 className="text-lg font-semibold text-ac-text mb-4">
          {t("settings.sections.credentialPool")}
        </h2>
        <p className="text-sm text-ac-muted mb-4">{t("settings.poolHint")}</p>

        {/* Add new key */}
        <div className="flex items-center gap-2 mb-4">
          <select
            className="px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text"
            value={poolProvider}
            onChange={(e) => setPoolProvider(e.target.value)}
          >
            <option value="">{t("common.provider")}</option>
            {PROVIDERS_OPTIONS.filter((p) => p.value !== "auto").map((p) => (
              <option key={p.value} value={p.value}>{t(p.label)}</option>
            ))}
          </select>
          <input
            className="flex-1 px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text"
            type="password"
            value={poolNewKey}
            onChange={(e) => setPoolNewKey(e.target.value)}
            placeholder={t("settings.apiKeyPlaceholder")}
          />
          <input
            className="w-32 px-3 py-2 rounded-lg bg-ac-bg border border-border text-ac-text"
            type="text"
            value={poolNewLabel}
            onChange={(e) => setPoolNewLabel(e.target.value)}
            placeholder={t("settings.labelPlaceholder")}
          />
          <button
            className="px-4 py-2 rounded-lg bg-ac-blue text-white hover:bg-ac-blue/80 disabled:opacity-50 flex items-center gap-1"
            onClick={handleAddPoolKey}
            disabled={!poolProvider || !poolNewKey.trim()}
          >
            <Plus className="w-4 h-4" />
            {t("settings.add")}
          </button>
        </div>

        {/* Pool entries */}
        {Object.entries(credPool).map(([provider, entries]) =>
          entries.length > 0 && (
            <div key={provider} className="mb-4 p-4 rounded-xl bg-ac-bg border border-border">
              <div className="flex items-center gap-2 mb-3">
                <KeyRound className="w-4 h-4 text-ac-muted" />
                <span className="font-medium text-ac-text">
                  {PROVIDERS_OPTIONS.find((p) => p.value === provider)
                    ? t(PROVIDERS_OPTIONS.find((p) => p.value === provider)!.label)
                    : provider}
                </span>
              </div>
              {entries.map((entry, idx) => {
                const secret = entry.access_token || entry.api_key || entry.key || "";
                return (
                  <div key={entry.id || idx} className="flex items-center gap-3 py-2 border-t border-border">
                    <span className="text-sm text-ac-muted">
                      {entry.label || `${t("settings.keyLabel")} ${idx + 1}`}
                    </span>
                    <code className="flex-1 text-xs font-mono text-ac-text bg-ac-surface px-2 py-1 rounded">
                      {secret ? `${secret.slice(0, 8)}...${secret.slice(-4)}` : t("settings.empty")}
                    </code>
                    <button
                      className="p-1.5 rounded-lg text-red-400 hover:bg-red-500/10"
                      onClick={() => handleRemovePoolKey(provider, idx)}
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                );
              })}
            </div>
          )
        )}
      </div>

      {/* OAuth section */}
      <div className="mb-8">
        <h2 className="text-lg font-semibold text-ac-text mb-4">{t("providers.oauth.section")}</h2>
        <div className="grid grid-cols-2 gap-3">
          {OAUTH_PROVIDERS.map((provider) => (
            <button
              key={provider}
              className="flex items-center gap-3 p-4 rounded-xl bg-ac-bg border border-border hover:border-ac-blue transition-colors"
              onClick={() => setOauthModal(provider)}
            >
              <KeyRound className="w-5 h-5 text-ac-blue" />
              <span className="text-ac-text">{t(`providers.${provider}`)}</span>
            </button>
          ))}
        </div>
      </div>

      {/* OAuth Modal */}
      {oauthModal && (
        <OAuthLoginModal
          provider={oauthModal}
          providerLabel={t(`providers.${oauthModal}`)}
          profile={profile}
          onClose={() => setOauthModal(null)}
        />
      )}
    </div>
  );
}

export default ProvidersScreen;
