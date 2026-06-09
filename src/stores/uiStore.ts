import { create } from "zustand";
import type { Lang } from "../lib/i18n";

interface UIState {
  sidebarOpen: boolean;
  sidebarWidth: number;
  darkMode: boolean;
  settingsOpen: boolean;
  language: Lang;
  toggleSidebar: () => void;
  setSidebarOpen: (open: boolean) => void;
  toggleDarkMode: () => void;
  setSettingsOpen: (open: boolean) => void;
  setLanguage: (lang: Lang) => void;
  toggleLanguage: () => void;
}

export const useUIStore = create<UIState>((set) => ({
  sidebarOpen: true,
  sidebarWidth: 280,
  darkMode: window.matchMedia("(prefers-color-scheme: dark)").matches,
  settingsOpen: false,
  language: "ru",
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
  toggleDarkMode: () => set((s) => ({ darkMode: !s.darkMode })),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  setLanguage: (lang) => set({ language: lang }),
  toggleLanguage: () =>
    set((s) => ({ language: s.language === "en" ? "ru" : "en" })),
}));