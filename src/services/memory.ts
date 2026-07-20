/**
 * Memory Service - Typed wrapper for memory-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const memoryService = {
  // Read user profile
  async readMemory(profile?: string): Promise<string> {
    return invoke("read_memory_cmd", { profile });
  },

  // Write user profile
  async writeUserProfile(content: string, profile?: string): Promise<void> {
    return invoke("write_user_profile_cmd", { content, profile });
  },

  // Add memory entry
  async addMemoryEntry(key: string, value: string, namespace?: string): Promise<void> {
    return invoke("add_memory_entry_cmd", { key, value, namespace });
  },

  // Update memory entry
  async updateMemoryEntry(key: string, value: string, namespace?: string): Promise<void> {
    return invoke("update_memory_entry_cmd", { key, value, namespace });
  },

  // Remove memory entry
  async removeMemoryEntry(key: string, namespace?: string): Promise<void> {
    return invoke("remove_memory_entry_cmd", { key, namespace });
  },
};