/**
 * Terminal Service - Typed wrapper for terminal commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const terminalService = {
  // Open terminal
  async openTerminal(command?: string, cwd?: string): Promise<void> {
    return invoke("open_terminal_cmd", { command, cwd });
  },
};