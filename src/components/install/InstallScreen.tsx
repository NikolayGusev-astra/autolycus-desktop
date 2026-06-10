// src/components/install/InstallScreen.tsx
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, ChevronLeft, CheckCircle2, Loader2 } from "lucide-react";
import { PROVIDERS } from "../../constants";

type Step = "welcome" | "provider" | "apiKey" | "test" | "done";

export function InstallScreen({ onComplete }: { onComplete?: () => void }) {
  const [step, setStep] = useState<Step>("welcome");
  const [selectedProvider, setSelectedProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ success: boolean; message: string } | null>(null);

  const steps: { id: Step; label: string }[] = [
    { id: "welcome", label: "Welcome" },
    { id: "provider", label: "Provider" },
    { id: "apiKey", label: "API Key" },
    { id: "test", label: "Test" },
    { id: "done", label: "Done" },
  ];

  const currentIndex = steps.findIndex((s) => s.id === step);

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const envKey = PROVIDERS.setup.find((s) => s.id === selectedProvider)?.envKey || "";
      if (envKey) {
        await invoke("store_credential_cmd", { key: envKey, value: apiKey });
      }
      setTestResult({ success: true, message: "API key saved successfully!" });
      setStep("done");
    } catch (err) {
      setTestResult({ success: false, message: String(err) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="flex-1 flex items-center justify-center p-6">
      <div className="max-w-lg w-full">
        {/* Progress */}
        <div className="flex items-center gap-2 mb-8">
          {steps.map((s, i) => (
            <div key={s.id} className="flex items-center gap-2 flex-1">
              <div
                className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-medium ${
                  i <= currentIndex
                    ? "bg-ac-blue text-white"
                    : "bg-ac-bg text-ac-muted border border-ac-border"
                }`}
              >
                {i < currentIndex ? "✓" : i + 1}
              </div>
              {i < steps.length - 1 && (
                <div className={`flex-1 h-0.5 ${i < currentIndex ? "bg-ac-blue" : "bg-ac-border"}`} />
              )}
            </div>
          ))}
        </div>

        {/* Step Content */}
        <div className="bg-ac-bg border border-ac-border rounded-xl p-6">
          {step === "welcome" && (
            <div className="text-center">
              <h1 className="text-2xl font-bold mb-2">Welcome to Autolycus</h1>
              <p className="text-ac-muted mb-6">
                Let's get you set up. This wizard will guide you through configuring
                your first AI provider.
              </p>
              <button className="btn btn-primary" onClick={() => setStep("provider")}>
                Get Started <ChevronRight className="w-4 h-4 ml-1" />
              </button>
            </div>
          )}

          {step === "provider" && (
            <div>
              <h2 className="text-lg font-semibold mb-4">Choose a Provider</h2>
              <div className="space-y-2 max-h-64 overflow-y-auto">
                {PROVIDERS.setup
                  .filter((s) => s.needsKey)
                  .map((p) => (
                    <button
                      key={p.id}
                      className={`w-full text-left p-3 rounded-lg border transition-colors ${
                        selectedProvider === p.id
                          ? "border-ac-blue bg-ac-blue/10"
                          : "border-ac-border hover:border-ac-stone"
                      }`}
                      onClick={() => setSelectedProvider(p.id)}
                    >
                      <div className="flex items-center justify-between">
                        <div>
                          <span className="font-medium text-sm">{p.name}</span>
                          {p.tag && (
                            <span className="ml-2 text-xs bg-ac-blue/20 text-ac-blue px-2 py-0.5 rounded">
                              {p.tag}
                            </span>
                          )}
                          <p className="text-xs text-ac-muted mt-1">{p.desc}</p>
                        </div>
                        {selectedProvider === p.id && (
                          <CheckCircle2 className="w-5 h-5 text-ac-blue" />
                        )}
                      </div>
                    </button>
                  ))}
              </div>
              <div className="flex justify-between mt-6">
                <button className="btn btn-ghost btn-sm" onClick={() => setStep("welcome")}>
                  <ChevronLeft className="w-4 h-4 mr-1" /> Back
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => setStep("apiKey")}
                  disabled={!selectedProvider}
                >
                  Next <ChevronRight className="w-4 h-4 ml-1" />
                </button>
              </div>
            </div>
          )}

          {step === "apiKey" && (
            <div>
              <h2 className="text-lg font-semibold mb-4">Enter API Key</h2>
              <p className="text-sm text-ac-muted mb-4">
                Enter your {PROVIDERS.setup.find((s) => s.id === selectedProvider)?.name} API key.
              </p>
              <input
                type="password"
                className="input w-full mb-4"
                placeholder={
                  PROVIDERS.setup.find((s) => s.id === selectedProvider)?.placeholder ||
                  "Enter API key"
                }
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
              />
              <div className="flex justify-between">
                <button className="btn btn-ghost btn-sm" onClick={() => setStep("provider")}>
                  <ChevronLeft className="w-4 h-4 mr-1" /> Back
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => setStep("test")}
                  disabled={!apiKey.trim()}
                >
                  Next <ChevronRight className="w-4 h-4 ml-1" />
                </button>
              </div>
            </div>
          )}

          {step === "test" && (
            <div className="text-center">
              <h2 className="text-lg font-semibold mb-4">Test Connection</h2>
              {testing ? (
                <div className="flex items-center justify-center gap-2 py-4">
                  <Loader2 className="w-5 h-5 animate-spin" />
                  <span className="text-ac-muted">Testing...</span>
                </div>
              ) : testResult ? (
                <div className={`p-4 rounded-lg ${testResult.success ? "bg-green-500/10" : "bg-red-500/10"}`}>
                  <p className={testResult.success ? "text-green-500" : "text-red-400"}>
                    {testResult.message}
                  </p>
                </div>
              ) : (
                <p className="text-ac-muted mb-4">
                  Click below to save your API key and test the connection.
                </p>
              )}
              <div className="flex justify-between mt-6">
                <button className="btn btn-ghost btn-sm" onClick={() => setStep("apiKey")}>
                  <ChevronLeft className="w-4 h-4 mr-1" /> Back
                </button>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => void handleTest()}
                  disabled={testing}
                >
                  {testing ? "Testing..." : "Save & Test"}
                </button>
              </div>
            </div>
          )}

          {step === "done" && (
            <div className="text-center">
              <CheckCircle2 className="w-12 h-12 text-green-500 mx-auto mb-4" />
              <h2 className="text-lg font-semibold mb-2">Setup Complete!</h2>
              <p className="text-ac-muted mb-6">
                Your provider has been configured successfully. You can now start using Autolycus.
              </p>
              <button
                className="btn btn-primary"
                onClick={() => onComplete?.()}
              >
                Start Chatting
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default InstallScreen;
