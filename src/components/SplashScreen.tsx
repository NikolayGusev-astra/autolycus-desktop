// src/components/SplashScreen.tsx
import { useEffect, useRef } from "react";
import { Bot } from "lucide-react";
import { useTranslation } from "../hooks/useTranslation";

interface SplashScreenProps {
  onComplete: (autoconnect: boolean) => void;
}

export function SplashScreen({ onComplete }: SplashScreenProps) {
  const calledRef = useRef(false);
  const { t } = useTranslation();

  useEffect(() => {
    // Check localStorage for autoconnect preference
    let autoconnect = false;
    try {
      autoconnect = localStorage.getItem("autolycus_autoconnect") === "true";
    } catch {
      // localStorage might not be available in some contexts
    }

    const delay = autoconnect ? 800 : 2000;

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
    <div className="fixed inset-0 bg-ac-pitch flex flex-col items-center justify-center">
      {/* Logo animation */}
      <div className="mb-6 opacity-0 animate-fade-in">
        <div className="w-20 h-20 rounded-2xl bg-ac-amber/10 border border-ac-amber/20 flex items-center justify-center">
          <Bot className="w-10 h-10 text-ac-amber" />
        </div>
      </div>

      {/* Title */}
      <h1 className="text-3xl font-bold text-ac-ivory tracking-tight opacity-0 animate-fade-in-delay-1">
        Autolycus
      </h1>

      {/* Subtitle */}
      <p className="mt-2 text-sm text-ac-stone/60 opacity-0 animate-fade-in-delay-2">
        {t("splash.subtitle")}
      </p>

      {/* Loading indicator */}
      <div className="mt-10 opacity-0 animate-fade-in-delay-3">
        <div className="w-5 h-5 border-2 border-ac-amber/30 border-t-ac-amber rounded-full animate-spin" />
      </div>
    </div>
  );
}