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
    let _ = conn.execute("ALTER TABLE projects ADD COLUMN goal_id INTEGER REFERENCES goals(id) ON DELETE SET NULL", []);
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN assignee TEXT DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN labels TEXT DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE tasks ADD COLUMN session_id TEXT DEFAULT ''", []);
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
    pub created_at: Option<i64>,
    pub completed_at: Option<i64>,
}

pub fn list_tasks(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<Task>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, title, description, status, priority, due_date, project_id, assignee, labels, created_at, completed_at FROM tasks ORDER BY created_at DESC")
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
                created_at: r.get(9)?,
                completed_at: r.get(10)?,
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
) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO tasks (title, priority, due_date, project_id) VALUES (?1, ?2, ?3, ?4)",
        params![title, priority, due_date, project_id],
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
) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(t) = title { sets.push("title = ?".into()); binds.push(Box::new(t.to_string())); }
    if let Some(p) = priority { sets.push("priority = ?".into()); binds.push(Box::new(p)); }
    if let Some(d) = due_date { sets.push("due_date = ?".into()); binds.push(Box::new(d.to_string())); }
    if let Some(pi) = project_id { sets.push("project_id = ?".into()); binds.push(Box::new(pi)); }
    if let Some(a) = assignee { sets.push("assignee = ?".into()); binds.push(Box::new(a.to_string())); }
    if let Some(l) = labels { sets.push("labels = ?".into()); binds.push(Box::new(l.to_string())); }
    if sets.is_empty() { return Ok(()); }
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
    if let Some(v) = title { sets.push("title = ?".into()); binds.push(Box::new(v.to_string())); }
    if let Some(v) = target_date { sets.push("target_date = ?".into()); binds.push(Box::new(v.to_string())); }
    if let Some(v) = progress { sets.push("progress = ?".into()); binds.push(Box::new(v)); }
    if sets.is_empty() { return Ok(()); }
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
        .prepare("SELECT id, name, color, description, goal_id, created_at FROM projects ORDER BY name")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get::<_, Option<String>>(2)?.unwrap_or_else(|| "#888".into()),
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

pub fn list_sections(hermes_home: &Path, profile: Option<&str>, project_id: i64) -> Result<Vec<Section>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, project_id, name, position FROM sections WHERE project_id = ?1 ORDER BY position")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok(Section { id: r.get(0)?, project_id: r.get(1)?, name: r.get(2)?, position: r.get(3)? })
    }).map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_section(hermes_home: &Path, profile: Option<&str>, project_id: i64, name: &str) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("INSERT INTO sections (project_id, name) VALUES (?1, ?2)", params![project_id, name])
        .map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_section(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM sections WHERE id = ?1", params![id]).map_err(|e| format!("delete: {}", e))?;
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

pub fn list_profiles(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<ConnectionProfile>, String> {
    let conn = open(hermes_home, profile)?;
    let mut stmt = conn
        .prepare("SELECT id, name, mode, host, port, username, key_path, api_url, api_key FROM connection_profiles ORDER BY name")
        .map_err(|e| format!("prepare: {}", e))?;
    let rows = stmt.query_map([], |r| {
        Ok(ConnectionProfile {
            id: r.get(0)?, name: r.get(1)?, mode: r.get(2)?, host: r.get(3)?,
            port: r.get(4)?, username: r.get(5)?, key_path: r.get(6)?,
            api_url: r.get(7)?, api_key: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
        })
    }).map_err(|e| format!("query: {}", e))?;
    Ok(rows.flatten().collect())
}

pub fn create_profile(hermes_home: &Path, profile: Option<&str>, name: &str, mode: &str, host: &str, port: i64, username: &str, key_path: &str, api_url: &str, api_key: &str) -> Result<i64, String> {
    let conn = open(hermes_home, profile)?;
    conn.execute(
        "INSERT INTO connection_profiles (name, mode, host, port, username, key_path, api_url, api_key) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![name, mode, host, port, username, key_path, api_url, api_key],
    ).map_err(|e| format!("insert: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_profile(hermes_home: &Path, profile: Option<&str>, id: i64) -> Result<(), String> {
    let conn = open(hermes_home, profile)?;
    conn.execute("DELETE FROM connection_profiles WHERE id = ?1", params![id]).map_err(|e| format!("delete: {}", e))?;
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
    if let Some(v) = name { sets.push("name = ?".into()); binds.push(Box::new(v.to_string())); }
    if let Some(v) = color { sets.push("color = ?".into()); binds.push(Box::new(v.to_string())); }
    if let Some(v) = goal_id { sets.push("goal_id = ?".into()); binds.push(Box::new(v)); }
    if sets.is_empty() { return Ok(()); }
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

pub fn list_self_checks(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<SelfCheck>, String> {
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
    pub goals_total: i64,
    pub projects_total: i64,
}

pub fn dash_stats(hermes_home: &Path, profile: Option<&str>) -> Result<DashStats, String> {
    let conn = open(hermes_home, profile)?;
    let tasks_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap_or(0);
    let tasks_done: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE status='done'", [], |r| r.get(0))
        .unwrap_or(0);
    // today = due_date is today (YYYY-MM-DD)
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let tasks_today: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE due_date = ?1 AND status != 'done'", params![today], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let goals_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals", [], |r| r.get(0))
        .unwrap_or(0);
    let projects_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
        .unwrap_or(0);
    Ok(DashStats {
        tasks_total,
        tasks_done,
        tasks_today,
        goals_total,
        projects_total,
    })
}
