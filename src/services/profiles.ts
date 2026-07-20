/**
 * Profiles Service - Typed wrapper for profile-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const profilesService = {
  // List all profiles
  async listProfiles(): Promise<Array<{ name: string; isActive: boolean; model?: string }>> {
    return invoke("list_profiles_cmd");
  },

  // Create a new profile
  async createProfile(name: string, cloneFrom?: string): Promise<void> {
    return invoke("create_profile_cmd", { name, clone: cloneFrom });
  },

  // Delete a profile
  async deleteProfile(name: string): Promise<void> {
    return invoke("delete_profile_cmd", { name });
  },

  // Set active profile
  async setActiveProfile(name: string): Promise<void> {
    return invoke("set_active_profile_cmd", { name });
  },
};