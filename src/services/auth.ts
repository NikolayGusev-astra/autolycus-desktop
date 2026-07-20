/**
 * Auth Service - Typed wrapper for auth-related commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const authService = {
  // Login
  async authLogin(provider: string): Promise<{ url: string }> {
    return invoke("auth_login_cmd", { provider });
  },

  // Cancel auth
  async authCancel(): Promise<void> {
    return invoke("auth_cancel_cmd");
  },
};