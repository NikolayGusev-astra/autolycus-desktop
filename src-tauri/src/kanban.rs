// src-tauri/src/kanban.rs
// Kanban boards/tasks management with SQLite storage.
// Ported from fathah/hermes-desktop src/main/kanban.ts (simplified — no CLI dependency)

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    pub body: Option<String>,
    pub assignee: Option<String>,
    pub status: String,
    pub priority: i32,
    pub board_slug: String,
    pub created_by: Option<String>,
    pub created_at: Option<i64>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub result: Option<String>,
    pub skills: Vec<String>,
    pub max_retries: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanBoard {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub is_current: bool,
    pub archived: bool,
    pub total: i32,
    pub counts: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanRun {
    pub id: i64,
    pub task_id: String,
    pub profile: Option<String>,
    pub status: Option<String>,
    pub outcome: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub last_heartbeat_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanComment {
    pub id: i64,
    pub task_id: String,
    pub author: Option<String>,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanEvent {
    pub id: i64,
    pub task_id: String,
    pub kind: String,
    pub payload: Option<serde_json::Value>,
    pub created_at: i64,
    pub run_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanTaskDetail {
    pub task: KanbanTask,
    pub comments: Vec<KanbanComment>,
    pub events: Vec<KanbanEvent>,
    pub parents: Vec<String>,
    pub children: Vec<String>,
    pub runs: Vec<KanbanRun>,
    pub latest_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub key: String,
    pub label: String,
    pub tasks: Vec<KanbanTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanBoardView {
    pub board: KanbanBoard,
    pub columns: Vec<KanbanColumn>,
}

/// Default column order for kanban boards.
pub const DEFAULT_COLUMNS: &[&str] = &["backlog", "todo", "in_progress", "review", "done"];

// ── Database path ─────────────────────────────────────────────────────────

fn kanban_db_path(hermes_home: &Path, profile: Option<&str>) -> std::path::PathBuf {
    let profile_home = crate::config::profile_home(hermes_home, profile);
    profile_home.join("kanban.db")
}

// ── Board CRUD ────────────────────────────────────────────────────────────

/// List all kanban boards.
pub fn list_boards(hermes_home: &Path, profile: Option<&str>) -> Result<Vec<KanbanBoard>, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    let mut stmt = conn
        .prepare("SELECT slug, name, description, icon, color, is_current, archived FROM boards ORDER BY name")
        .map_err(|e| format!("DB prepare error: {}", e))?;

    let boards = stmt
        .query_map([], |row| {
            let slug: String = row.get(0)?;
            // Count tasks per board
            let task_count: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE board_slug = ?1",
                    [&slug],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            // Count tasks per status
            let mut counts = HashMap::new();
            let mut count_stmt = conn
                .prepare("SELECT status, COUNT(*) FROM tasks WHERE board_slug = ?1 GROUP BY status")
                .unwrap();
            let count_rows = count_stmt.query_map([&slug], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?))
            }).unwrap();
            for c in count_rows {
                if let Ok((status, count)) = c {
                    counts.insert(status, count);
                }
            }

            Ok(KanbanBoard {
                slug: slug.clone(),
                name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
                color: row.get(4)?,
                is_current: row.get::<_, i32>(5)? != 0,
                archived: row.get::<_, i32>(6)? != 0,
                total: task_count,
                counts,
            })
        })
        .map_err(|e| format!("DB query error: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("DB row error: {}", e))?;

    Ok(boards)
}

/// Create a new kanban board.
pub fn create_board(
    hermes_home: &Path,
    profile: Option<&str>,
    slug: &str,
    name: &str,
    description: Option<&str>,
) -> Result<KanbanBoard, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    init_kanban_db(&db_path)?;

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    conn.execute(
        "INSERT INTO boards (slug, name, description, is_current, archived) VALUES (?1, ?2, ?3, 0, 0)",
        rusqlite::params![slug, name, description.unwrap_or("")],
    )
    .map_err(|e| format!("DB insert error: {}", e))?;

    Ok(KanbanBoard {
        slug: slug.to_string(),
        name: name.to_string(),
        description: description.map(|s| s.to_string()),
        icon: None,
        color: None,
        is_current: false,
        archived: false,
        total: 0,
        counts: HashMap::new(),
    })
}

/// Delete a kanban board and all its tasks.
pub fn delete_board(
    hermes_home: &Path,
    profile: Option<&str>,
    slug: &str,
) -> Result<bool, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    if !db_path.exists() {
        return Ok(false);
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    conn.execute("DELETE FROM tasks WHERE board_slug = ?1", [slug])
        .map_err(|e| format!("DB delete error: {}", e))?;
    conn.execute("DELETE FROM boards WHERE slug = ?1", [slug])
        .map_err(|e| format!("DB delete error: {}", e))?;

    Ok(true)
}

// ── Task CRUD ─────────────────────────────────────────────────────────────

/// List tasks for a board, grouped by status.
pub fn list_tasks(
    hermes_home: &Path,
    profile: Option<&str>,
    board_slug: &str,
) -> Result<KanbanBoardView, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    if !db_path.exists() {
        return Err("Kanban database not found".to_string());
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    // Get board info
    let board = conn
        .query_row(
            "SELECT slug, name, description, icon, color, is_current, archived FROM boards WHERE slug = ?1",
            [board_slug],
            |row| {
                Ok(KanbanBoard {
                    slug: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    icon: row.get(3)?,
                    color: row.get(4)?,
                    is_current: row.get::<_, i32>(5)? != 0,
                    archived: row.get::<_, i32>(6)? != 0,
                    total: 0,
                    counts: HashMap::new(),
                })
            },
        )
        .map_err(|e| format!("Board not found: {}", e))?;

    // Get tasks grouped by status
    let mut columns = Vec::new();
    for &col_key in DEFAULT_COLUMNS {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, body, assignee, status, priority, board_slug, created_by,
                        created_at, started_at, completed_at, result, skills, max_retries
                 FROM tasks WHERE board_slug = ?1 AND status = ?2
                 ORDER BY priority DESC, created_at DESC",
            )
            .map_err(|e| format!("DB prepare error: {}", e))?;

        let tasks = stmt
            .query_map(rusqlite::params![board_slug, col_key], |row| {
                let skills_str: String = row.get(12).unwrap_or_default();
                let skills: Vec<String> = if skills_str.is_empty() {
                    Vec::new()
                } else {
                    skills_str.split(',').map(|s| s.to_string()).collect()
                };

                Ok(KanbanTask {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                    assignee: row.get(3)?,
                    status: row.get(4)?,
                    priority: row.get(5)?,
                    board_slug: row.get(6)?,
                    created_by: row.get(7)?,
                    created_at: row.get(8)?,
                    started_at: row.get(9)?,
                    completed_at: row.get(10)?,
                    result: row.get(11)?,
                    skills,
                    max_retries: row.get(13)?,
                })
            })
            .map_err(|e| format!("DB query error: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("DB row error: {}", e))?;

        columns.push(KanbanColumn {
            key: col_key.to_string(),
            label: col_label(col_key).to_string(),
            tasks,
        });
    }

    Ok(KanbanBoardView { board, columns })
}

/// Create a new task.
pub fn create_task(
    hermes_home: &Path,
    profile: Option<&str>,
    board_slug: &str,
    title: &str,
    body: Option<&str>,
    status: &str,
) -> Result<KanbanTask, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    init_kanban_db(&db_path)?;

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO tasks (id, title, body, status, priority, board_slug, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
        rusqlite::params![task_id, title, body.unwrap_or(""), status, board_slug, now],
    )
    .map_err(|e| format!("DB insert error: {}", e))?;

    Ok(KanbanTask {
        id: task_id,
        title: title.to_string(),
        body: body.map(|s| s.to_string()),
        assignee: None,
        status: status.to_string(),
        priority: 0,
        board_slug: board_slug.to_string(),
        created_by: None,
        created_at: Some(now),
        started_at: None,
        completed_at: None,
        result: None,
        skills: Vec::new(),
        max_retries: None,
    })
}

/// Update a task.
pub fn update_task(
    hermes_home: &Path,
    profile: Option<&str>,
    task_id: &str,
    fields: &HashMap<String, String>,
) -> Result<bool, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    if !db_path.exists() {
        return Ok(false);
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    // Build dynamic UPDATE query
    let mut set_clauses = Vec::new();
    let mut params: Vec<String> = Vec::new();

    if let Some(title) = fields.get("title") {
        set_clauses.push("title = ?".to_string());
        params.push(title.clone());
    }
    if let Some(body) = fields.get("body") {
        set_clauses.push("body = ?".to_string());
        params.push(body.clone());
    }
    if let Some(status) = fields.get("status") {
        set_clauses.push("status = ?".to_string());
        params.push(status.clone());
    }
    if let Some(priority) = fields.get("priority") {
        set_clauses.push("priority = ?".to_string());
        params.push(priority.clone());
    }
    if let Some(assignee) = fields.get("assignee") {
        set_clauses.push("assignee = ?".to_string());
        params.push(assignee.clone());
    }

    if set_clauses.is_empty() {
        return Ok(false);
    }

    let sql = format!(
        "UPDATE tasks SET {} WHERE id = ?",
        set_clauses.join(", ")
    );
    params.push(task_id.to_string());

    let param_refs: Vec<&dyn rusqlite::ToSql> = params
        .iter()
        .map(|p| p as &dyn rusqlite::ToSql)
        .collect();

    let updated = conn
        .execute(&sql, &param_refs[..])
        .map_err(|e| format!("DB update error: {}", e))?;

    Ok(updated > 0)
}

/// Delete a task.
pub fn delete_task(
    hermes_home: &Path,
    profile: Option<&str>,
    task_id: &str,
) -> Result<bool, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    if !db_path.exists() {
        return Ok(false);
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    let deleted = conn
        .execute("DELETE FROM tasks WHERE id = ?1", [task_id])
        .map_err(|e| format!("DB delete error: {}", e))?;

    Ok(deleted > 0)
}

/// Move a task to a different status.
pub fn move_task(
    hermes_home: &Path,
    profile: Option<&str>,
    task_id: &str,
    new_status: &str,
) -> Result<bool, String> {
    let db_path = kanban_db_path(hermes_home, profile);
    if !db_path.exists() {
        return Ok(false);
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    let now = chrono::Utc::now().timestamp();
    let (started_at, completed_at) = match new_status {
        "in_progress" => (Some(now), None),
        "done" => (None, Some(now)),
        _ => (None, None),
    };

    let sql = if started_at.is_some() {
        "UPDATE tasks SET status = ?1, started_at = ?2 WHERE id = ?3"
    } else if completed_at.is_some() {
        "UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3"
    } else {
        "UPDATE tasks SET status = ?1 WHERE id = ?2"
    };

    let updated = if started_at.is_some() {
        conn.execute(sql, rusqlite::params![new_status, started_at.unwrap(), task_id])
    } else if completed_at.is_some() {
        conn.execute(sql, rusqlite::params![new_status, completed_at.unwrap(), task_id])
    } else {
        conn.execute(sql, rusqlite::params![new_status, task_id])
    }
    .map_err(|e| format!("DB update error: {}", e))?;

    Ok(updated > 0)
}

// ── Internal ──────────────────────────────────────────────────────────────

fn col_label(key: &str) -> &str {
    match key {
        "backlog" => "Backlog",
        "todo" => "To Do",
        "in_progress" => "In Progress",
        "review" => "Review",
        "done" => "Done",
        _ => key,
    }
}

/// Initialize the kanban database schema.
fn init_kanban_db(db_path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("DB open error: {}", e))?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS boards (
            slug TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT DEFAULT '',
            icon TEXT DEFAULT NULL,
            color TEXT DEFAULT NULL,
            is_current INTEGER DEFAULT 0,
            archived INTEGER DEFAULT 0,
            created_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            body TEXT DEFAULT '',
            assignee TEXT DEFAULT NULL,
            status TEXT DEFAULT 'backlog',
            priority INTEGER DEFAULT 0,
            board_slug TEXT NOT NULL,
            created_by TEXT DEFAULT NULL,
            created_at INTEGER,
            started_at INTEGER DEFAULT NULL,
            completed_at INTEGER DEFAULT NULL,
            result TEXT DEFAULT NULL,
            skills TEXT DEFAULT '',
            max_retries INTEGER DEFAULT NULL,
            FOREIGN KEY (board_slug) REFERENCES boards(slug)
        );

        CREATE INDEX IF NOT EXISTS idx_tasks_board ON tasks(board_slug);
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
        ",
    )
    .map_err(|e| format!("DB init error: {}", e))?;

    Ok(())
}
