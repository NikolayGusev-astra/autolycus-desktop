/**
 * ESLint rule to ban raw Hermes RPC (invoke) calls from React components.
 * 
 * Components should use the typed service layer (src/services/*.ts) instead of
 * calling invoke() directly. This enforces the anti-corruption layer pattern
 * per ADR-001 Phase 0.
 * 
 * Allowed files:
 * - src/services/*.ts (the service layer itself)
 * - src/hooks/*.ts (custom hooks that wrap services)
 * - src/stores/*.ts (Zustand stores)
 * - src-tauri/** (Rust code)
 * 
 * To use a service, import from '@/services/chat' etc.
 */

export default {
  meta: {
    type: "problem",
    docs: {
      description: "Disallow direct invoke() calls in React components - use typed services instead",
      category: "Best Practices",
      recommended: true,
    },
    fixable: null,
    schema: [
      {
        type: "object",
        properties: {
          allowedPaths: {
            type: "array",
            items: { type: "string" },
            description: "Glob patterns for files allowed to use invoke directly",
          },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      noRawInvoke: "Direct invoke() call detected in component. Use typed services from '@/services/' instead (ADR-001 Phase 0).",
    },
  },

  create(context) {
    const options = context.options[0] || {};
    const allowedPaths = options.allowedPaths || [
      "src/services/",
      "src/hooks/",
      "src/stores/",
      "src-tauri/",
    ];

    // Check if the current file is in an allowed path
    // context.getFilename() returns absolute path, so we need to get relative path from project root
    const filename = context.getFilename();
    // Get relative path from current working directory
    const cwd = process.cwd();
    let relativePath = filename;
    if (filename.startsWith(cwd)) {
      relativePath = filename.slice(cwd.length + 1); // +1 for path separator
    }
    // Normalize path separators to forward slash
    relativePath = relativePath.replace(/\\/g, "/");

    const isAllowed = allowedPaths.some(pattern => {
      // Check if the relative path starts with the allowed pattern (prefix matching)
      return relativePath.startsWith(pattern);
    });

    if (isAllowed) {
      return {};
    }

    // Track imports of invoke from @tauri-apps/api
    let hasTauriInvokeImport = false;
    let invokeImportName = "invoke";

    return {
      ImportDeclaration(node) {
        if (node.source.value === "@tauri-apps/api/core" || node.source.value === "@tauri-apps/api") {
          for (const specifier of node.specifiers) {
            if (specifier.type === "ImportSpecifier" && specifier.imported.name === "invoke") {
              hasTauriInvokeImport = true;
              invokeImportName = specifier.local.name;
            }
          }
        }
      },

      CallExpression(node) {
        if (!hasTauriInvokeImport) {
          return;
        }

        // Check if this is a call to invoke()
        if (
          node.callee.type === "Identifier" &&
          node.callee.name === invokeImportName
        ) {
          // Check if first argument is a string literal (command name)
          if (node.arguments.length > 0 && node.arguments[0].type === "Literal" && typeof node.arguments[0].value === "string") {
            const command = node.arguments[0].value;
            
            // List of known Hermes/backend commands that should go through services
            const hermesCommands = [
              // Chat
              "send_message_cmd",
              "abort_message_cmd",
              "get_session_messages_cmd",
              "list_sessions_cmd",
              "search_sessions_cmd",
              "delete_session_cmd",
              "get_session_stats_cmd",
              // Gateway
              "start_gateway_cmd",
              "stop_gateway_cmd",
              "gateway_status_cmd",
              "get_gateway_port_cmd",
              "list_models_api_cmd",
              // Session/Feed
              "list_feed_cmd",
              "list_feed_channels_cmd",
              "list_email_unread_cmd",
              "list_jira_my_active_cmd",
              "list_calendar_today_cmd",
              "list_meeting_reminders_cmd",
              "mark_email_read_cmd",
              "generate_meeting_briefing_cmd",
              "send_email_cmd",
              "jira_transition_cmd",
              "jira_comment_cmd",
              "register_steersman_mcp_cmd",
              "generate_smart_briefing_cmd",
              "get_cached_briefing_cmd",
              // Connection/SSH
              "start_ssh_tunnel_cmd",
              "stop_ssh_tunnel_cmd",
              "ssh_tunnel_status_cmd",
              "start_remote_gateway_cmd",
              "test_connection",
              "set_connection_config",
              "get_connection_config",
              "detect_local_instances_cmd",
              "detect_remote_instances_cmd",
              "connect_to_instance",
              "auto_connect_local_cmd",
              // Models/Providers
              "list_models_cmd",
              "add_model_cmd",
              "remove_model_cmd",
              "update_model_cmd",
              "get_model_config_cmd",
              "set_model_config_cmd",
              "set_default_model_cmd",
              "set_model_routing_cmd",
              "get_provider_base_url_cmd",
              "get_all_provider_urls_cmd",
              "list_providers_cmd",
              "discover_models_cmd",
              "is_discoverable_cmd",
              "get_oauth_models_cmd",
              // Profiles
              "list_profiles_cmd",
              "create_profile_cmd",
              "delete_profile_cmd",
              "set_active_profile_cmd",
              // Config
              "get_env_cmd",
              "set_env_cmd",
              // Memory
              "read_memory_cmd",
              "write_user_profile_cmd",
              "add_memory_entry_cmd",
              "update_memory_entry_cmd",
              "remove_memory_entry_cmd",
              // Media
              "save_media_blob_cmd",
              "save_media_file_cmd",
              "get_media_info_cmd",
              "read_media_data_url_cmd",
              "list_media_files_cmd",
              // Skills
              "list_installed_skills_cmd",
              "get_skill_content_cmd",
              "install_skill_cmd",
              "uninstall_skill_cmd",
              // Terminal
              "open_terminal_cmd",
              // Kanban
              "list_kanban_boards_cmd",
              "create_kanban_board_cmd",
              "delete_kanban_board_cmd",
              "list_kanban_tasks_cmd",
              "create_kanban_task_cmd",
              "update_kanban_task_cmd",
              "delete_kanban_task_cmd",
              "move_kanban_task_cmd",
              // Registry
              "fetch_registry_catalog_cmd",
              "get_installed_registry_cmd",
              "install_from_registry_cmd",
              // Cron
              "list_cron_jobs_cmd",
              "create_cron_job_cmd",
              "remove_cron_job_cmd",
              "pause_cron_job_cmd",
              "resume_cron_job_cmd",
              "trigger_cron_job_cmd",
              // Validation
              "validate_chat_readiness_cmd",
              // Config Health
              "config_health_check_cmd",
              "auto_fix_config_cmd",
              // Auth
              "auth_login_cmd",
              "auth_cancel_cmd",
              // Onboarding/Soul
              "read_soul_cmd",
              "write_soul_cmd",
              "reset_soul_cmd",
              "get_personalities_cmd",
              "get_personality_cmd",
              "set_personality_cmd",
              "set_config_yaml_value_cmd",
              "get_config_section_cmd",
              "save_provider_key_cmd",
              // Install
              "install_hermes_cmd",
              // STT
              "transcribe_audio_cmd",
              // Telegram
              "send_telegram_message_cmd",
              "validate_telegram_bot_token_cmd",
              "save_telegram_config_cmd",
              "load_telegram_config_cmd",
              // Versions
              "get_app_version",
              "get_versions_cmd",
              // MCP
              "register_steersman_mcp_cmd",
            ];

            if (hermesCommands.includes(command)) {
              context.report({
                node,
                messageId: "noRawInvoke",
                data: { command },
              });
            }
          }
        }
      },
    };
  },
};