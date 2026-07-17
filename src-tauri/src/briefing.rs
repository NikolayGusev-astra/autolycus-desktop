// src-tauri/src/briefing.rs
// Briefing: sends an analysis prompt to the running Hermes Agent via the
// WebSocket /api/ws transport (ADR-004). The agent has direct access to all
// connected sources (email via himalaya MCP, Jira MCP, Telegram, chat
// sessions in state.db) and the LLM for reasoning — no separate Python
// briefing server needed.
//
// This replaces the old mcp-smart-briefing/server.py subprocess approach,
// which spawned additional email/jira MCP children (losing credentials),
// read the wrong DB path, and had zero LLM analysis.

use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::config::profile_home;
use crate::sessions::state_db_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingResult {
    pub session_id: String,
    pub source: String,
    pub started_at: i64,
    pub title: String,
    pub preview: String,
    pub formatted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedBriefing {
    pub session_id: String,
    pub title: String,
    pub text: String,
    pub started_at: i64,
    pub age_secs: f64,
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The briefing prompt sent to the agent. The agent uses its own tools
/// (email, jira, sessions) to gather data, then the LLM analyzes and
/// produces actionable suggestions.
pub fn briefing_prompt(days: i64) -> String {
    format!(
        r#"Подготовь брифинг за последние {days} дней. Используй свои инструменты:

1. **Email** — проверь входящие через email MCP. Найди срочные, важные, требующие ответа.
2. **Jira** — проверь задачи через jira MCP. Найди просроченные, зависшие, изменившие статус.
3. **Чат-сессии** — проанализируй недавние обсуждения. Свяжи с задачами:
   - Если задача обсуждена и выполнена в чате → отметь, что нужна смена статуса
   - Если задача ждёт ответа, но ответа не было → предложи, что ответить
4. **Готовность** — общий обзор: что сделано, что в работе, что требует внимания.

Формат ответа:
## Срочное
- [кратко, с указанием источника: email/jira/chat]

## Важное
- ...

## Задачи — статус и связь с обсуждениями
- Задача KEY: статус. Обсуждалась в чате: да/нет. Действие: отметить выполнено / нужен ответ

## Предложения ответов
- На [письмо/сообщение от X]: предложенный краткий ответ

## Общая готовность
- Выполнено: N. В работе: M. Требует внимания: K."#,
        days = days
    )
}

/// Generate a briefing by sending an analysis prompt to the running Hermes
/// Agent via the WebSocket transport. The agent gathers data from its own
/// connected sources (email, jira, sessions) and produces an LLM-analyzed
/// briefing with actionable suggestions.
///
/// This replaces the old mcp-smart-briefing/server.py subprocess approach.
pub async fn generate_smart_briefing(
    hermes_home: &Path,
    profile: Option<&str>,
    days: i64,
) -> Result<BriefingResult, String> {
    let started_at = now_ts();
    let prompt = briefing_prompt(days);

    // The agent's response is streamed as chat_event Tauri events (tokens),
    // but for the briefing we need the FULL text synchronously. We read it
    // from the session transcript after the turn completes.
    //
    // The briefing is sent as a chat message via the WS transport, just like
    // a regular user prompt. The frontend ChatView picks up the streaming
    // tokens; here we persist the result and return a preview.
    //
    // NOTE: the actual WS call happens in the Tauri command layer
    // (generate_smart_briefing_cmd in lib.rs), because it needs the AppHandle
    // (for emit) and the GatewayState (for port/token). This function is the
    // formatting + persistence layer.

    let session_id = format!("smart_briefing:{}", profile.unwrap_or("default"));
    let title = format!("Briefing {}d via agent", days);

    // Persist a placeholder; the real text arrives via the WS stream and
    // gets written when the turn completes (or the frontend reads it from
    // the session messages).
    let profile_path = profile_home(hermes_home, profile);
    let db_path = state_db_path(&profile_path, None);
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Write the prompt as the user message so it shows in the transcript.
    if let Err(e) = insert_briefing_session(
        &db_path,
        &session_id,
        "briefing_agent",
        started_at,
        &title,
        &prompt,
        "(briefing generation in progress — see chat)",
    ) {
        tracing::warn!(target: "steersman_desktop_lib::briefing", error = %e, "persist failed");
    }

    Ok(BriefingResult {
        session_id,
        source: "briefing_agent".into(),
        started_at,
        title,
        preview: prompt.lines().take(4).collect::<Vec<_>>().join("\n"),
        formatted: prompt,
    })
}

/// Read the most recent cached briefing from state.db.
pub fn get_cached_briefing(
    hermes_home: &Path,
    profile: Option<&str>,
    max_age_secs: f64,
) -> CachedBriefing {
    let session_id = format!("smart_briefing:{}", profile.unwrap_or("default"));
    let profile_path = profile_home(hermes_home, profile);
    let db_path = state_db_path(&profile_path, None);

    let now = now_ts() as f64;
    let mut text = String::new();
    let mut title = String::new();
    let mut started_at = 0i64;

    if db_path.exists() {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            if let Ok(row) = conn.query_row(
                "SELECT title, preview, started_at FROM sessions WHERE id = ?1",
                rusqlite::params![&session_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0).unwrap_or_default(),
                        r.get::<_, String>(1).unwrap_or_default(),
                        r.get::<_, i64>(2).unwrap_or(0),
                    ))
                },
            ) {
                title = row.0;
                text = row.1;
                started_at = row.2;
            }
        }
    }

    CachedBriefing {
        session_id,
        title,
        text,
        started_at,
        age_secs: now - started_at as f64,
    }
}

// ── state.db session insert ───────────────────────────────────────────────

fn insert_briefing_session(
    db_path: &Path,
    session_id: &str,
    source: &str,
    started_at: i64,
    title: &str,
    user_msg: &str,
    assistant_msg: &str,
) -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open(db_path)?;
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, source, started_at, title, preview)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![session_id, source, started_at, title, assistant_msg],
    )?;
    Ok(())
}
