// src-tauri/src/productivity.rs
// Desktop-owned productivity data (tasks, goals, projects, protocols,
// self-checks) — the shturman.ai feature set that Hermes itself doesn't have.
// Stored in `kanban-desktop.db` (separate from the agent's kanban.db) so we
// never clash with its schema.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::config::profile_home;

// ── DB path ───────────────────────────────────────────────────────────────

fn db_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    profile_home(hermes_home, profile).join("kanban-desktop.db")
}

fn open(hermes_home: &Path, profile: Option<&str>) -> Result<Connection, String> {
    let p = db_path(hermes_home, profile);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
    }
    let conn = Connection::open(&p).map_err(|e| format!("open db: {}", e))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            color TEXT DEFAULT '#888',
            description TEXT DEFAULT '',
            created_at INTEGER DEFAULT (strftime('%s','now'))
        );
        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            description TEXT DEFAULT '',
            status TEXT DEFAULT 'todo',
            priority INTEGER DEFAULT 3,
            due_date TEXT,
            project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
            created_at INTEGER DEFAULT (strftime('%s','now')),
            completed_at INTEGER
        );
        CREATE TABLE IF NOT EXISTS goals (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            description TEXT DEFAULT '',
            target_date TEXT,
            progress INTEGER DEFAULT 0,
            created_at INTEGER DEFAULT (strftime('%s','now'))
        );
        CREATE TABLE IF NOT EXISTS protocols (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            participants TEXT DEFAULT '',
            meeting_date TEXT,
            decisions TEXT DEFAULT '',
            risks TEXT DEFAULT '',
            notes TEXT DEFAULT '',
            created_at INTEGER DEFAULT (strftime('%s','now'))
        );
        CREATE TABLE IF NOT EXISTS self_checks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            energy INTEGER,
            joy INTEGER,
            mood TEXT,
            notes TEXT,
            created_at INTEGER DEFAULT (strftime('%s','now'))
        );",
    )
    .map_err(|e| format!("migrate: {}", e))?;
    // Additive migrations: link projects → goals (epic hierarchy: Goal →
    // Projects → Tasks). ALTER TABLE ... ADD COLUMN is idempotent via try/catch.
    let _ = conn.execute(
        "ALTER TABLE projects ADD COLUMN goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL",
        [],
    );
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN assignee TEXT DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN labels TEXT DEFAULT ''", []);
    let _ = conn.execute(
        "ALTER TABLE tasks ADD COLUMN session_id TEXT DEFAULT ''",
        [],
    );
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN section_id INTEGER", []);
    // Sections within projects (like Todoist).
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sections (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            position INTEGER DEFAULT 0
        );",
    );
    // Connection profiles (multiple Hermes servers).
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS connection_profiles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            mode TEXT DEFAULT 'local',
            host TEXT DEFAULT '',
            port INTEGER DEFAULT 22,
            username TEXT DEFAULT '',
            key_path TEXT DEFAULT '',
            api_url TEXT DEFAULT '',
            api_key TEXT DEFAULT ''
        );",
    );
    // ADR-009: external refs — link Jira/email/Confluence items to internal
    // tasks/projects/goals. One external item (e.g. Jira DEVOS-3) can map to
    // multiple internal tasks (subtasks). UNIQUE(source, external_id) prevents
    // duplicate links for the SAME external item to the SAME target.
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_refs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            external_id TEXT NOT NULL,
            external_url TEXT DEFAULT '',
            title TEXT DEFAULT '',
            task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
            project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
            goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL,
            created_at INTEGER DEFAULT (strftime('%s','now')),
            UNIQUE(source, external_id)
        );",
    );
    // ADR-009: session links — attach chat sessions (from state.db) to
    // tasks/projects/goals. session_id is a plain string (no FK — it lives in
    // a different SQLite DB owned by the Hermes backend). One session can link
    // to multiple targets; orphan links (session deleted in state.db) are
    // filtered out at render time.
    // Note: we do NOT add a UNIQUE constraint at the table level because
    // SQLite treats NULL != NULL in UNIQUE indexes, which would allow
    // duplicate links when project_id/goal_id are NULL. Idempotency is
    // enforced in link_session() via application-level check.
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_links (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
            project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
            goal_id INTEGER REFERENCES goals(id) ON DELETE CASCADE,
            linked_at INTEGER DEFAULT (strftime('%s','now')),
            linked_by TEXT DEFAULT 'manual',
            note TEXT DEFAULT ''
        );",
    );
    // Partial unique index: only enforce uniqueness when all FK columns are non-NULL.
    // For cases with NULLs (session→task only, session→project only), we handle
    // deduplication in application code (link_session).
    let _ = conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_session_links_unique_full
         ON session_links (session_id, task_id, project_id, goal_id)
         WHERE task_id IS NOT NULL AND project_id IS NOT NULL AND goal_id IS NOT NULL",
        [],
    );
    Ok(())
}

