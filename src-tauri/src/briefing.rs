// src-tauri/src/briefing.rs
// Smart-briefing integration: spawns the smart_briefing MCP server,
// fetches structured JSON (urgent/important/stale/personal Jira + emails
// + sessions), formats it as a human-readable briefing, and writes it
// into state.db as a special session (source="briefing_smart").

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use crate::config::profile_home;
use crate::sessions::state_db_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingSummary {
    pub period_days: i64,
    pub jira_total_snapshotted: i64,
    pub jira_new: i64,
    pub jira_changed: i64,
    pub jira_stale: i64,
    pub emails_total: i64,
    pub emails_urgent: i64,
    pub sessions_referenced: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingIssue {
    pub key: String,
    pub summary: String,
    pub status: String,
    pub assignee: Option<String>,
    pub updated: String,
    #[serde(default)]
    pub old_status: Option<String>,
    #[serde(default)]
    pub old_assignee: Option<String>,
    #[serde(default)]
    pub urgency: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub is_stale: Option<bool>,
    #[serde(default)]
    pub staleness_days: Option<i64>,
    #[serde(default)]
    pub suggested_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingEmail {
    pub id: String,
    pub subject: String,
    pub from_addr: Option<String>,
    pub date: String,
    #[serde(default)]
    pub urgency: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingSession {
    pub title: String,
    pub source: String,
    pub started_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingPayload {
    pub generated_at: String,
    pub summary: BriefingSummary,
    #[serde(default)]
    pub urgent_jira: Vec<BriefingIssue>,
    #[serde(default)]
    pub important_jira: Vec<BriefingIssue>,
    #[serde(default)]
    pub stale_jira: Vec<BriefingIssue>,
    #[serde(default)]
    pub personal_jira: Vec<BriefingIssue>,
    #[serde(default)]
    pub urgent_emails: Vec<BriefingEmail>,
    #[serde(default)]
    pub personal_emails: Vec<BriefingEmail>,
    #[serde(default)]
    pub work_emails: Vec<BriefingEmail>,
    #[serde(default)]
    pub recent_sessions_sample: Vec<BriefingSession>,
    #[serde(default)]
    pub actionable_suggestions: Vec<BriefingIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefingResult {
    pub session_id: String,
    pub source: String,
    pub started_at: f64,
    pub title: String,
    pub preview: String,
    pub json: BriefingPayload,
    pub formatted: String,
}

fn call_smart_briefing_mcp(hermes_home: &Path, days: i64) -> Result<BriefingPayload, String> {
    // Search both possible home locations: ~/.hermes (user checkout) and
    // %LOCALAPPDATA%\hermes (uv-managed install). On Windows the desktop
    // resolves the install dir, but the user may keep their config in ~/.hermes.
    let candidate_homes: Vec<PathBuf> = {
        let mut v = vec![hermes_home.to_path_buf()];
        if let Some(user_home) = dirs::home_dir() {
            let uh = user_home.join(".hermes");
            if uh != *hermes_home && uh.exists() {
                v.push(uh);
            }
            let local = user_home
                .join("AppData")
                .join("Local")
                .join("hermes");
            if local != *hermes_home && local.exists() {
                v.push(local);
            }
        }
        v
    };
    let server = candidate_homes
        .iter()
        .map(|h| h.join("mcp-smart-briefing/server.py"))
        .find(|p| p.exists())
        .ok_or_else(|| {
            format!(
                "MCP server not found in any of: {}",
                candidate_homes
                    .iter()
                    .map(|h| h.join("mcp-smart-briefing/server.py").display().to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })?;
    let server_home = server.parent().and_then(|p| p.parent()).unwrap_or(hermes_home);

    let python = candidate_homes
        .iter()
        .map(|h| h.join("hermes-agent/venv/Scripts/python.exe"))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "python".to_string());

    // MCP over stdio uses newline-delimited JSON-RPC. We must WRITE the
    // requests to the child's stdin, then read responses from stdout.
    // The previous implementation built `_input` but never wrote it, so the
    // server received EOF immediately and the function always failed with
    // "no JSON-RPC id=2 response in MCP output".
    let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"steersman-desktop","version":"3.2.0"}}}"#;
    let call_req = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"generate_briefing","arguments":{{"days":{}}}}}}}"#,
        days
    );
    let stdin_payload = format!("{}\n{}\n", init_req, call_req);

    // ── Credentials: inherit ALL .env keys (ADR-003 principle: full inheritance) ─
    // The briefing MCP server spawns email/jira MCP servers as its own children
    // via subprocess.run — those children inherit os.environ. Hermes Agent's
    // _build_safe_env() strips non-whitelisted vars, so the briefing server
    // itself was launched without EMAIL_*/JIRA_* creds. We fix this by passing
    // the COMPLETE .env contents into the briefing server's environment, so
    // both the briefing server AND its email/jira children get credentials.
    let mut env_map = crate::config::read_env(server_home, None);
    // hermes_home .env overrides server_home .env (has the complete credential set).
    for (k, v) in crate::config::read_env(hermes_home, None) {
        env_map.insert(k, v);
    }
    // Ensure EMAIL_ADDRESS is set even if only EMAIL_USER exists (mcp-email-life
    // reads EMAIL_ADDRESS at line 52).
    if !env_map.contains_key("EMAIL_ADDRESS") {
        if let Some(user) = env_map.get("EMAIL_USER") {
            env_map.insert("EMAIL_ADDRESS".to_string(), user.clone());
        }
    }

    // Propagate proxy to the MCP child processes.
    let briefing_proxy = crate::config::resolve_effective_proxy(hermes_home, None);
    if !briefing_proxy.is_empty() {
        env_map.insert("HTTP_PROXY".to_string(), briefing_proxy.clone());
        env_map.insert("HTTPS_PROXY".to_string(), briefing_proxy.clone());
        env_map.insert("ALL_PROXY".to_string(), briefing_proxy.clone());
        env_map.insert("http_proxy".to_string(), briefing_proxy.clone());
        env_map.insert("https_proxy".to_string(), briefing_proxy.clone());
        env_map.insert("all_proxy".to_string(), briefing_proxy);
    }

    let mut cmd = Command::new(&python);
    cmd.arg(&server)
        .current_dir(server_home)
        .env("HERMES_HOME", server_home)
        .env("PYTHONIOENCODING", "utf-8");

    // Inject ALL .env keys into the child environment. This is the key fix:
    // instead of a hand-curated allowlist that always missed something, we
    // pass everything so whatever the MCP servers need is available.
    for (key, value) in &env_map {
        cmd.env(key, value);
    }

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn failed: {}", e))?;

    // Write the JSON-RPC requests to stdin, then drop stdin to signal EOF.
    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_payload.as_bytes())
            .map_err(|e| format!("stdin write failed: {}", e))?;
        // Drop stdin to close the pipe; the MCP server reads until EOF.
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait failed: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("MCP exit {}: {}", output.status, err.chars().take(300).collect::<String>()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        // Skip JSON-RPC notifications (no "id") and the initialize response (id=1).
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("id").and_then(|x| x.as_i64()) == Some(2) {
                if let Some(text) = v.pointer("/result/content/0/text").and_then(|x| x.as_str()) {
                    return serde_json::from_str::<BriefingPayload>(text)
                        .map_err(|e| format!("parse payload: {}", e));
                }
            }
        }
    }
    Err(format!(
        "no JSON-RPC id=2 response in MCP output (stdout {} bytes, stderr {} bytes)",
        output.stdout.len(),
        output.stderr.len()
    ))
}

