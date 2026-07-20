/**
 * Connection Service - Typed wrapper for connection/SSH-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ConnectionConfig, SshConfig } from "@/lib/types";

export const connectionService = {
  // Get current connection config
  async getConnectionConfig(): Promise<ConnectionConfig> {
    return invoke("get_connection_config");
  },

  // Set connection config
  async setConnectionConfig(config: ConnectionConfig): Promise<void> {
    return invoke("set_connection_config", { config });
  },

  // Test connection
  async testConnection(config: ConnectionConfig): Promise<{ success: boolean; error?: string }> {
    return invoke("test_connection", { config });
  },

  // Start SSH tunnel
  async startSshTunnel(sshConfig: SshConfig): Promise<number> {
    return invoke("start_ssh_tunnel_cmd", { sshConfig });
  },

  // Stop SSH tunnel
  async stopSshTunnel(): Promise<void> {
    return invoke("stop_ssh_tunnel_cmd");
  },

  // Get SSH tunnel status
  async getSshTunnelStatus(): Promise<{ running: boolean; port?: number }> {
    return invoke("ssh_tunnel_status_cmd");
  },

  // Start remote gateway via SSH
  async startRemoteGateway(): Promise<{ url: string; token: string }> {
    return invoke("start_remote_gateway_cmd");
  },

  // Detect local instances
  async detectLocalInstances(): Promise<Array<{ name: string; path: string; version: string }>> {
    return invoke("detect_local_instances_cmd");
  },

  // Detect remote instances
  async detectRemoteInstances(): Promise<Array<{ name: string; url: string }>> {
    return invoke("detect_remote_instances_cmd");
  },

  // Connect to instance
  async connectToInstance(name: string): Promise<void> {
    return invoke("connect_to_instance", { name });
  },

  // Auto connect to local
  async autoConnectLocal(): Promise<void> {
    return invoke("auto_connect_local_cmd");
  },
};