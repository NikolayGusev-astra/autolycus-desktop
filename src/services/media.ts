// src/services/media.ts
// Typed service layer for media-related Tauri commands
// Part of the anti-corruption layer per ADR-001 Phase 0

import { invoke } from "@tauri-apps/api/core";

export interface MediaInfo {
  id: string;
  filename: string;
  mimeType: string;
  size: number;
  createdAt: number;
}

// Media service - wraps all media-related Tauri commands
export const mediaService = {
  // Get media info
  async getMediaInfo(id: string): Promise<MediaInfo | null> {
    return invoke("get_media_info_cmd", { id });
  },

  // Read media as data URL
  async readMediaDataUrl(id: string): Promise<string> {
    return invoke("read_media_data_url_cmd", { id });
  },

  // List media files
  async listMediaFiles(): Promise<MediaInfo[]> {
    return invoke("list_media_files_cmd");
  },

  // Save media blob (Uint8Array)
  async saveMediaBlob(blob: Uint8Array, extension: string): Promise<string> {
    return invoke("save_media_blob_cmd", { blob, extension });
  },

  // Save media file from path
  async saveMediaFile(path: string, extension: string): Promise<string> {
    return invoke("save_media_file_cmd", { path, extension });
  },
};