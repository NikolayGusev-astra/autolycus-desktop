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

export interface GatewayStatus {
  connected: boolean;
  model?: string;
  tokensUsed?: number;
  tokensLimit?: number;
  costUsd?: number;
}

interface ConnectionStore {
  config: ConnectionConfig;
  loading: boolean;
  error: string | null;
  gatewayStatus: GatewayStatus;
  loadConfig: () => Promise<void>;
  saveConfig: (config: Partial<ConnectionConfig>) => Promise<boolean>;
  testConnection: (mode: ConnectionMode, url?: string, ssh?: SshConfig) => Promise<boolean>;
  fetchGatewayStatus: () => Promise<void>;
  setGatewayStatus: (status: GatewayStatus) => void;
}

const DEFAULT_SSH: SshConfig = {
  host: "",
  port: 22,
  username: "",
  key_path: "",
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

const DEFAULT_GATEWAY_STATUS: GatewayStatus = {
  connected: false,
  model: undefined,
  tokensUsed: undefined,
  tokensLimit: undefined,
  costUsd: undefined,
};

export const useConnectionStore = create<ConnectionStore>((set, get) => ({
  config: DEFAULT_CONFIG,
  loading: false,
  error: null,
  gatewayStatus: DEFAULT_GATEWAY_STATUS,

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

  fetchGatewayStatus: async () => {
    try {
      const status = await invoke<{
        model?: string;
        tokens_used?: number;
        tokens_limit?: number;
        cost_usd?: number;
      }>("gateway_status_cmd");
      if (status) {
        set({
          gatewayStatus: {
            connected: true,
            model: status.model,
            tokensUsed: status.tokens_used,
            tokensLimit: status.tokens_limit,
            costUsd: status.cost_usd,
          },
        });
      }
    } catch {
      // gateway_status_cmd may not exist yet
    }
  },

  setGatewayStatus: (gatewayStatus) => set({ gatewayStatus }),
}));