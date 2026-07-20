/**
 * Telegram Service - Typed wrapper for telegram commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const telegramService = {
  // Send telegram message
  async sendTelegramMessage(chatId: string, text: string): Promise<void> {
    return invoke("send_telegram_message_cmd", { chatId, text });
  },

  // Validate telegram bot token
  async validateTelegramBotToken(token: string): Promise<boolean> {
    return invoke("validate_telegram_bot_token_cmd", { token });
  },

  // Save telegram config
  async saveTelegramConfig(config: { botToken: string; chatId: string; enabled: boolean }): Promise<void> {
    return invoke("save_telegram_config_cmd", { config });
  },

  // Load telegram config
  async loadTelegramConfig(): Promise<{ botToken: string; chatId: string; enabled: boolean } | null> {
    return invoke("load_telegram_config_cmd");
  },
};