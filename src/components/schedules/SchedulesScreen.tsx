// src/components/schedules/SchedulesScreen.tsx
// v0.5.0: Cron schedules screen — list, create, remove, pause, resume, trigger via Rust

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Timer,
  Plus,
  Trash2,
  Play,
  Pause,
  RefreshCw,
  Loader,
  Clock,
  Zap,
} from "lucide-react";

interface CronJob {
  id: string;
  name: string;
  schedule: string;
  prompt: string;
  state: string;
  enabled: boolean;
  next_run_at: string | null;
  last_run_at: string | null;
  last_status: string | null;
  last_error: string | null;
  deliver?: string[];
  skills?: string[];
  script?: string | null;
}

export function SchedulesScreen() {
  const [jobs, setJobs] = useState<CronJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newSchedule, setNewSchedule] = useState("");
  const [newPrompt, setNewPrompt] = useState("");
  const [creating, setCreating] = useState(false);

  const loadJobs = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<CronJob[]>("list_cron_jobs_cmd", {
        includeDisabled: true,
        profile: null,
      });
      setJobs(result);
    } catch (err) {
      console.error("Failed to load cron jobs:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadJobs();
  }, [loadJobs]);

  const handleCreate = async () => {
    if (!newName.trim() || !newSchedule.trim()) return;
    try {
      setCreating(true);
      await invoke("create_cron_job_cmd", {
        schedule: newSchedule.trim(),
        prompt: newPrompt.trim() || null,
        name: newName.trim(),
        deliver: null,
        profile: null,
      });
      setNewName("");
      setNewSchedule("");
      setNewPrompt("");
      setShowCreate(false);
      await loadJobs();
    } catch (err) {
      console.error("Failed to create cron job:", err);
    } finally {
      setCreating(false);
    }
  };

  const handleRemove = async (jobId: string) => {
    try {
      await invoke("remove_cron_job_cmd", {
        jobId,
        profile: null,
      });
      await loadJobs();
    } catch (err) {
      console.error("Failed to remove cron job:", err);
    }
  };

  const handlePause = async (jobId: string) => {
    try {
      await invoke("pause_cron_job_cmd", {
        jobId,
        profile: null,
      });
      await loadJobs();
    } catch (err) {
      console.error("Failed to pause cron job:", err);
    }
  };

  const handleResume = async (jobId: string) => {
    try {
      await invoke("resume_cron_job_cmd", {
        jobId,
        profile: null,
      });
      await loadJobs();
    } catch (err) {
      console.error("Failed to resume cron job:", err);
    }
  };

  const handleTrigger = async (jobId: string) => {
    try {
      await invoke("trigger_cron_job_cmd", {
        jobId,
        profile: null,
      });
    } catch (err) {
      console.error("Failed to trigger cron job:", err);
    }
  };

  const schedulePresets = [
    { label: "Every hour", value: "0 * * * *" },
    { label: "Every 6 hours", value: "0 */6 * * *" },
    { label: "Daily at midnight", value: "0 0 * * *" },
    { label: "Daily at 9 AM", value: "0 9 * * *" },
    { label: "Weekly (Mon 9 AM)", value: "0 9 * * 1" },
  ];

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto">
        {/* Header */}
        <div className="px-4 py-3 border-b border-ac-border">
          <h2 className="text-sm font-semibold text-ac-ivory flex items-center gap-2">
            <Timer className="w-4 h-4 text-ac-amber" />
            Schedules
          </h2>
        </div>

        {/* Create button / form */}
        <div className="px-4 py-3 border-b border-ac-border/50">
          {!showCreate ? (
            <button
              onClick={() => setShowCreate(true)}
              className="ac-btn w-full px-3 py-2 text-sm flex items-center justify-center gap-2"
            >
              <Plus className="w-4 h-4" />
              New Schedule
            </button>
          ) : (
            <div className="space-y-2">
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="Job name..."
                className="ac-input w-full text-sm px-3 py-1.5"
              />
              <input
                type="text"
                value={newSchedule}
                onChange={(e) => setNewSchedule(e.target.value)}
                placeholder="Cron expression (e.g. 0 */6 * * *)"
                className="ac-input w-full text-sm px-3 py-1.5 font-mono"
              />
              {/* Presets */}
              <div className="flex flex-wrap gap-1">
                {schedulePresets.map((preset) => (
                  <button
                    key={preset.value}
                    onClick={() => setNewSchedule(preset.value)}
                    className={`text-[10px] px-2 py-0.5 rounded border transition-colors ${
                      newSchedule === preset.value
                        ? "border-ac-amber bg-ac-amber/10 text-ac-amber"
                        : "border-ac-border text-ac-stone hover:border-ac-amber/50"
                    }`}
                  >
                    {preset.label}
                  </button>
                ))}
              </div>
              <textarea
                value={newPrompt}
                onChange={(e) => setNewPrompt(e.target.value)}
                placeholder="Prompt (optional)..."
                className="ac-input w-full text-xs px-3 py-1.5 min-h-[60px] resize-y"
              />
              <div className="flex gap-2">
                <button
                  onClick={handleCreate}
                  disabled={!newName.trim() || !newSchedule.trim() || creating}
                  className="ac-btn flex-1 px-3 py-1.5 text-sm disabled:opacity-40 flex items-center justify-center gap-1"
                >
                  {creating ? (
                    <Loader className="w-3.5 h-3.5 animate-spin" />
                  ) : (
                    <Plus className="w-3.5 h-3.5" />
                  )}
                  Create
                </button>
                <button
                  onClick={() => {
                    setShowCreate(false);
                    setNewName("");
                    setNewSchedule("");
                    setNewPrompt("");
                  }}
                  className="ac-btn px-3 py-1.5 text-sm"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Job list */}
        <div className="px-4 py-3">
          {loading ? (
            <div className="flex items-center justify-center h-32">
              <Loader className="w-5 h-5 text-ac-amber animate-spin" />
            </div>
          ) : jobs.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-32 text-ac-stone text-sm">
              <Timer className="w-8 h-8 mb-2 opacity-30" />
              <p>No schedules configured</p>
              <p className="text-[10px] mt-1">Create one using the button above</p>
            </div>
          ) : (
            <div className="space-y-2">
              {jobs.map((job) => {
                const isActive = job.enabled && job.state === "active";
                const statusLabel = isActive ? "Active" : "Paused";

                return (
                  <div
                    key={job.id}
                    className="p-3 rounded-lg bg-ac-surface/30 border border-ac-border/50"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <span className="text-xs font-medium text-ac-ivory">
                            {job.name}
                          </span>
                          <span
                            className={`text-[10px] px-1.5 py-0.5 rounded-full ${
                              isActive
                                ? "bg-ac-green/10 text-ac-green"
                                : "bg-ac-stone/10 text-ac-stone"
                            }`}
                          >
                            {statusLabel}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 text-[10px] text-ac-stone">
                          <Clock className="w-3 h-3" />
                          <code className="font-mono">{job.schedule}</code>
                        </div>
                        {job.prompt && (
                          <p className="text-[10px] text-ac-stone/70 mt-1 line-clamp-2">
                            {job.prompt}
                          </p>
                        )}
                        {job.last_run_at && (
                          <p className="text-[10px] text-ac-stone/50 mt-1">
                            Last run:{" "}
                            {new Date(job.last_run_at).toLocaleString()}
                            {job.last_status &&
                              ` — ${job.last_status}`}
                          </p>
                        )}
                        {job.last_error && (
                          <p className="text-[10px] text-ac-red mt-1">
                            Error: {job.last_error}
                          </p>
                        )}
                      </div>

                      <div className="flex gap-1 shrink-0">
                        {isActive ? (
                          <button
                            onClick={() => handlePause(job.id)}
                            className="text-ac-stone hover:text-ac-yellow p-1 rounded transition-colors"
                            title="Pause"
                          >
                            <Pause className="w-3.5 h-3.5" />
                          </button>
                        ) : (
                          <button
                            onClick={() => handleResume(job.id)}
                            className="text-ac-stone hover:text-ac-green p-1 rounded transition-colors"
                            title="Resume"
                          >
                            <Play className="w-3.5 h-3.5" />
                          </button>
                        )}
                        <button
                          onClick={() => handleTrigger(job.id)}
                          className="text-ac-stone hover:text-ac-amber p-1 rounded transition-colors"
                          title="Trigger now"
                        >
                          <Zap className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => handleRemove(job.id)}
                          className="text-ac-stone hover:text-ac-red p-1 rounded transition-colors"
                          title="Remove"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      {/* Footer */}
      <div className="px-4 py-2 border-t border-ac-border text-[10px] text-ac-stone/50 flex justify-between items-center">
        <span>{jobs.length} schedules</span>
        <button
          onClick={loadJobs}
          className="text-ac-stone hover:text-ac-amber transition-colors"
          title="Refresh"
        >
          <RefreshCw className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}