// ── Tasks ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub status: String,
    pub priority: i64,
    pub due_date: Option<String>,
    pub project_id: Option<i64>,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub labels: String,
    pub section_id: Option<i64>,
    pub created_at: Option<i64>,
    pub completed_at: Option<i64>,
}

pub fn list_tasks(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<Task>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, title, description, status, priority, due_date, project_id, assignee, labels, section_id, created_at, completed_at FROM tasks ORDER BY created_at DESC")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Task {
                id: r.get(0)?,
                title: r.get(1)?,
                description: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                status: r.get(3)?,
                priority: r.get(4)?,
                due_date: r.get(5)?,
                project_id: r.get(6)?,
                assignee: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                labels: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                section_id: r.get(9)?,
                created_at: r.get(10)?,
                completed_at: r.get(11)?,
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    let mut out = Vec::new();
    for r in rows.flatten() {
        out.push(r);
    }
    Ok(out)
}

pub fn create_task(
    hermes_home: &Path,
    profile: Option<&str>,
    title: &str,
    priority: i64,
    due_date: Option<&str>,
    project_id: Option<i64>,
    assignee: &str,
    section_id: Option<i64>,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO tasks (title, priority, due_date, project_id, assignee, section_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![title, priority, due_date, project_id, assignee, section_id],
    )
    .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn update_task_status(
    hermes_home: &Path,
    profile: Option<&str>,
    id: i64,
    status: &str,
) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    let completed = if status == "done" {
        Some(format!("{}", chrono::Utc::now().timestamp()))
    } else {
        None
    };
    conn.execute(
        "UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3",
        params![status, completed, id],
    )
    .map_err(|e| format!("update: {}", e))?;
    Ok(())
}

pub fn delete_task(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(|e| format!("delete: {}", e))?;
    Ok(())
}

/// Generic task update — any of the fields can be None (unchanged).
pub fn update_task(
    hermes_home: &Path,
    profile: Option<&str>,
    id: i64,
    title: Option<&str>,
    priority: Option<i64>,
    due_date: Option<&str>,
    project_id: Option<Option<i64>>,
    assignee: Option<&str>,
    labels: Option<&str>,
    section_id: Option<i64>,
) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(t) = title {
        sets.push("title = ?".into());
        binds.push(Box::new(t.to_string()));
    }
    if let Some(p) = priority {
        sets.push("priority = ?".into());
        binds.push(Box::new(p));
    }
    if let Some(d) = due_date {
        sets.push("due_date = ?".into());
        binds.push(Box::new(d.to_string()));
    }
    if let Some(pi) = project_id {
        sets.push("project_id = ?".into());
        binds.push(Box::new(pi));
    }
    if let Some(a) = assignee {
        sets.push("assignee = ?".into());
        binds.push(Box::new(a.to_string()));
    }
    if let Some(l) = labels {
        sets.push("labels = ?".into());
        binds.push(Box::new(l.to_string()));
    }
    if let Some(sid) = section_id {
        sets.push("section_id = ?".into());
        binds.push(Box::new(sid));
    }
    if sets.is_empty() {
        return Ok(());
    }
    sets.push("id = id".into()); // no-op to ensure non-empty
    let sql = format!("UPDATE tasks SET {} WHERE id = ?", sets.join(", "));
    binds.push(Box::new(id));
    let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, bind_refs.as_slice())
        .map_err(|e| format!("update_task: {}", e))?;
    Ok(())
}

