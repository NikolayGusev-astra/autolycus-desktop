// src-tauri/src/bin/steersman-mcp-server.rs
// ADR-008: Steersman MCP Server binary.
//
// Launched by the Hermes backend as a subprocess (registered in config.yaml
// under mcp_servers.steersman). Speaks newline-delimited JSON-RPC over stdio,
// exposing Steersman's productivity DB and session search as MCP tools.
//
// The agent can then call steersman_create_task, steersman_list_tasks, etc.
// directly from chat — making it a real executive assistant with write-back.

fn main() {
    steersman_desktop_lib::mcp_server::run_loop();
}
