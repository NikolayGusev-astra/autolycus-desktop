/**
 * Registry Service - Typed wrapper for registry-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const registryService = {
  // Fetch registry catalog
  async fetchRegistryCatalog(): Promise<Array<{
    id: string;
    name: string;
    version: string;
    description: string;
    author: string;
    url: string;
    tags: string[];
  }>> {
    return invoke("fetch_registry_catalog_cmd");
  },

  // Get installed registry
  async getInstalledRegistry(): Promise<Array<{
    id: string;
    name: string;
    version: string;
    enabled: boolean;
  }>> {
    return invoke("get_installed_registry_cmd");
  },

  // Install from registry
  async installFromRegistry(id: string, version?: string): Promise<void> {
    return invoke("install_from_registry_cmd", { id, version });
  },
};