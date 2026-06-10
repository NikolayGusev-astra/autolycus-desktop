// src/hooks/useDiscoveredModels.ts
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface DiscoveredModel {
  id: string;
  name: string;
  provider: string;
  description?: string;
  context_length?: number;
  pricing?: { prompt: string; completion: string };
}

export function useDiscoveredModels(provider?: string) {
  const [models, setModels] = useState<DiscoveredModel[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const discover = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (provider) {
        const result = await invoke<DiscoveredModel[]>("discover_models_cmd", { provider });
        setModels(result);
      } else {
        // Discover from all providers
        const providers = ["openrouter", "openai", "anthropic", "ollama", "lmstudio"];
        const results = await Promise.allSettled(
          providers.map((p) => invoke<DiscoveredModel[]>("discover_models_cmd", { provider: p }))
        );
        const allModels: DiscoveredModel[] = [];
        for (const r of results) {
          if (r.status === "fulfilled") {
            allModels.push(...r.value);
          }
        }
        setModels(allModels);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [provider]);

  useEffect(() => {
    void discover();
  }, [discover]);

  return { models, loading, error, refetch: discover };
}