// ── Goals ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub target_date: Option<String>,
    pub progress: i64,
    pub created_at: Option<i64>,
}

pub fn list_goals(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<Goal>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, title, description, target_date, progress, created_at FROM goals ORDER BY created_at DESC")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Goal {
                id: r.get(0)?,
                title: r.get(1)?,
                description: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                target_date: r.get(3)?,
                progress: r.get(4)?,
                created_at: r.get(5)?,
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_goal(
    hermes_home: &Path,
    profile: Option<&str>,
    title: &str,
    target_date: Option<&str>,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO goals (title, target_date) VALUES (?1, ?2)",
        params![title, target_date],
    )
    .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_goal(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM goals WHERE id = ?1", params![id])
        .map_err(|e| format!("delete: {}", e))?;
    Ok(())
}

/// Generic goal update.
pub fn update_goal(
    hermes_home: &Path,
    profile: Option<&str>,
    id: i64,
    title: Option<&str>,
    target_date: Option<&str>,
    progress: Option<i64>,
) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(v) = title {
        sets.push("title = ?".into());
        binds.push(Box::new(v.to_string()));
    }
    if let Some(v) = target_date {
        sets.push("target_date = ?".into());
        binds.push(Box::new(v.to_string()));
    }
    if let Some(v) = progress {
        sets.push("progress = ?".into());
        binds.push(Box::new(v));
    }
    if sets.is_empty() {
        return Ok(());
    }
    sets.push("id = id".into());
    let sql = format!("UPDATE goals SET {} WHERE id = ?", sets.join(", "));
    binds.push(Box::new(id));
    let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, bind_refs.as_slice())
        .map_err(|e| format!("update_goal: {}", e))?;
    Ok(())
}

// ── Projects ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub goal_id: Option<i64>,
    pub created_at: Option<i64>,
}

pub fn list_projects(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<Project>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, color, description, goal_id, created_at FROM projects ORDER BY name",
        )
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r
                    .get::<_, Option<String>>(2)?
                    .unwrap_or_else(|| "#888".into()),
                description: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                goal_id: r.get(4).ok().flatten(),
                created_at: r.get(5)?,
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_project(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
    color: &str,
    goal_id: Option<i64>,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO projects (name, color, goal_id) VALUES (?1, ?2, ?3)",
        params![name, color, goal_id],
    )
    .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_project(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| format!("delete: {}", e))?;
    Ok(())
}

// ── Sections (sub-groups within projects, like Todoist) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub position: i64,
}

