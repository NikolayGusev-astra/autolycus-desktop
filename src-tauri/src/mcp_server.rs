// src-tauri/src/mcp_server.rs
// ADR-008: Steersman MCP Server — exposes Steersman's productivity DB and
// session search to the Hermes agent as MCP tools. The agent can then
// create/update tasks, list goals, and search past sessions directly from
// chat — making it a real "executive assistant" with a write-back channel.
//
// This module is the server-side logic. The binary entry point is
// mcp_server_main.rs (a separate [[bin]] target). Both import this module.
//
// Wire protocol: newline-delimited JSON-RPC 2.0 over stdio (same framing
// the email/jira/calendar MCP servers use — verified at mcp_client.rs).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

/// Resolve HERMES_HOME from env (set by the Hermes backend when launching us).
fn hermes_home() -> Result<PathBuf, String> {
    if let Ok(val) = std::env::var("HERMES_HOME") {
        let p = PathBuf::from(&val);
        if p.is_dir() {
            return Ok(p);
        }
    }
    // Fallback: ~/.hermes
    dirs::home_dir()
        .map(|h| h.join(".hermes"))
        .ok_or_else(|| "HERMES_HOME not set and ~/.hermes not found".to_string())
}

/// Tool descriptor sent in tools/list.
struct ToolDef {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

/// All tools Steersman exposes to the agent.
fn tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "steersman_list_tasks",
            description: "List tasks from the Steersman productivity database. Returns active tasks (not done) by default. Use status='all' to include completed tasks.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "status": {"type": "string", "description": "Filter: 'active' (default), 'done', or 'all'"},
                    "limit": {"type": "integer", "description": "Max tasks to return (default 20)"}
                }
            }),
        },
        ToolDef {
            name: "steersman_create_task",
            description: "Create a new task in the Steersman productivity database.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Task title"},
                    "priority": {"type": "integer", "description": "Priority 1-5 (default 3)"},
                    "due_date": {"type": "string", "description": "Due date ISO 8601 (optional)"},
                    "assignee": {"type": "string", "description": "Assignee (optional)"}
                },
                "required": ["title"]
            }),
        },
        ToolDef {
            name: "steersman_update_task_status",
            description: "Update a task's status (e.g. mark done, in_progress).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Task ID"},
                    "status": {"type": "string", "description": "New status: todo, in_progress, done"}
                },
                "required": ["id", "status"]
            }),
        },
        ToolDef {
            name: "steersman_list_goals",
            description: "List goals (epics) from the Steersman productivity database.",
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "steersman_create_goal",
            description: "Create a new goal (epic).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Goal title"},
                    "target_date": {"type": "string", "description": "Target date ISO 8601 (optional)"}
                },
                "required": ["title"]
            }),
        },
        ToolDef {
            name: "steersman_search_sessions",
            description: "Search past chat sessions in state.db by text query.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "limit": {"type": "integer", "description": "Max results (default 10)"}
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "steersman_link_session",
            description: "Link a chat session to a task, project, or goal (ADR-009). The agent uses this to connect a conversation to the work it produced.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": {"type": "string", "description": "Chat session ID (from state.db or search_sessions)"},
                    "task_id": {"type": "integer", "description": "Task ID to link to (optional)"},
                    "project_id": {"type": "integer", "description": "Project ID to link to (optional)"},
                    "goal_id": {"type": "integer", "description": "Goal ID to link to (optional)"},
                    "note": {"type": "string", "description": "Optional note about why this was linked"}
                },
                "required": ["session_id"]
            }),
        },
        ToolDef {
            name: "steersman_get_meeting_context",
            description: "L8: gather context for a calendar meeting — related tasks (by title/attendee keyword match) and recent chat session previews. Does NOT call the LLM; returns the raw context for the agent to build a briefing.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "event_uid": {"type": "string", "description": "Calendar event UID (from list_calendar_today / list_meeting_reminders)"},
                    "keyword": {"type": "string", "description": "Optional keyword to match task titles (e.g. customer name from organizer). Defaults to organizer handle."}
                },
                "required": ["event_uid"]
            }),
        },
    ]
}

