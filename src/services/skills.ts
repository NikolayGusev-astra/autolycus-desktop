/**
 * Skills Service - Typed wrapper for skills-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const skillsService = {
  // List installed skills
  async listInstalledSkills(): Promise<Array<{ name: string; version: string; enabled: boolean }>> {
    return invoke("list_installed_skills_cmd");
  },

  // Get skill content
  async getSkillContent(name: string): Promise<string | null> {
    return invoke("get_skill_content_cmd", { name });
  },

  // Install skill
  async installSkill(name: string, source: string): Promise<void> {
    return invoke("install_skill_cmd", { name, source });
  },

  // Uninstall skill
  async uninstallSkill(name: string): Promise<void> {
    return invoke("uninstall_skill_cmd", { name });
  },
};