pub fn list_sections(
    hermes_home: &Path,
    profile: Option<&str>,
    project_id: i64,
) -> Result<Vec<Section>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, project_id, name, position FROM sections WHERE project_id = ?1 ORDER BY position")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map(params![project_id], |r| {
            Ok(Section {
                id: r.get(0)?,
                project_id: r.get(1)?,
                name: r.get(2)?,
                position: r.get(3)?,
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_section(
    hermes_home: &Path,
    profile: Option<&str>,
    project_id: i64,
    name: &str,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO sections (project_id, name) VALUES (?1, ?2)",
        params![project_id, name],
    )
    .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_section(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM sections WHERE id = ?1", params![id])
        .map_err(|e| format!("delete: {}", e))?;
    Ok(())
}

// ── Connection Profiles (multiple Hermes servers) ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: i64,
    pub name: String,
    pub mode: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub key_path: String,
    pub api_url: String,
    #[serde(skip_serializing)]
    pub api_key: String,
}

pub fn list_profiles(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Result<Vec<ConnectionProfile>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, name, mode, host, port, username, key_path, api_url, api_key FROM connection_profiles ORDER BY name")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ConnectionProfile {
                id: r.get(0)?,
                name: r.get(1)?,
                mode: r.get(2)?,
                host: r.get(3)?,
                port: r.get(4)?,
                username: r.get(5)?,
                key_path: r.get(6)?,
                api_url: r.get(7)?,
                api_key: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_profile(
    hermes_home: &Path,
    profile: Option<&str>,
    name: &str,
    mode: &str,
    host: &str,
    port: i64,
    username: &str,
    key_path: &str,
    api_url: &str,
    api_key: &str,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO connection_profiles (name, mode, host, port, username, key_path, api_url, api_key) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![name, mode, host, port, username, key_path, api_url, api_key],
    ).map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_profile(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM connection_profiles WHERE id = ?1", params![id])
        .map_err(|e| format!("delete: {}", e))?;
    Ok(())
}

/// Generic project update.
pub fn update_project(
    hermes_home: &Path,
    profile: Option<&str>,
    id: i64,
    name: Option<&str>,
    color: Option<&str>,
    goal_id: Option<Option<i64>>,
) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(v) = name {
        sets.push("name = ?".into());
        binds.push(Box::new(v.to_string()));
    }
    if let Some(v) = color {
        sets.push("color = ?".into());
        binds.push(Box::new(v.to_string()));
    }
    if let Some(v) = goal_id {
        sets.push("goal_id = ?".into());
        binds.push(Box::new(v));
    }
    if sets.is_empty() {
        return Ok(());
    }
    sets.push("id = id".into());
    let sql = format!("UPDATE projects SET {} WHERE id = ?", sets.join(", "));
    binds.push(Box::new(id));
    let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, bind_refs.as_slice())
        .map_err(|e| format!("update_project: {}", e))?;
    Ok(())
}

// ── Protocols ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protocol {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub participants: String,
    pub meeting_date: Option<String>,
    #[serde(default)]
    pub decisions: String,
    #[serde(default)]
    pub risks: String,
    #[serde(default)]
    pub notes: String,
    pub created_at: Option<i64>,
}

pub fn list_protocols(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<Protocol>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, title, participants, meeting_date, decisions, risks, notes, created_at FROM protocols ORDER BY created_at DESC")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Protocol {
                id: r.get(0)?,
                title: r.get(1)?,
                participants: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                meeting_date: r.get(3)?,
                decisions: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                risks: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                notes: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                created_at: r.get(7)?,
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_protocol(
    hermes_home: &Path,
    profile: Option<&str>,
    title: &str,
    participants: &str,
    meeting_date: Option<&str>,
    decisions: &str,
    risks: &str,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO protocols (title, participants, meeting_date, decisions, risks) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![title, participants, meeting_date, decisions, risks],
    )
    .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_protocol(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM protocols WHERE id = ?1", params![id])
        .map_err(|e| format!("delete: {}", e))?;
    Ok(())
}

// ── Self-checks (statistics) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheck {
    pub id: i64,
    pub energy: Option<i64>,
    pub joy: Option<i64>,
    pub mood: Option<String>,
    pub notes: Option<String>,
    pub created_at: Option<i64>,
}

pub fn list_self_checks(
    hermes_home: &Path,
    profile: Option<&str>,
) -> Result<Vec<SelfCheck>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, energy, joy, mood, notes, created_at FROM self_checks ORDER BY created_at DESC LIMIT 60")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SelfCheck {
                id: r.get(0)?,
                energy: r.get(1)?,
                joy: r.get(2)?,
                mood: r.get(3)?,
                notes: r.get(4)?,
                created_at: r.get(5)?,
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn add_self_check(
    hermes_home: &Path,
    profile: Option<&str>,
    energy: i64,
    joy: i64,
    mood: &str,
    notes: &str,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO self_checks (energy, joy, mood, notes) VALUES (?1, ?2, ?3, ?4)",
        params![energy, joy, mood, notes],
    )
    .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

