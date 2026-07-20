/**
 * Sessions Service - Typed wrapper for session-related backend commands.
 * Components should import from here instead of calling invoke() directly.
 */
import { invoke } from "@tauri-apps/api/core";

export const sessionsService = {
  // List feed
  async listFeed(channel?: string, limit?: number, offset?: number): Promise<Array<{
    id: string;
    title: string;
    source: string;
    timestamp: number;
    metadata: Record<string, unknown>;
  }>> {
    return invoke("list_feed_cmd", { channel, limit, offset });
  },

  // List feed channels
  async listFeedChannels(): Promise<Array<{ name: string; count: number }>> {
    return invoke("list_feed_channels_cmd");
  },

  // List email unread
  async listEmailUnread(): Promise<Array<{ id: string; subject: string; from: string; date: number }>> {
    return invoke("list_email_unread_cmd");
  },

  // List Jira my active
  async listJiraMyActive(): Promise<Array<{ key: string; summary: string; status: string }>> {
    return invoke("list_jira_my_active_cmd");
  },

  // List calendar today
  async listCalendarToday(): Promise<Array<{ id: string; title: string; start: number; end: number }>> {
    return invoke("list_calendar_today_cmd");
  },

  // List meeting reminders
  async listMeetingReminders(): Promise<Array<{ id: string; title: string; time: number }>> {
    return invoke("list_meeting_reminders_cmd");
  },

  // Mark email read
  async markEmailRead(id: string): Promise<void> {
    return invoke("mark_email_read_cmd", { id });
  },

  // Generate meeting briefing
  async generateMeetingBriefing(eventId: string): Promise<string> {
    return invoke("generate_meeting_briefing_cmd", { eventId });
  },

  // Send email
  async sendEmail(to: string, subject: string, body: string): Promise<void> {
    return invoke("send_email_cmd", { to, subject, body });
  },

  // Jira transition
  async jiraTransition(issueKey: string, transitionId: string): Promise<void> {
    return invoke("jira_transition_cmd", { issueKey, transitionId });
  },

  // Jira comment
  async jiraComment(issueKey: string, comment: string): Promise<void> {
    return invoke("jira_comment_cmd", { issueKey, comment });
  },

  // Register Steersman MCP
  async registerSteersmanMcp(): Promise<void> {
    return invoke("register_steersman_mcp_cmd");
  },

  // Generate smart briefing
  async generateSmartBriefing(sessionId: string): Promise<string> {
    return invoke("generate_smart_briefing_cmd", { sessionId });
  },

  // Get cached briefing
  async getCachedBriefing(sessionId: string): Promise<string | null> {
    return invoke("get_cached_briefing_cmd", { sessionId });
  },
};