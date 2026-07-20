// src-tauri/src/feed_sources.rs
// ADR-007: Live source data fetchers for the Activity Feed.
//
// Each fetcher launches a stdio MCP server (via mcp_client), calls a tool,
// and returns structured data for the FeedView tiles.
// v1: email. v2: jira, calendar.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp::read_mcp_servers_yaml;
use crate::mcp_client::McpStdioClient;

/// A single email message summary (no body — too long for a feed card).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    /// Email Message-ID header (unique per message, e.g. "<12345@mail.com>").
    /// Used as the React key and for display identity.
    pub id: String,
    /// IMAP sequence number (e.g. "1", "42"). Required by mark_read — the
    /// email MCP's imap.store expects this, NOT the Message-ID header.
    #[serde(default)]
    pub uid: String,
    pub subject: String,
    pub from: String,
    pub date: String,
}

/// Fetch unread email messages from the configured email MCP server.
///
/// Reads the `mcp_servers.email` block from config.yaml, spawns the MCP
/// server as a subprocess, calls `list_inbox(unread_only=true)`, and returns
/// the message summaries. Returns an error if email is not configured or the
/// MCP server fails to respond.
pub async fn list_email_unread(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Result<Vec<EmailMessage>, String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let email_cfg = servers
        .get("email")
        .ok_or_else(|| "Email MCP server not configured".to_string())?;

    let command = email_cfg
        .command
        .clone()
        .ok_or_else(|| "Email MCP: no command".to_string())?;
    let args = email_cfg.args.clone().unwrap_or_default();
    let env = email_cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;

    let result = client
        .call_tool(
            "list_inbox",
            &json!({
                "unread_only": true,
                "days": 7,
                "limit": 20,
            }),
        )
        .await;

    client.shutdown().await;
    let result = result?;

    parse_email_list_response(&result)
}

/// Mark an email message as read (or unread) via the email MCP server.
/// ADR-008 Phase 2: actionable cards — user clicks "Mark read" on a card.
pub async fn mark_email_read(
    hermes_home: &Path,
    profile: Option<&str>,
    uid: &str,
    read: bool,
) -> Result<(), String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let cfg = servers.get("email").ok_or("Email MCP not configured")?;
    let command = cfg.command.clone().ok_or("Email MCP: no command")?;
    let args = cfg.args.clone().unwrap_or_default();
    let env = cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;
    let result = client
        .call_tool(
            "mark_read",
            &json!({
                "uid": uid,
                "folder": "INBOX",
                "read": read,
            }),
        )
        .await;
    client.shutdown().await;
    let result = result?;
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("mark_read error");
        return Err(format!("Email MCP: {}", text));
    }
    Ok(())
}

/// Send an email via the email MCP server.
pub async fn send_email(
    hermes_home: &Path,
    profile: Option<&str>,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let cfg = servers.get("email").ok_or("Email MCP not configured")?;
    let command = cfg.command.clone().ok_or("Email MCP: no command")?;
    let args = cfg.args.clone().unwrap_or_default();
    let env = cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;
    let result = client
        .call_tool(
            "send_email",
            &json!({
                "to": to,
                "subject": subject,
                "body": body,
            }),
        )
        .await;
    client.shutdown().await;
    let result = result?;
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("send_email error");
        return Err(format!("Email MCP: {}", text));
    }
    Ok(())
}

/// Transition a Jira issue to a new status (e.g. "Done", "In Progress").
/// ADR-008 Phase 2: actionable cards — user clicks "Close" on a Jira card.
pub async fn jira_transition(
    hermes_home: &Path,
    profile: Option<&str>,
    issue_key: &str,
    transition_name: &str,
) -> Result<(), String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let cfg = servers.get("jira").ok_or("Jira MCP not configured")?;
    let command = cfg.command.clone().ok_or("Jira MCP: no command")?;
    let args = cfg.args.clone().unwrap_or_default();
    let env = cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;
    let result = client
        .call_tool(
            "jira_transition_issue",
            &json!({
                "issue_key": issue_key,
                "transition_name": transition_name,
            }),
        )
        .await;
    client.shutdown().await;
    let result = result?;
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("transition error");
        return Err(format!("Jira MCP: {}", text));
    }
    Ok(())
}

