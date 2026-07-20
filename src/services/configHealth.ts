/**
 * Config Health Service - Typed wrapper for config health commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const configHealthService = {
  // Run config health check
  async configHealthCheck(): Promise<{
    issues: Array<{ code: string; message: string; severity: "error" | "warning" | "info" }>;
    summary: { total: number; errors: number; warnings: number; info: number };
  }> {
    return invoke("config_health_check_cmd");
  },

  // Auto fix config
  async autoFixConfig(code: string): Promise<{ fixed: boolean; message: string }> {
    return invoke("auto_fix_config_cmd", { code });
  },
};