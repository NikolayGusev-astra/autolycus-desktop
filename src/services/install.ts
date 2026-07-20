/**
 * Install Service - Typed wrapper for install commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const installService = {
  // Install Hermes
  async installHermes(): Promise<{ success: boolean; path?: string; error?: string }> {
    return invoke("install_hermes_cmd");
  },
};