/// Add a comment to a Jira issue.
pub async fn jira_comment(
    hermes_home: &Path,
    profile: Option<&str>,
    issue_key: &str,
    body: &str,
) -> Result<(), String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let cfg = servers.get("jira").ok_or("Jira MCP not configured")?;
    let command = cfg.command.clone().ok_or("Jira MCP: no command")?;
    let args = cfg.args.clone().unwrap_or_default();
    let env = cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;
    let result = client
        .call_tool(
            "jira_add_comment",
            &json!({
                "issue_key": issue_key,
                "body": body,
            }),
        )
        .await;
    client.shutdown().await;
    let result = result?;
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("comment error");
        return Err(format!("Jira MCP: {}", text));
    }
    Ok(())
}

/// Parse the MCP `list_inbox` tool response into a list of EmailMessage.
///
/// The email MCP (mcp-email-life/server.py:340-341) wraps the actual data in
/// `result.content[0].text` as a JSON string. The inner JSON is:
/// `{"messages": [{"id","subject","from","to","date","body"}], "total": N}`.
///
/// On tool errors the MCP sets `isError: true` and `content[0].text` is a
/// plain error string (e.g. "ERROR: getaddrinfo failed"), NOT JSON. We check
/// `isError` first and return a clean error instead of crashing on JSON parse.
pub fn parse_email_list_response(result: &Value) -> Result<Vec<EmailMessage>, String> {
    // Check for MCP tool error first (server.py:343-344 sets isError: true).
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let err_text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("unknown MCP error");
        return Err(format!("Email MCP error: {}", err_text));
    }

    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c0| c0.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| "MCP response missing content[0].text".to_string())?;

    let payload: Value =
        serde_json::from_str(text).map_err(|e| format!("Failed to parse email payload: {}", e))?;

    parse_email_payload(&payload)
}

