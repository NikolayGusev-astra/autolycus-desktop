import {
  Bot,
  ArrowRight,
  Cpu,
  Wifi,
  Terminal,
  FolderOpen,
  Plug,
} from "lucide-react";
import { useTranslation } from "../hooks/useTranslation";

interface DetectedInstance {
  path: string;
  instance_type: string;
  version: string;
  gateway_running: boolean;
  gateway_port: number | null;
  active_profile: string;
  home_dir?: string;
  label?: string;
}

interface WelcomeScreenProps {
  onGetStarted: () => void;
  detectedInstances?: DetectedInstance[];
  /** App version, sourced from the backend (Cargo pkg version). */
  appVersion?: string;
  /** Connect to a discovered instance, adopting its environment. */
  onConnectInstance?: (instance: DetectedInstance) => void;
}

export function WelcomeScreen({
  onGetStarted,
  detectedInstances,
  appVersion,
  onConnectInstance,
}: WelcomeScreenProps) {
  const { t } = useTranslation();
  const versionLabel = appVersion ? `v${appVersion}` : "";

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

        {/* Detected local instances — offer to adopt an environment */}
        {detectedInstances && detectedInstances.length > 0 && (
          <div className="mb-6 text-left">
            <label className="text-[11px] text-ac-stone mb-2 block text-center">
              {t("welcome.detectedInstances")}
            </label>
            <div className="space-y-1.5 max-h-48 overflow-y-auto">
              {detectedInstances.map((inst, idx) => {
                const connectable = !!inst.home_dir && !!onConnectInstance;
                return (
                  <button
                    key={idx}
                    type="button"
                    disabled={!connectable}
                    onClick={() => connectable && onConnectInstance!(inst)}
                    className={`w-full text-left px-3 py-2 text-xs border rounded transition-colors ${
                      connectable
                        ? "border-ac-amber/30 hover:border-ac-amber/60 hover:bg-ac-amber/5 cursor-pointer"
                        : "border-ac-border/50 opacity-70 cursor-default"
                    }`}
                    title={connectable ? t("welcome.connectTooltip") : undefined}
                  >
                    <div className="flex items-center gap-2">
                      <Cpu className="w-3 h-3 text-ac-amber flex-shrink-0" />
                      <span className="font-medium text-ac-ivory truncate">
                        {inst.label ?? inst.instance_type}
                      </span>
                      <span className="text-[10px] text-ac-stone ml-auto">
                        {inst.version}
                      </span>
                    </div>
                    <div className="flex items-center gap-2 mt-1 text-[10px] text-ac-stone/70">
                      <FolderOpen className="w-2.5 h-2.5" />
                      <span className="truncate">
                        {inst.home_dir ?? inst.path}
                      </span>
                    </div>
                    <div className="flex items-center gap-3 mt-1 text-[10px]">
                      {inst.gateway_running ? (
                        <span className="text-ac-green flex items-center gap-1">
                          <Wifi className="w-2.5 h-2.5" />{" "}
                          {t("welcome.gatewayRunning")} {inst.gateway_port}
                        </span>
                      ) : (
                        <span className="text-ac-stone/50 flex items-center gap-1">
                          <Terminal className="w-2.5 h-2.5" />{" "}
                          {t("welcome.gatewayOffline")}
                        </span>
                      )}
                      <span className="text-ac-stone/50">
                        {t("welcome.profile")} {inst.active_profile}
                      </span>
                      {connectable && (
                        <span className="ml-auto text-ac-amber flex items-center gap-1">
                          <Plug className="w-2.5 h-2.5" />{" "}
                          {t("welcome.connect")}
                        </span>
                      )}
                    </div>
                  </button>
                );
              })}
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

        {/* Footer with version from the backend */}
        <p className="mt-8 text-[11px] text-ac-stone/40">
          {versionLabel && `Штурман Desktop ${versionLabel}`}
        </p>
      </div>
    </div>
  );
}
