// src/services/config.ts
// Typed service layer for config/env-related Tauri commands
// Part of the anti-corruption layer per ADR-001 Phase 0

import { invoke } from "@tauri-apps/api/core";

// Config service - wraps all config/env-related Tauri commands
export const configService = {
  // Get env variable
  async getEnv(key: string, profile?: string | null): Promise<string | null> {
    return invoke("get_env_cmd", { key, profile });
  },

  // Set env variable
  async setEnv(key: string, value: string, profile?: string | null): Promise<void> {
    return invoke("set_env_cmd", { key, value, profile });
  },

  // Get config section
  async getConfigSection(section: string): Promise<Record<string, unknown>> {
    return invoke("get_config_section_cmd", { section });
  },

  // Set config YAML value
  async setConfigYamlValue(key: string, value: unknown): Promise<void> {
    return invoke("set_config_yaml_value_cmd", { key, value });
  },

  // Set model routing
  async setModelRouting(routing: Record<string, string>, profile?: string | null): Promise<void> {
    return invoke("set_model_routing_cmd", { routing, profile });
  },
};