/// Parse the inner payload `{"messages": [...], "total": N}` into EmailMessage.
fn parse_email_payload(payload: &Value) -> Result<Vec<EmailMessage>, String> {
    let messages = payload
        .get("messages")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "Email payload missing 'messages' array".to_string())?;

    let out = messages
        .iter()
        .map(|m| EmailMessage {
            id: m
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            uid: m
                .get("uid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            subject: m
                .get("subject")
                .and_then(|v| v.as_str())
                .unwrap_or("(no subject)")
                .to_string(),
            from: m
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            date: m
                .get("date")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    Ok(out)
}

// ── Jira (v2) ──────────────────────────────────────────────────────────────

/// A Jira issue summary for the feed card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub status: String,
    pub priority: String,
    pub assignee: String,
    pub updated: String,
}

/// Fetch overdue/active Jira issues assigned to the current user.
///
/// Uses `jira_search_jql` with `assignee = currentUser() AND statusCategory != Done`.
/// Falls back to unassigned overdue if the user has no issues. The Jira MCP
/// must be configured in `mcp_servers.jira` in config.yaml.
pub async fn list_jira_my_active(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Result<Vec<JiraIssue>, String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let cfg = servers
        .get("jira")
        .ok_or_else(|| "Jira MCP server not configured".to_string())?;
    let command = cfg.command.clone().ok_or("Jira MCP: no command")?;
    let args = cfg.args.clone().unwrap_or_default();
    let env = cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;
    let result = client
        .call_tool(
            "jira_search_jql",
            &json!({
                "jql": "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
                "max_results": 10,
            }),
        )
        .await;
    client.shutdown().await;
    let result = result?;
    parse_jira_search_response(&result)
}

/// Parse the `jira_search_jql` response into Vec<JiraIssue>.
/// Response: `result.content[0].text` = JSON `{"issues": [{key, summary, status, ...}], "total"}`.
pub fn parse_jira_search_response(result: &Value) -> Result<Vec<JiraIssue>, String> {
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let err_text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("unknown Jira error");
        return Err(format!("Jira MCP error: {}", err_text));
    }
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c0| c0.get("text"))
        .and_then(|t| t.as_str())
        .ok_or("Jira response missing content[0].text")?;
    let payload: Value =
        serde_json::from_str(text).map_err(|e| format!("Jira payload parse error: {}", e))?;
    let issues = payload
        .get("issues")
        .and_then(|i| i.as_array())
        .ok_or("Jira payload missing 'issues' array")?;
    let out = issues
        .iter()
        .map(|i| JiraIssue {
            key: i
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            summary: i
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("(no summary)")
                .to_string(),
            status: i
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            priority: i
                .get("priority")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            assignee: i
                .get("assignee_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            updated: i
                .get("updated")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    Ok(out)
}

// ── Calendar (v2) ──────────────────────────────────────────────────────────

/// A calendar event summary for the feed card (L8: extended for meeting→task
/// conversion and pre-meeting briefing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Stable event UID (iCalendar UID). Used as external_refs.external_id
    /// when converting a meeting to a task.
    #[serde(default)]
    pub uid: String,
    pub summary: String,
    /// Full description / meeting agenda body from the invite.
    #[serde(default)]
    pub description: String,
    pub start: String,
    pub end: String,
    pub location: String,
    /// Who sent the invite (meeting source — used for briefing classification).
    #[serde(default)]
    pub organizer: String,
    /// Attendee list (used for pre-meeting briefing context).
    #[serde(default)]
    pub attendees: Vec<String>,
    /// Whether this is a recurring series (e.g. Daily standup).
    #[serde(default)]
    pub recurring: bool,
    /// RRULE string if recurring (used to decide series vs instance link).
    #[serde(default)]
    pub recurrence_rule: String,
}

/// Fetch today's calendar events from the rupost_calendar MCP.
///
/// Calls `list_calendars` to discover the personal calendar URL, then
/// `list_events` for today's date range.
pub async fn list_calendar_today(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Result<Vec<CalendarEvent>, String> {
    let servers = read_mcp_servers_yaml(hermes_home, profile);
    let cfg = servers
        .get("rupost_calendar")
        .ok_or_else(|| "Calendar MCP server not configured".to_string())?;
    let command = cfg.command.clone().ok_or("Calendar MCP: no command")?;
    let args = cfg.args.clone().unwrap_or_default();
    let env = cfg.env.clone().unwrap_or_default();

    let mut client = McpStdioClient::spawn(&command, &args, &env)?;
    client.initialize().await?;

    // 1. Discover the calendar URL.
    let cal_result = client.call_tool("list_calendars", &json!({})).await?;
    let cal_url = parse_calendar_url(&cal_result)?;

    // 2. List events for today + next 7 days.
    let now = chrono::Utc::now();
    let since = now.format("%Y-%m-%dT00:00:00").to_string();
    let until = (now + chrono::Duration::days(7))
        .format("%Y-%m-%dT23:59:59")
        .to_string();
    let events_result = client
        .call_tool(
            "list_events",
            &json!({
                "calendar_url": cal_url,
                "since": since,
                "until": until,
                "limit": 10,
            }),
        )
        .await;
    client.shutdown().await;
    let events_result = events_result?;
    parse_calendar_events_response(&events_result)
}

/// Parse an ISO-8601 timestamp (with or without offset) into a chrono DateTime.
/// Returns None if unparseable — callers treat unparseable as "no reminder".
fn parse_iso(ts: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    // Try common fallbacks: space-separated, no offset (assume local/UTC).
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    None
}

/// Minutes until the event starts (negative if already started/past).
pub fn minutes_until_start(
    event: &CalendarEvent,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<i64> {
    let start = parse_iso(&event.start)?;
    let diff = start.signed_duration_since(now);
    Some(diff.num_minutes())
}

/// L8.3: compute meeting reminders.
///
/// Returns events whose start falls within `[0, reminder_minutes]` from now
/// (i.e. starting soon or just started). The frontend polls this and shows a
/// "встреча через N мин" card. Dedup (don't re-show) is the frontend's
/// responsibility via a `reminder_shown` set keyed by uid.
pub async fn list_meeting_reminders(
    hermes_home: &Path,
    profile: Option<&str>,
    reminder_minutes: i64,
) -> Result<Vec<CalendarEvent>, String> {
    let events = list_calendar_today(hermes_home, profile).await?;
    let now = chrono::Utc::now();
    let due = events
        .into_iter()
        .filter_map(|e| {
            let mins = minutes_until_start(&e, now)?;
            // Within window: starting within reminder_minutes, or just started
            // (up to 5 min grace so the card lingers briefly after start).
            if mins >= -5 && mins <= reminder_minutes {
                Some(e)
            } else {
                None
            }
        })
        .collect();
    Ok(due)
}

/// Extract the personal calendar URL from `list_calendars` response.
/// Response: `content[0].text` = JSON `{url, display_name, color}` (single)
/// or `{calendars: [...]}`.
fn parse_calendar_url(result: &Value) -> Result<String, String> {
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let err_text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("calendar error");
        return Err(format!("Calendar MCP error: {}", err_text));
    }
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c0| c0.get("text"))
        .and_then(|t| t.as_str())
        .ok_or("Calendar response missing content[0].text")?;
    let payload: Value =
        serde_json::from_str(text).map_err(|e| format!("Calendar payload parse error: {}", e))?;
    // Single calendar: {url: "..."}
    if let Some(url) = payload.get("url").and_then(|u| u.as_str()) {
        return Ok(url.to_string());
    }
    // Multiple: {calendars: [{url: "..."}]}
    if let Some(cals) = payload.get("calendars").and_then(|c| c.as_array()) {
        if let Some(first) = cals.first() {
            if let Some(url) = first.get("url").and_then(|u| u.as_str()) {
                return Ok(url.to_string());
            }
        }
    }
    Err("No calendar URL found in list_calendars response".to_string())
}

/// Parse `list_events` response into Vec<CalendarEvent>.
///
/// The rupost_calendar MCP (FastMCP) serializes the `list[Event]` return as a
/// JSON string in `content[0].text`. Depending on count, this can be:
///   - A bare array: `[{"summary":...,"start_local":...}]`
///   - A bare object (single event): `{"summary":...,"start_local":...}`
///   - Or wrapped: `{"events": [...]}`
///
/// Event fields use `start_local`/`end_local` (ISO with offset), not `start`.
fn parse_calendar_events_response(result: &Value) -> Result<Vec<CalendarEvent>, String> {
    if result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let err_text = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("events error");
        return Err(format!("Calendar events error: {}", err_text));
    }
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c0| c0.get("text"))
        .and_then(|t| t.as_str())
        .ok_or("Events response missing content[0].text")?;
    let payload: Value =
        serde_json::from_str(text).map_err(|e| format!("Events payload parse error: {}", e))?;

    // Normalize to a list of event objects.
    let events: Vec<&Value> = if let Some(arr) = payload.get("events").and_then(|e| e.as_array()) {
        arr.iter().collect()
    } else if let Some(arr) = payload.as_array() {
        arr.iter().collect()
    } else if payload.is_object() {
        // Bare single event object.
        vec![&payload]
    } else {
        return Err("Events payload has no events".to_string());
    };

    let out = events
        .iter()
        .map(|e| {
            // Attendees may be a string list or a list of {email, display_name}.
            let attendees: Vec<String> = e
                .get("attendees")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| {
                            if let Some(s) = a.as_str() {
                                Some(s.to_string())
                            } else {
                                a.get("email")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| a.get("display_name").and_then(|v| v.as_str()))
                                    .or_else(|| a.get("name").and_then(|v| v.as_str()))
                                    .map(|s| s.to_string())
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let recurring = e
                .get("recurring")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    // Some servers expose RRULE only, or a "is_recurring" flag.
                    e.get("rrule")
                        .and_then(|v| v.as_str())
                        .map(|s| !s.is_empty())
                })
                .unwrap_or(false);
            CalendarEvent {
                uid: e
                    .get("uid")
                    .and_then(|v| v.as_str())
                    .or_else(|| e.get("id").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                summary: e
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no title)")
                    .to_string(),
                description: e
                    .get("description")
                    .and_then(|v| v.as_str())
                    .or_else(|| e.get("body").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                start: e
                    .get("start_local")
                    .and_then(|v| v.as_str())
                    .or_else(|| e.get("start").and_then(|v| v.as_str()))
                    .or_else(|| e.get("dtstart").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                end: e
                    .get("end_local")
                    .and_then(|v| v.as_str())
                    .or_else(|| e.get("end").and_then(|v| v.as_str()))
                    .or_else(|| e.get("dtend").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                location: e
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                organizer: e
                    .get("organizer")
                    .and_then(|v| v.as_str())
                    .or_else(|| e.get("organizer_email").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                attendees,
                recurring,
                recurrence_rule: e
                    .get("rrule")
                    .and_then(|v| v.as_str())
                    .or_else(|| e.get("recurrence_rule").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
            }
        })
        .collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Live debug test — run with:
    ///   cargo test --lib live_email_debug -- --nocapture --ignored
    /// Shows the EXACT error string from list_email_unread against the real
    /// config.yaml. Helps diagnose env-propagation / spawn issues.
    #[tokio::test]
    #[ignore]
    async fn live_email_debug() {
        let home = crate::config::resolve_hermes_home();
        // First, dump the env we read from config.
        let servers = read_mcp_servers_yaml(&home, std::option::Option::None);
        if let Some(email) = servers.get("email") {
            let env = email.env.clone().unwrap_or_default();
            eprintln!("=== email MCP env from config.yaml ===");
            for (k, v) in &env {
                let display = if k.contains("PASSWORD") || k.contains("PAT") {
                    format!("{} chars", v.len())
                } else {
                    v.clone()
                };
                eprintln!("  {} = {}", k, display);
            }
        } else {
            eprintln!("NO email MCP in config!");
        }
        match list_email_unread(&home, std::option::Option::None).await {
            Ok(msgs) => eprintln!("OK: {} messages", msgs.len()),
            Err(e) => eprintln!("ERR: {}", e),
        }
    }

    /// Live debug — run with:
    ///   cargo test --lib live_jira_debug -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn live_jira_debug() {
        let home = crate::config::resolve_hermes_home();
        match list_jira_my_active(&home, std::option::Option::None).await {
            Ok(issues) => eprintln!("OK: {} issues", issues.len()),
            Err(e) => eprintln!("ERR: {}", e),
        }
    }

    /// Live debug — run with:
    ///   cargo test --lib live_cal_debug -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn live_cal_debug() {
        let home = crate::config::resolve_hermes_home();
        // Check key name first
        let servers = read_mcp_servers_yaml(&home, std::option::Option::None);
        eprintln!("=== MCP server keys ===");
        for k in servers.keys() {
            eprintln!("  {}", k);
        }
        match list_calendar_today(&home, std::option::Option::None).await {
            Ok(events) => eprintln!("OK: {} events", events.len()),
            Err(e) => eprintln!("ERR: {}", e),
        }
    }

    #[test]
    fn parse_email_list_response_extracts_messages() {
        // Realistic MCP response: result.content[0].text is a JSON string
        // wrapping the actual email data.
        let inner = json!({
            "messages": [
                {"id": "<msg1@mail>", "subject": "Urgent: deploy today", "from": "boss@corp.ru", "date": "Thu, 17 Jul 2026 10:00:00 +0300", "body": "..."},
                {"id": "<msg2@mail>", "subject": "Re: PR review", "from": "dev@corp.ru", "date": "Thu, 17 Jul 2026 09:30:00 +0300", "body": "..."},
            ],
            "total": 2
        });
        let response = json!({
            "content": [{"type": "text", "text": inner.to_string()}]
        });
        let msgs = parse_email_list_response(&response).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].subject, "Urgent: deploy today");
        assert_eq!(msgs[0].from, "boss@corp.ru");
        assert_eq!(msgs[1].id, "<msg2@mail>");
    }

    #[test]
    fn parse_email_response_handles_empty() {
        let inner = json!({"messages": [], "total": 0});
        let response = json!({
            "content": [{"type": "text", "text": inner.to_string()}]
        });
        let msgs = parse_email_list_response(&response).unwrap();
        assert!(msgs.is_empty(), "empty inbox must yield empty vec");
    }

    #[test]
    fn parse_email_response_errors_on_iserror_flag() {
        // When the MCP server can't reach IMAP (e.g. DNS fail without VPN),
        // it returns isError: true with a plain-text error (NOT JSON).
        // We must return a clean error, not crash on JSON parse.
        let response = json!({
            "content": [{"type": "text", "text": "ERROR: [Errno 11001] getaddrinfo failed"}],
            "isError": true
        });
        let result = parse_email_list_response(&response);
        assert!(result.is_err(), "isError response must be an error");
        let err = result.unwrap_err();
        assert!(
            err.contains("getaddrinfo"),
            "error must carry the MCP message: {}",
            err
        );
    }

    #[test]
    fn parse_email_response_errors_on_missing_content() {
        // Malformed response (no content wrapper) → error, not panic.
        let bad = json!({"foo": "bar"});
        let result = parse_email_list_response(&bad);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("content"));
    }

    #[test]
    fn parse_email_defaults_missing_fields() {
        // A message missing 'subject' must get "(no subject)", not crash.
        let inner = json!({
            "messages": [{"id": "x", "from": "a@b.c", "date": ""}],
            "total": 1
        });
        let response = json!({"content": [{"type": "text", "text": inner.to_string()}]});
        let msgs = parse_email_list_response(&response).unwrap();
        assert_eq!(msgs[0].subject, "(no subject)");
    }

    #[test]
    fn list_email_unread_returns_error_if_not_configured() {
        // No config.yaml → read_mcp_servers_yaml returns empty → error.
        let dir = std::env::temp_dir().join(format!(
            "steersman-feed-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // No config.yaml written → servers map is empty.
        let result = futures::executor::block_on(list_email_unread(&dir, None));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not configured") || err.contains("Email"),
            "unexpected error: {}",
            err
        );
    }

    // ── Jira parser tests ──────────────────────────────────────────────────

    #[test]
    fn parse_jira_response_extracts_issues() {
        let inner = json!({
            "issues": [
                {"key":"DEVOS-3","summary":"Fix bug","status":"In Progress","priority":"High","assignee_name":"ngusev","updated":"2026-07-17T10:00:00"},
                {"key":"DEVOS-5","summary":"Add tests","status":"Open","priority":"Medium","assignee_name":"ngusev","updated":"2026-07-16T15:00:00"}
            ],
            "total": 2
        });
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let issues = parse_jira_search_response(&response).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].key, "DEVOS-3");
        assert_eq!(issues[0].status, "In Progress");
        assert_eq!(issues[1].summary, "Add tests");
    }

    #[test]
    fn parse_jira_response_handles_empty() {
        let inner = json!({"issues":[],"total":0});
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let issues = parse_jira_search_response(&response).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn parse_jira_response_errors_on_iserror() {
        let response = json!({
            "content":[{"type":"text","text":"Error: unknown user key"}],
            "isError": true
        });
        let result = parse_jira_search_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown user key"));
    }

    // ── Calendar parser tests ───────────────────────────────────────────────

    #[test]
    fn parse_calendar_url_single() {
        let inner = json!({"url":"https://cal.example.com/personal/","display_name":"Personal"});
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let url = parse_calendar_url(&response).unwrap();
        assert_eq!(url, "https://cal.example.com/personal/");
    }

    #[test]
    fn parse_calendar_url_multiple() {
        let inner =
            json!({"calendars":[{"url":"https://cal.example.com/work/","display_name":"Work"}]});
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let url = parse_calendar_url(&response).unwrap();
        assert_eq!(url, "https://cal.example.com/work/");
    }

    #[test]
    fn parse_calendar_events_extracts() {
        let inner = json!({
            "events": [
                {"summary":"Daily standup","start_local":"2026-07-18T09:00:00+03:00","end_local":"2026-07-18T09:30:00+03:00","location":"Room 1"},
                {"summary":"1:1 with boss","start_local":"2026-07-18T11:00:00+03:00","end_local":"2026-07-18T11:30:00+03:00","location":""}
            ]
        });
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let events = parse_calendar_events_response(&response).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].summary, "Daily standup");
        assert_eq!(events[0].location, "Room 1");
        assert_eq!(events[0].start, "2026-07-18T09:00:00+03:00");
        assert_eq!(events[1].location, "");
    }

    #[test]
    fn parse_calendar_events_bare_single_object() {
        // The real rupost_calendar MCP returns a bare event object (not
        // wrapped in {events:[]}) when there's one event.
        let inner = json!({
            "uid":"abc","summary":"Daily Portfolio","start_local":"2026-07-18T10:00:00+03:00","end_local":"2026-07-18T10:30:00+03:00","location":"https://meet/xyz"
        });
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let events = parse_calendar_events_response(&response).unwrap();
        assert_eq!(events.len(), 1, "bare object must yield single event");
        assert_eq!(events[0].summary, "Daily Portfolio");
        assert_eq!(events[0].start, "2026-07-18T10:00:00+03:00");
    }

    #[test]
    fn parse_calendar_events_bare_array() {
        // Some MCPs return a bare array instead of {events: [...]}.
        let inner = json!([{"summary":"Lunch","start":"12:00","end":"13:00"}]);
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let events = parse_calendar_events_response(&response).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Lunch");
    }

    #[test]
    fn parse_calendar_events_errors_on_iserror() {
        let response =
            json!({"content":[{"type":"text","text":"CalDAV auth failed"}],"isError":true});
        let result = parse_calendar_events_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CalDAV"));
    }

    #[test]
    fn parse_calendar_events_extracts_meeting_metadata() {
        // L8: recurring daily with attendees/organizer/description must parse.
        let inner = json!({
            "events": [{
                "uid": "devos-daily-2026-07-18",
                "summary": "Daily standup",
                "description": "Quick status sync",
                "start_local": "2026-07-18T09:00:00+03:00",
                "end_local": "2026-07-18T09:30:00+03:00",
                "location": "Room 1",
                "organizer": "boss@corp.ru",
                "attendees": [
                    {"email": "a@corp.ru", "display_name": "Alice"},
                    "b@corp.ru"
                ],
                "recurring": true,
                "rrule": "FREQ=DAILY"
            }]
        });
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let events = parse_calendar_events_response(&response).unwrap();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.uid, "devos-daily-2026-07-18");
        assert_eq!(ev.description, "Quick status sync");
        assert_eq!(ev.organizer, "boss@corp.ru");
        assert_eq!(
            ev.attendees,
            vec!["a@corp.ru".to_string(), "b@corp.ru".to_string()]
        );
        assert!(ev.recurring);
        assert_eq!(ev.recurrence_rule, "FREQ=DAILY");
    }

    #[test]
    fn parse_calendar_events_recurring_from_rrule_only() {
        // Some servers omit a boolean `recurring` and only expose `rrule`.
        let inner = json!({
            "events": [{
                "uid": "x",
                "summary": "Weekly",
                "start_local": "2026-07-18T09:00:00+03:00",
                "end_local": "2026-07-18T09:30:00+03:00",
                "rrule": "FREQ=WEEKLY"
            }]
        });
        let response = json!({"content":[{"type":"text","text":inner.to_string()}]});
        let events = parse_calendar_events_response(&response).unwrap();
        assert!(
            events[0].recurring,
            "recurring inferred from non-empty rrule"
        );
    }

    #[test]
    fn minutes_until_start_parses_rfc3339() {
        let ev = CalendarEvent {
            uid: "u".into(),
            summary: "x".into(),
            description: String::new(),
            start: "2026-07-18T10:00:00+00:00".into(),
            end: String::new(),
            location: String::new(),
            organizer: String::new(),
            attendees: vec![],
            recurring: false,
            recurrence_rule: String::new(),
        };
        // now = 09:50 UTC → 10 min until start.
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-18T09:50:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(minutes_until_start(&ev, now), Some(10));
    }

    #[test]
    fn list_meeting_reminders_filters_window() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-18T09:50:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let soon = CalendarEvent {
            uid: "soon".into(),
            summary: "Soon".into(),
            description: String::new(),
            start: "2026-07-18T09:55:00+00:00".into(),
            end: String::new(),
            location: String::new(),
            organizer: String::new(),
            attendees: vec![],
            recurring: false,
            recurrence_rule: String::new(),
        };
        let far = CalendarEvent {
            uid: "far".into(),
            summary: "Far".into(),
            description: String::new(),
            start: "2026-07-18T15:00:00+00:00".into(),
            end: String::new(),
            location: String::new(),
            organizer: String::new(),
            attendees: vec![],
            recurring: false,
            recurrence_rule: String::new(),
        };
        // Within 15-min window: only `soon`.
        let due: Vec<_> = vec![soon.clone(), far.clone()]
            .into_iter()
            .filter_map(|e| {
                let mins = minutes_until_start(&e, now)?;
                if mins >= -5 && mins <= 15 {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].uid, "soon");
    }
}