fn format_briefing(p: &BriefingPayload) -> String {
    let dash = "-";
    let mut s = String::new();
    s.push_str(&format!("SMART BRIEFING {} period {} d.\n", dash, p.summary.period_days));
    let when = p.generated_at.replace('T', " ");
    let when = when.split('.').next().unwrap_or("");
    s.push_str(&format!("Generated: {}\n\n", when));
    s.push_str(&format!(
        "Jira: snapshotted={}, new={}, changed={}, stale={}\n",
        p.summary.jira_total_snapshotted,
        p.summary.jira_new,
        p.summary.jira_changed,
        p.summary.jira_stale
    ));
    s.push_str(&format!(
        "Email: total={}, urgent={}\n",
        p.summary.emails_total, p.summary.emails_urgent
    ));
    s.push_str(&format!("Sessions in sample: {}\n", p.summary.sessions_referenced));
    s.push('\n');

    if !p.urgent_jira.is_empty() {
        s.push_str("== URGENT (Jira) ==\n");
        for i in &p.urgent_jira {
            let a = i.assignee.as_deref().map(|x| format!(" @{}", x)).unwrap_or_default();
            s.push_str(&format!("  - [{}] {} {} {}{}\n", i.key, i.summary, dash, i.status, a));
        }
        s.push('\n');
    }
    if !p.important_jira.is_empty() {
        s.push_str("== IMPORTANT (Jira) ==\n");
        for i in &p.important_jira {
            let delta = match (&i.old_status, &i.status) {
                (Some(o), n) if o != n => format!(" (was: {})", o),
                _ => String::new(),
            };
            s.push_str(&format!("  - [{}] {} {} {}{}\n", i.key, i.summary, dash, i.status, delta));
        }
        s.push('\n');
    }
    if !p.stale_jira.is_empty() {
        s.push_str("== STALE (not urgent) ==\n");
        for i in &p.stale_jira {
            let days = i.staleness_days.unwrap_or(0);
            s.push_str(&format!("  - [{}] {} {} {} d.\n", i.key, i.summary, dash, days));
        }
        s.push('\n');
    }
    if !p.urgent_emails.is_empty() {
        s.push_str("== URGENT EMAILS ==\n");
        for m in &p.urgent_emails {
            s.push_str(&format!("  - {} {} from {}\n", m.subject, dash,
                m.from_addr.as_deref().unwrap_or("?")));
        }
        s.push('\n');
    }
    if !p.actionable_suggestions.is_empty() {
        s.push_str("== READY-TO-POST JIRA COMMENTS ==\n");
        for a in &p.actionable_suggestions {
            s.push_str(&format!("  [{}] {}\n", a.key, a.summary));
        }
        s.push('\n');
    }
    if p.urgent_jira.is_empty() && p.important_jira.is_empty()
        && p.stale_jira.is_empty() && p.urgent_emails.is_empty() {
        s.push_str("All clear. No urgent tasks or emails.\n");
    }
    s
}