// ── Dashboard stats ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DashStats {
    pub tasks_total: i64,
    pub tasks_done: i64,
    pub tasks_today: i64,
    /// Active (not done) tasks — the number the dashboard shows prominently.
    pub active_tasks: i64,
    /// Overdue: due_date < today AND not done. NULL due_date excluded.
    pub overdue_tasks: i64,
    pub goals_total: i64,
    pub projects_total: i64,
    /// Protocol count for the dashboard tile.
    pub protocols: i64,
}

pub fn dash_stats(hermes_home: &Path, profile: Option<&str>) -> Result<DashStats, String> {
    let conn = open(hermes_home, profile)?;
    let tasks_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap_or(0);
    let tasks_done: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE status='done'", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let active_tasks = tasks_total - tasks_done;
    // today = due_date is today (YYYY-MM-DD)
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let tasks_today: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE due_date = ?1 AND status != 'done'",
            params![today],
            |r| r.get(0),
        )
        .unwrap_or(0);
    // Overdue: due_date is set, in the past, and task not done.
    let overdue_tasks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE due_date IS NOT NULL AND due_date != '' AND due_date < ?1 AND status != 'done'",
            params![today],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let goals_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals", [], |r| r.get(0))
        .unwrap_or(0);
    let projects_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
        .unwrap_or(0);
    let protocols: i64 = conn
        .query_row("SELECT COUNT(*) FROM protocols", [], |r| r.get(0))
        .unwrap_or(0);
    Ok(DashStats {
        tasks_total,
        tasks_done,
        tasks_today,
        active_tasks,
        overdue_tasks,
        goals_total,
        projects_total,
        protocols,
    })
}

// ── ADR-009: External refs (Jira/email → internal tasks) ───────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalRef {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    #[serde(default)]
    pub external_url: String,
    #[serde(default)]
    pub title: String,
    pub task_id: Option<i64>,
    pub project_id: Option<i64>,
    pub goal_id: Option<i64>,
    pub created_at: Option<i64>,
}

/// Link an external item (Jira key, email Message-ID, Confluence page) to an
/// internal task/project/goal. UPSERT by (source, external_id) so re-linking
/// updates the target instead of erroring.
pub fn upsert_external_ref(
    hermes_home: &Path,
    profile: Option<&str>,
    source: &str,
    external_id: &str,
    external_url: Option<&str>,
    title: Option<&str>,
    task_id: Option<i64>,
    project_id: Option<i64>,
    goal_id: Option<i64>,
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO external_refs (source, external_id, external_url, title, task_id, project_id, goal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(source, external_id) DO UPDATE SET
            external_url = COALESCE(excluded.external_url, external_refs.external_url),
            title = COALESCE(excluded.title, external_refs.title),
            task_id = COALESCE(excluded.task_id, external_refs.task_id),
            project_id = COALESCE(excluded.project_id, external_refs.project_id),
            goal_id = COALESCE(excluded.goal_id, external_refs.goal_id)",
        params![source, external_id, external_url, title, task_id, project_id, goal_id],
    )
    .map_err(|e| format!("upsert external_ref: {}", e))?;
    Ok(conn.last_insert_rowid())
}

