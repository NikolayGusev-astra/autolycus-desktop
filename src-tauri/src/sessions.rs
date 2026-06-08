// src-tauri/src/sessions.rs
// Session management: SQLite state.db access
// Ported from fathah/hermes-desktop src/main/sessions.rs

use std::path::Path;

use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use crate::config::profile_home;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub source: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub message_count: i64,
    pub model: String,
    pub title: Option<String>,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub session_id: String,
    pub title: Option<String>,
    pub started_at: i64,
    pub source: String,
    pub message_count: i64,
    pub model: String,
    pub snippet: String,
}

// ── State DB path ─────────────────────────────────────────────────────────

pub fn state_db_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    profile_home(hermes_home, profile).join("state.db")
}

// ── List sessions ─────────────────────────────────────────────────────────

pub fn list_sessions(
    hermes_home: &Path,
    profile: Option<&str>,
    limit: i64,
    offset: i64,
) -> SqliteResult<Vec<SessionSummary>> {
    let db_path = state_db_path(hermes_home, profile);

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let conn = Connection::open(&db_path)?;

    let mut stmt = conn.prepare(
        "SELECT s.id, s.source, s.started_at, s.ended_at, s.message_count, s.model, s.title,
                COALESCE(
                    (SELECT SUBSTR(m.content, 1, 200) FROM messages m 
                     WHERE m.session_id = s.id AND m.role = 'user' 
                     ORDER BY m.timestamp ASC LIMIT 1),
                    ''
                ) as preview
         FROM sessions s
         ORDER BY s.started_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;

    let sessions = stmt
        .query_map(params![limit, offset], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                source: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
                message_count: row.get(4)?,
                model: row.get(5)?,
                title: row.get(6)?,
                preview: row.get(7)?,
            })
        })?
        .collect::<SqliteResult<Vec<_>>>()?;

    Ok(sessions)
}

// ── Get session messages ──────────────────────────────────────────────────

pub fn get_session_messages(
    hermes_home: &Path,
    profile: Option<&str>,
    session_id: &str,
) -> SqliteResult<Vec<SessionMessage>> {
    let db_path = state_db_path(hermes_home, profile);

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let conn = Connection::open(&db_path)?;

    let mut stmt = conn.prepare(
        "SELECT m.id, m.role, m.content, m.timestamp
         FROM messages m
         WHERE m.session_id = ?1
         ORDER BY m.timestamp ASC",
    )?;

    let messages = stmt
        .query_map(params![session_id], |row| {
            Ok(SessionMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                timestamp: row.get(3)?,
            })
        })?
        .collect::<SqliteResult<Vec<_>>>()?;

    Ok(messages)
}

// ── Search sessions ───────────────────────────────────────────────────────

pub fn search_sessions(
    hermes_home: &Path,
    profile: Option<&str>,
    query: &str,
    limit: i64,
) -> SqliteResult<Vec<SearchResult>> {
    let db_path = state_db_path(hermes_home, profile);

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let conn = Connection::open(&db_path)?;

    // Try FTS5 first, fallback to LIKE
    let fts_result = conn.prepare(
        "SELECT s.id, s.title, s.started_at, s.source, s.message_count, s.model,
                snippet(sessions_fts, 1, '<b>', '</b>', '...', 32) as snippet
         FROM sessions_fts fts
         JOIN sessions s ON s.id = fts.id
         WHERE sessions_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    );

    match fts_result {
        Ok(mut stmt) => {
            let results = stmt
                .query_map(params![query, limit], |row| {
                    Ok(SearchResult {
                        session_id: row.get(0)?,
                        title: row.get(1)?,
                        started_at: row.get(2)?,
                        source: row.get(3)?,
                        message_count: row.get(4)?,
                        model: row.get(5)?,
                        snippet: row.get(6)?,
                    })
                })?
                .collect::<SqliteResult<Vec<_>>>()?;
            if !results.is_empty() {
                return Ok(results);
            }
        }
        Err(_) => {} // FTS not available, fallback
    }

    // Fallback: LIKE search
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.started_at, s.source, s.message_count, s.model,
                SUBSTR(
                    (SELECT m.content FROM messages m 
                     WHERE m.session_id = s.id 
                     ORDER BY m.timestamp ASC LIMIT 1),
                    1, 200
                ) as snippet
         FROM sessions s
         WHERE s.id LIKE ?1 OR s.title LIKE ?2
         ORDER BY s.started_at DESC
         LIMIT ?3",
    )?;

    let results = stmt
        .query_map(params![&pattern, &pattern, limit], |row| {
            Ok(SearchResult {
                session_id: row.get(0)?,
                title: row.get(1)?,
                started_at: row.get(2)?,
                source: row.get(3)?,
                message_count: row.get(4)?,
                model: row.get(5)?,
                snippet: row.get(6).unwrap_or_default(),
            })
        })?
        .collect::<SqliteResult<Vec<_>>>()?;

    Ok(results)
}

// ── Delete session ────────────────────────────────────────────────────────

pub fn delete_session(
    hermes_home: &Path,
    profile: Option<&str>,
    session_id: &str,
) -> SqliteResult<()> {
    let db_path = state_db_path(hermes_home, profile);

    if !db_path.exists() {
        return Ok(());
    }

    let conn = Connection::open(&db_path)?;
    conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;

    Ok(())
}

// ── Delete multiple sessions ──────────────────────────────────────────────

pub fn delete_sessions(
    hermes_home: &Path,
    profile: Option<&str>,
    session_ids: &[String],
) -> SqliteResult<(usize, usize)> {
    let db_path = state_db_path(hermes_home, profile);

    if !db_path.exists() {
        return Ok((session_ids.len(), 0));
    }

    let conn = Connection::open(&db_path)?;
    let mut deleted = 0;

    for id in session_ids {
        let rows = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        if rows > 0 {
            conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])?;
            deleted += 1;
        }
    }

    Ok((session_ids.len(), deleted))
}

// ── Session stats ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SessionStats {
    pub total_sessions: i64,
    pub total_messages: i64,
}

pub fn get_session_stats(
    hermes_home: &Path,
    profile: Option<&str>,
) -> SqliteResult<SessionStats> {
    let db_path = state_db_path(hermes_home, profile);

    if !db_path.exists() {
        return Ok(SessionStats {
            total_sessions: 0,
            total_messages: 0,
        });
    }

    let conn = Connection::open(&db_path)?;

    let total_sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .unwrap_or(0);

    let total_messages: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(SessionStats {
        total_sessions,
        total_messages,
    })
}