fn now_ts() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn insert_briefing_session(
    db_path: &Path,
    session_id: &str,
    source: &str,
    started_at: f64,
    title: &str,
    user_msg: &str,
    assistant_msg: &str,
) -> SqliteResult<()> {
    let conn = Connection::open(db_path)?;
    // Wipe any previously-persisted messages for this (deterministic) session
    // id BEFORE inserting, so a repeat briefing replaces the content instead of
    // appending duplicate user/assistant rows under the same session_id.
    conn.execute(
        "DELETE FROM messages WHERE session_id = ?1",
        params![session_id],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO sessions
         (id, source, user_id, model, started_at, ended_at, end_reason,
          message_count, tool_call_count, input_tokens, output_tokens,
          cache_read_tokens, cache_write_tokens, title)
         VALUES (?1, ?2, 'desktop', 'smart-briefing-mcp', ?3, ?3, 'completed',
                 2, 0, 0, 0, 0, 0, ?4)",
        params![session_id, source, started_at, title],
    )?;
    conn.execute(
        "INSERT INTO messages (session_id, role, content, timestamp, finish_reason)
         VALUES (?1, 'user', ?2, ?3, 'stop')",
        params![session_id, user_msg, started_at],
    )?;
    conn.execute(
        "INSERT INTO messages (session_id, role, content, timestamp, finish_reason)
         VALUES (?1, 'assistant', ?2, ?3, 'stop')",
        params![session_id, assistant_msg, started_at + 0.001],
    )?;
    Ok(())
}

