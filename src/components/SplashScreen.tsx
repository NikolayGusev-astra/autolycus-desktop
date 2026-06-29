// src/components/SplashScreen.tsx
import { useEffect, useRef } from "react";
import { useTranslation } from "../hooks/useTranslation";
import logo from "../assets/logo.png";

interface SplashScreenProps {
  onComplete: (autoconnect: boolean) => void;
}

export function SplashScreen({ onComplete }: SplashScreenProps) {
  const calledRef = useRef(false);
  const { t } = useTranslation();

  useEffect(() => {
    // Auto-connect is on by default (ADR-003): we try to discover and adopt
    // the local Hermes instance so the user lands in chat directly. Only an
    // explicit "false" in localStorage disables it (set after a manual
    // disconnect). This delivers the shturman.ai "just works" startup.
    let autoconnect = true;
    try {
      const stored = localStorage.getItem("steersman_autoconnect");
      if (stored !== null) autoconnect = stored === "true";
    } catch {
      // localStorage might not be available in some contexts
    }

    // Give the theme + window a moment to settle before kicking off the
    // (possibly slow) auto-discovery in the parent.
    const delay = autoconnect ? 900 : 1800;

    const timer = setTimeout(() => {
      if (!calledRef.current) {
        calledRef.current = true;
        onComplete(autoconnect);
      }
    }, delay);

    return () => {
      clearTimeout(timer);
    };
  }, [onComplete]);

  return (
    <div className="fixed inset-0 bg-ac-bg flex flex-col items-center justify-center">
      {/* Logo animation */}
      <div className="mb-6 opacity-0 animate-fade-in">
        <img
          src={logo}
          alt="Штурман"
          className="w-20 h-20 rounded-2xl shadow-sm"
          draggable={false}
        />
      </div>

      {/* Title */}
      <h1 className="text-3xl font-semibold text-ac-ink tracking-tight opacity-0 animate-fade-in-delay-1">
        Штурман
      </h1>

      {/* Subtitle */}
      <p className="mt-2 text-sm text-ac-muted opacity-0 animate-fade-in-delay-2">
        {t("splash.subtitle")}
      </p>

      {/* Loading indicator */}
      <div className="mt-10 opacity-0 animate-fade-in-delay-3">
        <div className="w-5 h-5 border-2 border-ac-brand/30 border-t-ac-brand rounded-full animate-spin" />
      </div>
    </div>
  );
}