/// Look up whether an external item is already linked to an internal entity.
pub fn get_external_ref(
    hermes_home: &Path,
    profile: Option<&str>,
    source: &str,
    external_id: &str,
) -> Result<Option<ExternalRef>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, source, external_id, external_url, title, task_id, project_id, goal_id, created_at
             FROM external_refs WHERE source = ?1 AND external_id = ?2",
        )
        .map_err(|e| format!("prepare: {}", e))?;
    let row = stmt
        .query_row(params![source, external_id], |r| {
            Ok(ExternalRef {
                id: r.get(0)?,
                source: r.get(1)?,
                external_id: r.get(2)?,
                external_url: r.get(3).unwrap_or_default(),
                title: r.get(4).unwrap_or_default(),
                task_id: r.get(5)?,
                project_id: r.get(6)?,
                goal_id: r.get(7)?,
                created_at: r.get(8).ok(),
            })
        })
        .ok();
    Ok(row)
}

// ── ADR-009: Session links (chat sessions → tasks/projects/goals) ──────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLink {
    pub id: i64,
    pub session_id: String,
    pub task_id: Option<i64>,
    pub project_id: Option<i64>,
    pub goal_id: Option<i64>,
    pub linked_at: Option<i64>,
    #[serde(default)]
    pub linked_by: String,
    #[serde(default)]
    pub note: String,
}

/// Link a chat session to a task/project/goal. Idempotent: checks for existing
/// link first (SQLite UNIQUE constraint treats NULL != NULL, so we handle
/// idempotency in application logic instead of relying solely on the index).
pub fn link_session(
    hermes_home: &Path,
    profile: Option<&str>,
    session_id: &str,
    task_id: Option<i64>,
    project_id: Option<i64>,
    goal_id: Option<i64>,
    linked_by: Option<&str>,
    note: Option<&str>,
) -> Result<i64, String> {
    if task_id.is_none() && project_id.is_none() && goal_id.is_none() {
        return Err("at least one of task_id/project_id/goal_id is required".to_string());
    }
    let conn = open(hermes_home, profile)?;

    // Check if link already exists (handles NULLs correctly where UNIQUE index doesn't)
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM session_links WHERE session_id = ?1 AND task_id IS ?2 AND project_id IS ?3 AND goal_id IS ?4 LIMIT 1",
            params![session_id, task_id, project_id, goal_id],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if exists {
        // Return the existing link's id
        let existing_id: i64 = conn
            .query_row(
                "SELECT id FROM session_links WHERE session_id = ?1 AND task_id IS ?2 AND project_id IS ?3 AND goal_id IS ?4 LIMIT 1",
                params![session_id, task_id, project_id, goal_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("get existing link: {}", e))?;
        return Ok(existing_id);
    }

    conn.execute(
        "INSERT INTO session_links (session_id, task_id, project_id, goal_id, linked_by, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session_id,
            task_id,
            project_id,
            goal_id,
            linked_by.unwrap_or("manual"),
            note.unwrap_or("")
        ],
    )
    .map_err(|e| format!("link_session: {}", e))?;
    Ok(conn.last_insert_rowid())
}

/// Remove a session link by id.
pub fn unlink_session(
    hermes_home: &Path,
    profile: Option<&str>,
    link_id: i64,
) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM session_links WHERE id = ?1", params![link_id])
        .map_err(|e| format!("unlink_session: {}", e))?;
    Ok(())
}

/// Get all links for a given session (to show badges in the session list).
pub fn get_session_links(
    hermes_home: &Path,
    profile: Option<&str>,
    session_id: &str,
) -> Result<Vec<SessionLink>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, task_id, project_id, goal_id, linked_at, linked_by, note
             FROM session_links WHERE session_id = ?1 ORDER BY linked_at DESC",
        )
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok(SessionLink {
                id: r.get(0)?,
                session_id: r.get(1)?,
                task_id: r.get(2)?,
                project_id: r.get(3)?,
                goal_id: r.get(4)?,
                linked_at: r.get(5).ok(),
                linked_by: r.get(6).unwrap_or_default(),
                note: r.get(7).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("row: {}", e))
}

