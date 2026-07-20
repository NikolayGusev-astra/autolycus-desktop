/**
 * Versions Service - Typed wrapper for version commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const versionsService = {
  // Get app version
  async getAppVersion(): Promise<string> {
    return invoke("get_app_version");
  },

  // Get versions
  async getVersions(): Promise<{
    app: string;
    tauri: string;
    hermes: string;
  }> {
    return invoke("get_versions_cmd");
  },

  // Detect instances
  async detectInstances(): Promise<Array<{
    name: string;
    path: string;
    version: string;
  }>> {
    return invoke("detect_instances");
  },

  // Check python path
  async checkPythonPath(): Promise<string | null> {
    return invoke("check_python_path");
  },
};