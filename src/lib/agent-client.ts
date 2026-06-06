/**
 * Agent client — communicates with the Python backend via Tauri IPC.
 *
 * The Python backend (tui_gateway.entry) runs as a child process.
 * Communication:
 *   - Frontend → invoke("send_rpc", {method, params}) → Rust → stdin → Python
 *   - Python → stdout → Rust → listen("agent_event") → Frontend
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AgentConfig, AgentEvent } from "./types";

export type { AgentConfig, AgentEvent };

export interface ConnectionInfo {
  mode: string;
  instance: string;
}

type JsonRpcParams = Record<string, unknown>;

export class AgentClient {
  private connected = false;
  private eventListeners: Array<(event: AgentEvent) => void> = [];
  private unlisten: UnlistenFn | null = null;

  async connect(config: AgentConfig): Promise<ConnectionInfo> {
    const info = await invoke<ConnectionInfo>("start_agent", { config });
    this.connected = true;

    // Subscribe to backend events
    this.unlisten = await listen<AgentEvent>("agent_event", (event) => {
      for (const listener of this.eventListeners) {
        try {
          listener(event.payload);
        } catch (err) {
          console.error("Event listener error:", err);
        }
      }
    });

    return info;
  }

  async disconnect(): Promise<void> {
    await invoke("stop_agent");
    this.connected = false;
    if (this.unlisten) {
      this.unlisten();
      this.unlisten = null;
    }
  }

  async call(method: string, params?: JsonRpcParams): Promise<unknown> {
    if (!this.connected) {
      throw new Error("Not connected to backend");
    }
    return invoke<unknown>("send_rpc", {
      method,
      params: params ?? {},
    });
  }

  onEvent(listener: (event: AgentEvent) => void): () => void {
    this.eventListeners.push(listener);
    return () => {
      const idx = this.eventListeners.indexOf(listener);
      if (idx >= 0) {
        this.eventListeners.splice(idx, 1);
      }
    };
  }

  get isConnected(): boolean {
    return this.connected;
  }
}
