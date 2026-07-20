/**
 * Onboarding Service - Typed wrapper for onboarding commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const onboardingService = {
  // Save provider key
  async saveProviderKey(provider: string, key: string): Promise<void> {
    return invoke("save_provider_key_cmd", { provider, key });
  },

  // Write soul
  async writeSoul(content: string): Promise<void> {
    return invoke("write_soul_cmd", { content });
  },

  // Set personality
  async setPersonality(personality: string): Promise<void> {
    return invoke("set_personality_cmd", { personality });
  },

  // Get personalities
  async getPersonalities(): Promise<string[]> {
    return invoke("get_personalities_cmd");
  },

  // Get personality
  async getPersonality(): Promise<string | null> {
    return invoke("get_personality_cmd");
  },

  // Read soul
  async readSoul(): Promise<string> {
    return invoke("read_soul_cmd");
  },

  // Reset soul
  async resetSoul(): Promise<void> {
    return invoke("reset_soul_cmd");
  },
};