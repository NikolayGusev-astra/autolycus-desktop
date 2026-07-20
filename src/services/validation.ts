/**
 * Validation Service - Typed wrapper for validation commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const validationService = {
  // Validate chat readiness
  async validateChatReadiness(): Promise<{
    ready: boolean;
    code?: string;
    message?: string;
    fixes?: Array<{ code: string; description: string; location: string }>;
  }> {
    return invoke("validate_chat_readiness_cmd");
  },
};