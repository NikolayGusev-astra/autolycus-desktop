// src/stores/connectionStore.ts
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type ConnectionMode = "local" | "remote" | "ssh";

export interface SshConfig {
  host: string;
  port: number;
  username: string;
  key_path: string;
  remote_port: number;
  local_port: number;
}

export interface ConnectionConfig {
  mode: ConnectionMode;
  remote_url: string;
  has_api_key: boolean;
  api_key_length: number;
  ssh: SshConfig;
}

interface ConnectionStore {
  config: ConnectionConfig;
  loading: boolean;
  error: string | null;
  loadConfig: () => Promise<void>;
  saveConfig: (config: Partial<ConnectionConfig>) => Promise<boolean>;
  testConnection: (mode: ConnectionMode, url?: string, ssh?: SshConfig) => Promise<boolean>;
}

const DEFAULT_SSH: SshConfig = {
  host: "",
  port: 22,
  username: "root",
  key_path: "~/.ssh/id_rsa",
  remote_port: 8642,
  local_port: 18642,
};

const DEFAULT_CONFIG: ConnectionConfig = {
  mode: "local",
  remote_url: "",
  has_api_key: false,
  api_key_length: 0,
  ssh: DEFAULT_SSH,
};

export const useConnectionStore = create<ConnectionStore>((set, get) => ({
  config: DEFAULT_CONFIG,
  loading: false,
  error: null,

  loadConfig: async () => {
    try {
      set({ loading: true, error: null });
      const cfg = await invoke<ConnectionConfig>("get_connection_config");
      set({ config: cfg, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },

  saveConfig: async (partial) => {
    try {
      const current = get().config;
      const merged = { ...current, ...partial };

      await invoke("set_connection_config", {
        mode: merged.mode,
        remoteUrl: merged.remote_url,
        apiKey: "", // Don't expose API key to frontend
        sshHost: merged.ssh.host,
        sshPort: merged.ssh.port,
        sshUsername: merged.ssh.username,
        sshKeyPath: merged.ssh.key_path,
        sshRemotePort: merged.ssh.remote_port,
        sshLocalPort: merged.ssh.local_port,
      });

      set({ config: merged });
      return true;
    } catch (err) {
      set({ error: String(err) });
      return false;
    }
  },

  testConnection: async (mode, url, ssh) => {
    try {
      return await invoke<boolean>("test_connection", {
        mode,
        url: url || "",
        sshConfig: ssh || DEFAULT_SSH,
      });
    } catch {
      return false;
    }
  },
}));