pub fn generate_smart_briefing(
    hermes_home: &Path,
    profile: Option<&str>,
    days: i64,
) -> Result<BriefingResult, String> {
    let payload = call_smart_briefing_mcp(hermes_home, days)?;
    let formatted = format_briefing(&payload);

    let started_at = now_ts();
    // Deterministic session id keyed by profile: every briefing run for the
    // same profile overwrites the SAME row (INSERT OR REPLACE) instead of
    // spawning a new `smart_briefing:<ts>` session each time. Previously the
    // id embedded `started_at`, so each call created a fresh session and the
    // DB grew without bound (they were only hidden from the feed by a
    // `NOT LIKE 'smart_briefing:%'` filter). The profile key keeps per-profile
    // briefings distinct while still collapsing repeats.
    let profile_key = profile.unwrap_or("default");
    let session_id = format!("smart_briefing:{}", profile_key);
    let title = format!(
        "Smart Briefing {}d. {} urgent / {} changed",
        payload.summary.period_days,
        payload.urgent_jira.len(),
        payload.important_jira.len()
    );

    let profile_path = profile_home(hermes_home, profile);
    let db_path = state_db_path(&profile_path, None);
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let user_msg = format!("generate_smart_briefing days={}", days);
    if let Err(e) = insert_briefing_session(
        &db_path, &session_id, "briefing_smart",
        started_at, &title, &user_msg, &formatted,
    ) {
        tracing::warn!(target: "steersman_desktop_lib::briefing", error = %e, "persist failed");
    }

    Ok(BriefingResult {
        session_id,
        source: "briefing_smart".into(),
        started_at,
        title,
        preview: formatted.lines().take(6).collect::<Vec<_>>().join("\n"),
        json: payload,
        formatted,
    })
}

/// A cached briefing read back from state.db. The dashboard calls this on mount
/// instead of auto-firing an LLM/MCP call. If the briefing is fresh enough
/// (within `max_age_secs`), the dashboard shows it directly; otherwise it shows
/// a "stale" indicator with a manual refresh button.
#[derive(Debug, Clone, Serialize)]
pub struct CachedBriefing {
    /// The formatted briefing text (assistant message), or null if no briefing exists yet.
    pub text: Option<String>,
    /// Unix timestamp (seconds) when the briefing was generated, or null.
    pub generated_at: Option<f64>,
    /// True if the briefing is older than `max_age_secs` or doesn't exist.
    pub stale: bool,
    /// Human-readable title (session title).
    pub title: Option<String>,
}

/// Read the most recent cached briefing for a profile from state.db.
/// `max_age_secs` controls staleness — 1800 = 30 min.
pub fn get_cached_briefing(
    hermes_home: &Path,
    profile: Option<&str>,
    max_age_secs: f64,
) -> CachedBriefing {
    let profile_key = profile.unwrap_or("default");
    let session_id = format!("smart_briefing:{}", profile_key);

    let db_path = state_db_path(hermes_home, profile);
    if !db_path.exists() {
        return CachedBriefing { text: None, generated_at: None, stale: true, title: None };
    }

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return CachedBriefing { text: None, generated_at: None, stale: true, title: None },
    };

    // Read the session row for started_at + title.
    let (started_at, title): (f64, Option<String>) = match conn.query_row(
        "SELECT started_at, title FROM sessions WHERE id = ?1",
        params![session_id],
        |r| Ok((r.get::<_, f64>(0)?, r.get::<_, Option<String>>(1)?)),
    ) {
        Ok(v) => v,
        Err(_) => return CachedBriefing { text: None, generated_at: None, stale: true, title: None },
    };

    // Read the assistant message (the formatted briefing text).
    let text: Option<String> = conn
        .query_row(
            "SELECT content FROM messages WHERE session_id = ?1 AND role = 'assistant'
             ORDER BY timestamp DESC LIMIT 1",
            params![session_id],
            |r| r.get::<_, String>(0),
        )
        .ok();

    let now = now_ts();
    let age = now - started_at;
    let stale = age > max_age_secs || text.is_none();

    CachedBriefing {
        text,
        generated_at: Some(started_at),
        stale,
        title,
    }
}
