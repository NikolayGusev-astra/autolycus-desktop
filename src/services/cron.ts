/**
 * Cron Jobs Service - Typed wrapper for cron commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const cronService = {
  // List cron jobs
  async listCronJobs(): Promise<Array<{
    id: string;
    name: string;
    schedule: string;
    command: string;
    enabled: boolean;
    nextRun?: number;
    lastRun?: number;
  }>> {
    return invoke("list_cron_jobs_cmd");
  },

  // Create cron job
  async createCronJob(job: { name: string; schedule: string; command: string; enabled: boolean }): Promise<void> {
    return invoke("create_cron_job_cmd", { job });
  },

  // Remove cron job
  async removeCronJob(id: string): Promise<void> {
    return invoke("remove_cron_job_cmd", { id });
  },

  // Pause cron job
  async pauseCronJob(id: string): Promise<void> {
    return invoke("pause_cron_job_cmd", { id });
  },

  // Resume cron job
  async resumeCronJob(id: string): Promise<void> {
    return invoke("resume_cron_job_cmd", { id });
  },

  // Trigger cron job
  async triggerCronJob(id: string): Promise<void> {
    return invoke("trigger_cron_job_cmd", { id });
  },
};