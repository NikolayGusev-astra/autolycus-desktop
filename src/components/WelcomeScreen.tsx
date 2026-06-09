import { Bot, ArrowRight, Cpu, Wifi, Terminal, FolderOpen } from "lucide-react";
import { useTranslation } from "../hooks/useTranslation";

interface DetectedInstance {
  path: string;
  instance_type: string;
  version: string;
  gateway_running: boolean;
  gateway_port: number | null;
  active_profile: string;
}

interface WelcomeScreenProps {
  onGetStarted: () => void;
  detectedInstances?: DetectedInstance[];
}

export function WelcomeScreen({ onGetStarted, detectedInstances }: WelcomeScreenProps) {
  const { t } = useTranslation();

  return (
    <div className="fixed inset-0 bg-ac-pitch flex items-center justify-center">
      <div className="w-full max-w-md px-6 text-center">
        {/* Logo */}
        <div className="flex justify-center mb-6">
          <div className="w-16 h-16 rounded-2xl bg-ac-amber/10 border border-ac-amber/20 flex items-center justify-center">
            <Bot className="w-8 h-8 text-ac-amber" />
          </div>
        </div>

        {/* Title */}
        <h1 className="text-2xl font-semibold text-ac-ivory mb-2">
          {t("welcome.title")}
        </h1>

        {/* Subtitle */}
        <p className="text-sm text-ac-stone mb-6 leading-relaxed">
          {t("welcome.subtitle")}
        </p>

        {/* Detected local instances */}
        {detectedInstances && detectedInstances.length > 0 && (
          <div className="mb-6 text-left">
            <label className="text-[11px] text-ac-stone mb-2 block text-center">
              {t("welcome.detectedInstances")}
            </label>
            <div className="space-y-1.5 max-h-32 overflow-y-auto">
              {detectedInstances.map((inst, idx) => (
                <div
                  key={idx}
                  className="px-3 py-2 text-xs border border-ac-border/50 rounded"
                >
                  <div className="flex items-center gap-2">
                    <Cpu className="w-3 h-3 text-ac-amber flex-shrink-0" />
                    <span className="font-medium text-ac-ivory truncate">
                      {inst.instance_type}
                    </span>
                    <span className="text-[10px] text-ac-stone ml-auto">
                      {inst.version}
                    </span>
                  </div>
                  <div className="flex items-center gap-2 mt-1 text-[10px] text-ac-stone/70">
                    <FolderOpen className="w-2.5 h-2.5" />
                    <span className="truncate">{inst.path}</span>
                  </div>
                  <div className="flex items-center gap-3 mt-1 text-[10px]">
                    {inst.gateway_running ? (
                      <span className="text-green-500 flex items-center gap-1">
                        <Wifi className="w-2.5 h-2.5" /> {t("welcome.gatewayRunning")} {inst.gateway_port}
                      </span>
                    ) : (
                      <span className="text-ac-stone/50 flex items-center gap-1">
                        <Terminal className="w-2.5 h-2.5" /> {t("welcome.gatewayOffline")}
                      </span>
                    )}
                    <span className="text-ac-stone/50">{t("welcome.profile")} {inst.active_profile}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Get Started button */}
        <button
          onClick={onGetStarted}
          className="ac-btn inline-flex items-center gap-2 px-8 py-3 text-sm font-medium"
        >
          {t("btn.getStarted")}
          <ArrowRight className="w-4 h-4" />
        </button>

        {/* Footer */}
        <p className="mt-8 text-[11px] text-ac-stone/40">
          Autolycus Desktop v0.5.0
        </p>
      </div>
    </div>
  );
}