/// Dispatch a tools/call request to the right handler.
fn dispatch_tool(name: &str, args: &Value) -> Result<Value, String> {
    let home = hermes_home()?;
    match name {
        "steersman_list_tasks" => {
            let status = args
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("active");
            let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
            let mut tasks = crate::productivity::list_tasks(&home, None)
                .map_err(|e| format!("list_tasks: {}", e))?;
            if status != "all" {
                tasks.retain(|t| {
                    if status == "done" {
                        t.status == "done" || t.status == "completed"
                    } else {
                        t.status != "done" && t.status != "completed"
                    }
                });
            }
            tasks.truncate(limit as usize);
            Ok(json!(tasks))
        }
        "steersman_create_task" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or("title required")?;
            let priority = args.get("priority").and_then(|v| v.as_i64()).unwrap_or(3);
            let due = args.get("due_date").and_then(|v| v.as_str());
            let assignee = args.get("assignee").and_then(|v| v.as_str()).unwrap_or("");
            let id = crate::productivity::create_task(
                &home, None, title, priority, due, None, assignee, None,
            )
            .map_err(|e| format!("create_task: {}", e))?;
            Ok(json!({"id": id, "title": title, "status": "created"}))
        }
        "steersman_update_task_status" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or("id required")?;
            let status = args
                .get("status")
                .and_then(|v| v.as_str())
                .ok_or("status required")?;
            crate::productivity::update_task_status(&home, None, id, status)
                .map_err(|e| format!("update_task_status: {}", e))?;
            Ok(json!({"id": id, "status": status, "updated": true}))
        }
        "steersman_list_goals" => {
            let goals = crate::productivity::list_goals(&home, None)
                .map_err(|e| format!("list_goals: {}", e))?;
            Ok(json!(goals))
        }
        "steersman_create_goal" => {
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or("title required")?;
            let target_date = args.get("target_date").and_then(|v| v.as_str());
            let id = crate::productivity::create_goal(&home, None, title, target_date)
                .map_err(|e| format!("create_goal: {}", e))?;
            Ok(json!({"id": id, "title": title, "status": "created"}))
        }
        "steersman_search_sessions" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("query required")?;
            let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
            let results = crate::sessions::search_sessions(&home, None, query, limit)
                .map_err(|e| format!("search_sessions: {}", e))?;
            Ok(json!(results))
        }
        "steersman_link_session" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or("session_id required")?;
            let task_id = args.get("task_id").and_then(|v| v.as_i64());
            let project_id = args.get("project_id").and_then(|v| v.as_i64());
            let goal_id = args.get("goal_id").and_then(|v| v.as_i64());
            let note = args.get("note").and_then(|v| v.as_str());
            if task_id.is_none() && project_id.is_none() && goal_id.is_none() {
                return Err("at least one of task_id/project_id/goal_id required".to_string());
            }
            let link_id = crate::productivity::link_session(
                &home,
                None,
                session_id,
                task_id,
                project_id,
                goal_id,
                Some("agent"),
                note,
            )
            .map_err(|e| format!("link_session: {}", e))?;
            Ok(json!({"id": link_id, "session_id": session_id, "linked": true}))
        }
        "steersman_get_meeting_context" => {
            let event_uid = args
                .get("event_uid")
                .and_then(|v| v.as_str())
                .ok_or("event_uid required")?;
            // Fetch the event from the calendar MCP to get its summary/organizer.
            // run_loop is synchronous (blocking stdin read), so drive the async
            // calendar call on a one-off tokio runtime.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("runtime: {}", e))?;
            let events = rt
                .block_on(crate::feed_sources::list_calendar_today(&home, None))
                .map_err(|e| format!("calendar: {}", e))?;
            let event = events
                .iter()
                .find(|e| e.uid == event_uid)
                .ok_or("meeting not found in calendar")?;
            let keyword = args
                .get("keyword")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    event
                        .organizer
                        .split('@')
                        .next()
                        .map(|s: &str| s.to_string())
                })
                .unwrap_or_default();
            // Related tasks by title keyword match.
            let all_tasks = crate::productivity::list_tasks(&home, None).unwrap_or_default();
            let related_tasks: Vec<Value> = all_tasks
                .iter()
                .filter(|t| {
                    let s = t.title.to_lowercase();
                    s.contains(&event.summary.to_lowercase())
                        || (!keyword.is_empty() && s.contains(&keyword.to_lowercase()))
                })
                .map(|t| {
                    json!({
                        "id": t.id,
                        "title": t.title,
                        "status": t.status,
                        "priority": t.priority,
                    })
                })
                .collect();
            // Recent chat session previews for context.
            let sessions =
                crate::sessions::recent_session_previews(&home, None, 20).unwrap_or_default();
            Ok(json!({
                "event_uid": event_uid,
                "summary": event.summary,
                "organizer": event.organizer,
                "attendees": event.attendees,
                "related_tasks": related_tasks,
                "recent_sessions": sessions,
            }))
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}

