/**
 * Gateway Service - Typed wrapper for gateway-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const gatewayService = {
  // Start the local gateway
  async startGateway(): Promise<{ port: number; token: string }> {
    return invoke("start_gateway_cmd");
  },

  // Stop the local gateway
  async stopGateway(): Promise<void> {
    return invoke("stop_gateway_cmd");
  },

  // Check gateway status
  async getGatewayStatus(): Promise<boolean> {
    return invoke("gateway_status_cmd");
  },

  // Get gateway port
  async getGatewayPort(): Promise<number | null> {
    return invoke("get_gateway_port_cmd");
  },

  // List available models from API
  async listModelsApi(): Promise<Array<{ id: string; name: string; provider: string }>> {
    return invoke("list_models_api_cmd");
  },
};