/// Get all session_ids linked to a given task (for the task detail view).
pub fn get_links_for_task(
    hermes_home: &Path,
    profile: Option<&str>,
    task_id: i64,
) -> Result<Vec<SessionLink>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, task_id, project_id, goal_id, linked_at, linked_by, note
             FROM session_links WHERE task_id = ?1 ORDER BY linked_at DESC",
        )
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map(params![task_id], |r| {
            Ok(SessionLink {
                id: r.get(0)?,
                session_id: r.get(1)?,
                task_id: r.get(2)?,
                project_id: r.get(3)?,
                goal_id: r.get(4)?,
                linked_at: r.get(5).ok(),
                linked_by: r.get(6).unwrap_or_default(),
                note: r.get(7).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("query: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("row: {}", e))
}

/// Create an internal task AND link it to an external item in one transaction.
/// Used by "Jira → В задачу": atomically create_task + upsert_external_ref.
pub fn create_task_from_external(
    hermes_home: &Path,
    profile: Option<&str>,
    source: &str,
    external_id: &str,
    external_url: Option<&str>,
    title: &str,
    priority: i64,
    due_date: Option<&str>,
    project_id: Option<i64>,
    goal_id: Option<i64>,
    assignee: &str,
) -> Result<i64, String> {
    let mut conn = open(hermes_home, profile)?;
    let tx = conn.transaction().map_err(|e| format!("begin tx: {}", e))?;
    tx.execute(
        "INSERT INTO tasks (title, priority, due_date, project_id, assignee) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![title, priority, due_date, project_id, assignee],
    )
    .map_err(|e| format!("insert task: {}", e))?;
    let task_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO external_refs (source, external_id, external_url, title, task_id, project_id, goal_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(source, external_id) DO UPDATE SET task_id = ?5, project_id = ?6, goal_id = ?7, title = ?4",
        params![source, external_id, external_url, title, task_id, project_id, goal_id],
    )
    .map_err(|e| format!("insert external_ref: {}", e))?;
    tx.commit().map_err(|e| format!("commit: {}", e))?;
    Ok(task_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "steersman-prod-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn external_refs_table_created_by_migrate() {
        let dir = tempdir();
        // Opening the DB runs migrate(); the tables must exist after.
        let conn = open(&dir, None).unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(
            tables.contains(&"external_refs".to_string()),
            "external_refs missing: {:?}",
            tables
        );
        assert!(
            tables.contains(&"session_links".to_string()),
            "session_links missing: {:?}",
            tables
        );
    }

    #[test]
    fn upsert_external_ref_inserts_then_updates() {
        let dir = tempdir();
        // Create a task first (FK target).
        let task_id = create_task(&dir, None, "Test task", 3, None, None, "", None).unwrap();
        // Create a project to link to (FK target for project_id).
        let project_id = create_project(&dir, None, "Test Project", "#888", None).unwrap();
        // First insert: link DEVOS-3 to the task.
        upsert_external_ref(
            &dir,
            None,
            "jira",
            "DEVOS-3",
            Some("https://jira/DEVOS-3"),
            Some("Fix bug"),
            Some(task_id),
            None,
            None,
        )
        .unwrap();
        let r1 = get_external_ref(&dir, None, "jira", "DEVOS-3")
            .unwrap()
            .unwrap();
        assert_eq!(r1.task_id, Some(task_id));
        assert_eq!(r1.title, "Fix bug");

        // Upsert again with a project link — should UPDATE, not duplicate.
        upsert_external_ref(
            &dir,
            None,
            "jira",
            "DEVOS-3",
            None,
            Some("Fix bug v2"),
            Some(task_id),
            Some(project_id),
            None,
        )
        .unwrap();
        let r2 = get_external_ref(&dir, None, "jira", "DEVOS-3")
            .unwrap()
            .unwrap();
        assert_eq!(r2.project_id, Some(project_id), "project_id should update");
        assert_eq!(r2.title, "Fix bug v2", "title should update");

        // Only one row for (jira, DEVOS-3).
        let count: i64 = open(&dir, None)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM external_refs WHERE source='jira' AND external_id='DEVOS-3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "should be exactly 1 row after upsert");
    }

    #[test]
    fn get_external_ref_returns_none_for_unknown() {
        let dir = tempdir();
        let r = get_external_ref(&dir, None, "jira", "NONEXIST-1").unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn link_session_requires_at_least_one_target() {
        let dir = tempdir();
        let result = link_session(&dir, None, "sess1", None, None, None, None, None);
        assert!(result.is_err(), "must error when no task/project/goal");
    }

    #[test]
    fn link_session_and_get_round_trip() {
        let dir = tempdir();
        // Create a task first (FK target).
        let task_id = create_task(&dir, None, "Test task", 3, None, None, "", None).unwrap();
        // Link a session to it.
        link_session(
            &dir,
            None,
            "sess-abc",
            Some(task_id),
            None,
            None,
            Some("manual"),
            Some("discussed"),
        )
        .unwrap();
        // Query back.
        let links = get_session_links(&dir, None, "sess-abc").unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].task_id, Some(task_id));
        assert_eq!(links[0].linked_by, "manual");
        assert_eq!(links[0].note, "discussed");
    }

    #[test]
    fn link_session_is_idempotent() {
        let dir = tempdir();
        let task_id = create_task(&dir, None, "T", 3, None, None, "", None).unwrap();
        link_session(&dir, None, "s1", Some(task_id), None, None, None, None).unwrap();
        // Same link again — no error, no duplicate.
        link_session(&dir, None, "s1", Some(task_id), None, None, None, None).unwrap();
        let links = get_session_links(&dir, None, "s1").unwrap();
        assert_eq!(links.len(), 1, "duplicate link must be deduped");
    }

    #[test]
    fn get_links_for_task_returns_sessions() {
        let dir = tempdir();
        let task_id = create_task(&dir, None, "T", 3, None, None, "", None).unwrap();
        link_session(&dir, None, "sess-1", Some(task_id), None, None, None, None).unwrap();
        link_session(&dir, None, "sess-2", Some(task_id), None, None, None, None).unwrap();
        let links = get_links_for_task(&dir, None, task_id).unwrap();
        assert_eq!(links.len(), 2, "both sessions should be linked to the task");
        let session_ids: Vec<&str> = links.iter().map(|l| l.session_id.as_str()).collect();
        assert!(session_ids.contains(&"sess-1"));
        assert!(session_ids.contains(&"sess-2"));
    }

    #[test]
    fn create_task_from_external_is_atomic() {
        let dir = tempdir();
        // Create a task from a Jira issue — both task and external_ref must exist.
        let task_id = create_task_from_external(
            &dir,
            None,
            "jira",
            "INT-6515",
            Some("https://jira/INT-6515"),
            "Настроить MCP",
            3,
            None,
            None,
            None,
            "ngusev",
        )
        .unwrap();
        assert!(task_id > 0);
        // The task exists.
        let tasks = list_tasks(&dir, None).unwrap();
        assert!(tasks
            .iter()
            .any(|t| t.id == task_id && t.title == "Настроить MCP"));
        // The external_ref exists and points to the task.
        let r = get_external_ref(&dir, None, "jira", "INT-6515")
            .unwrap()
            .unwrap();
        assert_eq!(r.task_id, Some(task_id));
        assert_eq!(r.title, "Настроить MCP");
    }

    #[test]
    fn unlink_session_removes_link() {
        let dir = tempdir();
        let task_id = create_task(&dir, None, "T", 3, None, None, "", None).unwrap();
        let link_id =
            link_session(&dir, None, "s1", Some(task_id), None, None, None, None).unwrap();
        unlink_session(&dir, None, link_id).unwrap();
        let links = get_session_links(&dir, None, "s1").unwrap();
        assert!(links.is_empty(), "link should be removed");
    }
}
