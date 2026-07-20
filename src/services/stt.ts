/**
 * STT Service - Typed wrapper for speech-to-text commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const sttService = {
  // Transcribe audio
  async transcribeAudio(audioData: ArrayBuffer, options?: { language?: string; model?: string }): Promise<string> {
    return invoke("transcribe_audio_cmd", { audioData, options });
  },
};