/// Handle a single JSON-RPC request, returning the response Value.
pub fn handle_request(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "steersman-mcp", "version": env!("CARGO_PKG_VERSION")}
            }
        })),
        "tools/list" => {
            let tool_list: Vec<Value> = tools()
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"tools": tool_list}
            }))
        }
        "tools/call" => {
            let name = req
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = req
                .get("params")
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(json!({}));
            match dispatch_tool(name, &args) {
                Ok(result) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{"type": "text", "text": result.to_string()}]
                    }
                })),
                Err(e) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{"type": "text", "text": format!("ERROR: {}", e)}],
                        "isError": true
                    }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("method not found: {}", method)}
        })),
    }
}

/// Main read-dispatch loop. Reads newline-delimited JSON-RPC from stdin,
/// dispatches each request, writes responses to stdout. Runs until EOF.
pub fn run_loop() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(resp) = handle_request(&req) {
            let _ = writeln!(stdout, "{}", resp);
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn handle_initialize_returns_valid_response() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
        let resp = handle_request(&req).unwrap();
        assert_eq!(resp.get("id"), Some(&json!(1)));
        assert_eq!(
            resp.get("result")
                .and_then(|r| r.get("protocolVersion"))
                .and_then(|v| v.as_str()),
            Some("2024-11-05")
        );
        assert_eq!(resp["result"]["serverInfo"]["name"], "steersman-mcp");
    }

    #[test]
    fn tools_list_returns_all_steersman_tools() {
        let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        let resp = handle_request(&req).unwrap();
        let tools = resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .unwrap();
        let names: Vec<&str> = tools
            .iter()
            .map(|t| t.get("name").and_then(|n| n.as_str()).unwrap())
            .collect();
        assert!(names.contains(&"steersman_list_tasks"));
        assert!(names.contains(&"steersman_create_task"));
        assert!(names.contains(&"steersman_update_task_status"));
        assert!(names.contains(&"steersman_search_sessions"));
        // Each tool must have inputSchema.
        for t in tools {
            assert!(t.get("inputSchema").is_some(), "tool missing inputSchema");
            assert!(t.get("description").is_some(), "tool missing description");
        }
    }

    #[test]
    fn unknown_tool_returns_iserror() {
        // dispatch_tool with a bad name returns an error string.
        let result = dispatch_tool("nonexistent_tool", &json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tool"));
    }

    #[test]
    fn tools_call_unknown_returns_iserror_response() {
        let req = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"bad","arguments":{}}});
        let resp = handle_request(&req).unwrap();
        assert_eq!(resp["result"]["isError"], json!(true));
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("ERROR"));
    }

    #[test]
    fn unknown_method_returns_error_code() {
        let req = json!({"jsonrpc":"2.0","id":4,"method":"resources/list","params":{}});
        let resp = handle_request(&req).unwrap();
        assert_eq!(resp["error"]["code"], json!(-32601));
    }

    #[test]
    fn create_task_missing_title_returns_error() {
        let req = json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"steersman_create_task","arguments":{}}});
        let resp = handle_request(&req).unwrap();
        assert_eq!(resp["result"]["isError"], json!(true));
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("title"));
    }

    #[test]
    fn link_session_requires_a_target() {
        // Missing all of task_id/project_id/goal_id → error.
        let req = json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"steersman_link_session","arguments":{"session_id":"sess-xyz"}}});
        let resp = handle_request(&req).unwrap();
        assert_eq!(resp["result"]["isError"], json!(true));
    }

    #[test]
    fn tools_list_includes_link_session() {
        let req = json!({"jsonrpc":"2.0","id":7,"method":"tools/list","params":{}});
        let resp = handle_request(&req).unwrap();
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"steersman_link_session"));
    }
}
