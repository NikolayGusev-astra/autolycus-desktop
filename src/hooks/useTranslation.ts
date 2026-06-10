// src/hooks/useTranslation.ts
// Phase 8a: i18n hook — returns t(key) function based on current language from uiStore

import { useUIStore } from '../stores/uiStore';
import { t } from '../lib/i18n';

export function useTranslation() {
  const lang = useUIStore((s) => s.language);
  return { t: (key: string, _params?: unknown) => t(key, lang), lang };
}