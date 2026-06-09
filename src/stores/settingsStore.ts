// src/stores/settingsStore.ts
// Phase 1: Settings connected to Rust via invoke
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

// ── Rust type mirrors ──────────────────────────────────────────────────────

export interface SavedModel {
  id: string;
  name: string;
  provider: string;
  model: string;
  base_url: string;
  api_mode: string | null;
  created_at: number;
}

export interface ModelConfig {
  provider: string;
  model: string;
  base_url: string;
}

interface GeneralInfo {
  version: string;
  hermes_home: string;
}

// ── Store ──────────────────────────────────────────────────────────────────

interface SettingsStore {
  // General
  generalInfo: GeneralInfo | null;
  generalLoading: boolean;
  generalError: string | null;

  // Models
  models: SavedModel[];
  modelConfig: ModelConfig | null;
  modelsLoading: boolean;
  modelsError: string | null;

  // Actions
  loadGeneralInfo: () => Promise<void>;
  loadModels: () => Promise<void>;
  loadModelConfig: () => Promise<void>;
  addModel: (name: string, provider: string, model: string, base_url: string) => Promise<SavedModel | null>;
  removeModel: (id: string) => Promise<boolean>;
  setActiveModel: (provider: string, model: string, base_url: string) => Promise<boolean>;
  setTheme: (dark: boolean) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  // General
  generalInfo: null,
  generalLoading: false,
  generalError: null,

  // Models
  models: [],
  modelConfig: null,
  modelsLoading: false,
  modelsError: null,

  // ── General ──────────────────────────────────────────────────────────────

  loadGeneralInfo: async () => {
    set({ generalLoading: true, generalError: null });
    try {
      const version = await invoke<string>("get_app_version");
      const initResult = await invoke<{ hermes_home: string; version: string }>("init_app");
      set({
        generalInfo: { version, hermes_home: initResult.hermes_home },
        generalLoading: false,
      });
    } catch (err) {
      set({ generalError: String(err), generalLoading: false });
    }
  },

  setTheme: async (dark: boolean) => {
    try {
      await invoke("set_env_cmd", {
        key: "AUTOLYCUS_THEME",
        value: dark ? "dark" : "light",
        profile: null as string | null,
      });
    } catch (err) {
      console.error("Failed to save theme:", err);
    }
  },

  // ── Models ───────────────────────────────────────────────────────────────

  loadModels: async () => {
    set({ modelsLoading: true, modelsError: null });
    try {
      const models = await invoke<SavedModel[]>("list_models_cmd");
      set({ models, modelsLoading: false });
    } catch (err) {
      set({ modelsError: String(err), modelsLoading: false });
    }
  },

  loadModelConfig: async () => {
    try {
      const config = await invoke<ModelConfig>("get_model_config_cmd", {
        profile: null as string | null,
      });
      set({ modelConfig: config });
    } catch (err) {
      console.error("Failed to load model config:", err);
    }
  },

  addModel: async (name, provider, model, base_url) => {
    try {
      const saved = await invoke<SavedModel>("add_model_cmd", {
        name,
        provider,
        model,
        baseUrl: base_url,
      });
      // Reload models to get fresh list
      const models = await invoke<SavedModel[]>("list_models_cmd");
      set({ models });
      return saved;
    } catch (err) {
      console.error("Failed to add model:", err);
      return null;
    }
  },

  removeModel: async (id) => {
    try {
      const result = await invoke<boolean>("remove_model_cmd", { id });
      if (result) {
        const models = await invoke<SavedModel[]>("list_models_cmd");
        set({ models });
      }
      return result;
    } catch (err) {
      console.error("Failed to remove model:", err);
      return false;
    }
  },

  setActiveModel: async (provider, model, base_url) => {
    try {
      await invoke("set_model_config_cmd", {
        provider,
        model,
        baseUrl: base_url,
        profile: null as string | null,
      });
      set({ modelConfig: { provider, model, base_url } });
      return true;
    } catch (err) {
      console.error("Failed to set active model:", err);
      return false;
    }
  },
}));