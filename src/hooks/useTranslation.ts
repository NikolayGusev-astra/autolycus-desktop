// src/hooks/useTranslation.ts
// Phase 8a: i18n hook — returns t(key) function based on current language from uiStore
// Phase 8b: t() now has smart fallback: if the key isn't in the dictionary,
// tries PROVIDERS.labels (for provider ids like "openrouter", "kilo"),
// then humanizes the key. Fixes raw keys like "settings.providerHint" leaking through.

import { useUIStore } from '../stores/uiStore';
import { t as dictT } from '../lib/i18n';
import { PROVIDERS } from '../constants';

function humanize(key: string): string {
  // Strip leading namespace: "settings.providerHint" -> "Provider Hint"
  // "feed.update" -> "Update"
  const parts = key.split('.');
  const tail = parts[parts.length - 1];
  // camelCase -> "Camel Case"
  return tail
    .replace(/([A-Z])/g, ' $1')
    .replace(/^./, (c) => c.toUpperCase())
    .trim();
}

function fallback(key: string): string | null {
  // Strip "providers." prefix: "providers.kilo" -> try PROVIDERS.labels["kilo"]
  if (key.startsWith('providers.')) {
    const providerId = key.slice('providers.'.length);
    if (PROVIDERS.labels[providerId]) return PROVIDERS.labels[providerId];
  }
  // Bare provider id: "openrouter" -> "OpenRouter"
  if (PROVIDERS.labels[key]) return PROVIDERS.labels[key];
  // Generic humanize: "settings.providerHint" -> "Provider Hint"
  return humanize(key);
}

export function useTranslation() {
  const lang = useUIStore((s) => s.language);
  return {
    t: (key: string, params?: Record<string, string | number>) => {
      const result = dictT(key, lang, params);
      // If dictionary returned the key back (no translation found), try fallback
      if (result === key) {
        return fallback(key) ?? key;
      }
      return result;
    },
    lang,
  };
}