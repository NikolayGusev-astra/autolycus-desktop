/**
 * Models Service - Typed wrapper for model-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const modelsService = {
  // List models
  async listModels(): Promise<Array<{ id: string; name: string; provider: string; config: Record<string, unknown> }>> {
    return invoke("list_models_cmd");
  },

  // Add model
  async addModel(model: { id: string; name: string; provider: string; config: Record<string, unknown> }): Promise<void> {
    return invoke("add_model_cmd", { model });
  },

  // Remove model
  async removeModel(id: string): Promise<void> {
    return invoke("remove_model_cmd", { id });
  },

  // Update model
  async updateModel(model: { id: string; name: string; provider: string; config: Record<string, unknown> }): Promise<void> {
    return invoke("update_model_cmd", { model });
  },

  // Get model config
  async getModelConfig(id: string): Promise<Record<string, unknown> | null> {
    return invoke("get_model_config_cmd", { id });
  },

  // Set model config
  async setModelConfig(id: string, config: Record<string, unknown>): Promise<void> {
    return invoke("set_model_config_cmd", { id, config });
  },

  // Set default model
  async setDefaultModel(id: string): Promise<void> {
    return invoke("set_default_model_cmd", { id });
  },

  // Set model routing
  async setModelRouting(routing: Array<{ pattern: string; model: string }>): Promise<void> {
    return invoke("set_model_routing_cmd", { routing });
  },

  // Get provider base URL
  async getProviderBaseUrl(provider: string): Promise<string | null> {
    return invoke("get_provider_base_url_cmd", { provider });
  },

  // Get all provider URLs
  async getAllProviderUrls(): Promise<Record<string, string>> {
    return invoke("get_all_provider_urls_cmd");
  },

  // List providers
  async listProviders(): Promise<Array<{ name: string; baseUrl: string; models: string[] }>> {
    return invoke("list_providers_cmd");
  },

  // Discover models
  async discoverModels(provider: string, baseUrl: string, apiKey?: string): Promise<Array<{ id: string; name: string }>> {
    return invoke("discover_models_cmd", { provider, baseUrl, apiKey });
  },

  // Check if discoverable
  async isDiscoverable(provider: string): Promise<boolean> {
    return invoke("is_discoverable_cmd", { provider });
  },

  // Get OAuth models
  async getOAuthModels(provider: string): Promise<Array<{ id: string; name: string }>> {
    return invoke("get_oauth_models_cmd", { provider });
  },
};