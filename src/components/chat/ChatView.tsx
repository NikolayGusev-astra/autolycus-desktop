// src/components/chat/ChatView.tsx
// v0.6.0: pipeline status, approval flow, tool events, context window, gateway status

import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MessageList } from "./MessageList";
import { ChatInput } from "./ChatInput";
import { useGatewayStore } from "../../stores/gatewayStore";
import type { PipelineStatus, ApprovalRequest } from "../../lib/types";
import { useTranslation } from "../../hooks/useTranslation";

export function ChatView() {
  const {
    currentSessionId,
    addMessage,
    agentStatus,
    setAgentStatus,
    setPipelineStatus,
    setPendingApproval,
    } = useGatewayStore();

  const { t } = useTranslation();

  // Track currently running tool for UI
  const runningToolRef = useRef<{ name: string; msgId: string } | null>(null);

  // Fetch gateway status on mount (when connected)
  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const status = await invoke<{
          model?: string;
          tokens_used?: number;
          tokens_limit?: number;
          cost_usd?: number;
        }>("gateway_status_cmd");
        if (status) {
          setPipelineStatus({
            backend: "connected",
            model: status.model,
            tokensUsed: status.tokens_used,
            tokensLimit: status.tokens_limit,
            costUsd: status.cost_usd,
          });
        }
      } catch {
        // gateway_status_cmd may not exist yet — silently ignore
      }
    };

    // Fetch on mount and when connected state changes
    const connected = useGatewayStore.getState().connected;
    if (connected) {
      fetchStatus();
    }

    const unsub = useGatewayStore.subscribe((s, prev) => {
      if (s.connected && !prev.connected) {
        fetchStatus();
      }
    });
    return () => unsub();
  }, [setPipelineStatus]);

  // Listen for chat events from Rust backend
  useEffect(() => {
    const unlisten = listen<{
      type: string;
      content?: string;
      message?: string;
      name?: string;
      tool_call_id?: string;
      output?: string;
      duration_ms?: number;
      backend?: string;
      model?: string;
      tokens_used?: number;
      tokens_limit?: number;
      cost_usd?: number;
      session_id?: string;
      status?: string;
      // Approval fields
      request_id?: string;
      tool_name?: string;
      tool_input?: string;
      input?: string;
      action?: string;
      command_class?: string;
    }>("chat_event", (event) => {
      const payload = event.payload;
      const eventType = payload.type;

      switch (eventType) {
        case "token":
          if (payload.content) {
            const messages = useGatewayStore.getState().messages;
            const lastIdx = messages.length - 1;
            const lastMsg = messages[lastIdx];
            if (lastMsg && lastMsg.isStreaming && lastMsg.role === "assistant") {
              const updated = [...messages];
              updated[lastIdx] = { ...lastMsg, content: lastMsg.content + payload.content };
              useGatewayStore.setState({ messages: updated });
            } else {
              addMessage({
                id: `msg-${Date.now()}`,
                role: "assistant",
                content: payload.content,
                timestamp: Date.now(),
                isStreaming: true,
              });
            }
          }
          break;

        case "reasoning":
          if (payload.content) {
            setAgentStatus("thinking");
            addMessage({
              id: `thinking-${Date.now()}`,
              role: "assistant",
              content: payload.content,
              timestamp: Date.now(),
              isStreaming: true,
            });
          }
          break;

        case "tool_start": {
          setAgentStatus("tool_calling");
          const toolName = payload.name || payload.tool_name || "tool";
          // Show inline "🔧 **Running:** <tool>..." as a streaming assistant message
          const msgId = `tool-start-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
          addMessage({
            id: msgId,
            role: "assistant",
            content: `🔧 **${t("chat.tool_running")} ${toolName}...`,
            timestamp: Date.now(),
            isStreaming: true,
          });
          runningToolRef.current = { name: toolName, msgId };
          break;
        }

        case "tool_complete": {
          setAgentStatus("idle");
          const toolName = payload.name || runningToolRef.current?.name || "tool";
          const output = payload.output || payload.content || "";
          const durationMs = payload.duration_ms;

          // Update the running tool message
          if (runningToolRef.current) {
            const messages = useGatewayStore.getState().messages;
            const idx = messages.findIndex((m) => m.id === runningToolRef.current!.msgId);
            if (idx >= 0) {
              const updated = [...messages];
              const durationText =
                durationMs !== undefined
                  ? ` (${durationMs < 1000 ? `${durationMs}ms` : `${(durationMs / 1000).toFixed(1)}s`})`
                  : "";
              let resultContent = `✅ **\`${toolName}\` ${t("chat.tool_completed")}**${durationText}`;
              if (output) {
                // Truncate long output for inline display
                const truncated =
                  output.length > 500 ? output.slice(0, 500) + "\n\n" + t("chat.output_truncated") : output;
                resultContent += `\n\`\`\`\n${truncated}\n\`\`\``;
              }
              updated[idx] = {
                ...updated[idx],
                content: resultContent,
                isStreaming: false,
              };
              useGatewayStore.setState({ messages: updated });
            }
          }
          runningToolRef.current = null;
          break;
        }

        case "done": {
          const msgs = useGatewayStore.getState().messages;
          const lastIdx = msgs.length - 1;
          const lastMsg = msgs[lastIdx];
          if (lastMsg && lastMsg.isStreaming) {
            const updated = [...msgs];
            updated[lastIdx] = { ...lastMsg, isStreaming: false };
            useGatewayStore.setState({ messages: updated });
          }
          setAgentStatus("idle");
          runningToolRef.current = null;
          break;
        }

        case "error":
          setAgentStatus("error");
          break;

        case "status":
          if (payload.status) {
            setAgentStatus(payload.status as any);
          }
          break;

        case "pipeline_status": {
          const ps: PipelineStatus = {
            backend: payload.backend === "connected" ? "connected" : "disconnected",
            model: payload.model,
            tokensUsed: payload.tokens_used,
            tokensLimit: payload.tokens_limit,
            costUsd: payload.cost_usd,
          };
          setPipelineStatus(ps);
          break;
        }

        case "approval_request": {
          const approval: ApprovalRequest = {
            requestId: payload.request_id || payload.tool_call_id || `req-${Date.now()}`,
            toolName: payload.tool_name || payload.name || "tool",
            toolInput: payload.tool_input || payload.input || "",
            action: payload.action || payload.message || payload.content || "",
            commandClass: (payload.command_class as any) || "write",
          };
          setPendingApproval(approval);
          break;
        }

        default:
          break;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [addMessage, setAgentStatus, setPipelineStatus, setPendingApproval]);

  const handleSend = useCallback(
    async (text: string, attachments?: import("./ChatInput").Attachment[]) => {
      if (!text.trim() && !(attachments && attachments.length)) return;

      // Resolve attachments to references the agent can act on:
      //  - File attachments (images/docs/etc.) are saved to the media cache and
      //    turned into a path the agent can read.
      //  - Voice clips already have a path (saved by VoiceInput on stop).
      //  - URL chips are passed through as-is in the message text.
      let messageText = text.trim();
      const savedPaths: string[] = [];
      if (attachments && attachments.length) {
        for (const att of attachments) {
          if (att.path) {
            savedPaths.push(att.path);
          } else if (att.file) {
            try {
              const buf = new Uint8Array(await att.file.arrayBuffer());
              const ext = att.file.name.split(".").pop() || "bin";
              const path = await invoke<string>("save_media_blob_cmd", {
                data: Array.from(buf),
                ext,
              });
              savedPaths.push(path);
            } catch (e) {
              console.error("Failed to save attachment:", e);
            }
          } else if (att.name && !att.path && !att.file) {
            // URL chip — already in text; nothing else to do.
          }
        }
      }
      if (savedPaths.length) {
        // Make the paths visible to the agent: images can be inlined as data
        // URLs by the backend; for audio/video we hand the path so the agent
        // transcribes via its STT/Whisper tool.
        messageText = messageText
          ? `${messageText}\n\n[вложения: ${savedPaths.join(", ")}]`
          : `[вложения: ${savedPaths.join(", ")}]`;
      }

      // Add user message to UI immediately (with attachment previews).
      const userMsg = {
        id: `user-${Date.now()}`,
        role: "user" as const,
        content: messageText,
        timestamp: Date.now(),
        attachments: (attachments ?? []).map((a) => ({
          kind: a.kind === "file" && a.mime?.startsWith("image/") ? "image" as const
            : a.kind === "file" && a.mime?.startsWith("video/") ? "video" as const
            : a.kind,
          path: a.path,
          name: a.name,
          mime: a.mime,
        })),
      };
      addMessage(userMsg);

      // Build conversation history from prior messages so the agent has
      // context (the previous implementation sent history:null, so every turn
      // was amnesic). Keep the last few real turns (skip transient streaming
      // / tool-status markers that carry no user content).
      const prior = useGatewayStore.getState().messages;
      const HISTORY_TURNS = 10;
      const history = prior
        .filter((m) => !m.isStreaming && (m.role === "user" || m.role === "assistant"))
        .slice(-HISTORY_TURNS)
        .map((m) => ({ role: m.role, content: m.content }));

      try {
        // Fire the message and DON'T await the full response — streaming
        // arrives via chat_event events, which populate messages incrementally.
        // Awaiting here blocked the input clear (and the UI) until the agent
        // fully finished responding.
        invoke<string>("send_message_cmd", {
          request: {
            text: messageText,
            session_id: currentSessionId,
            history,
          },
        }).catch((err) => {
          console.error("Failed to send message:", err);
          setAgentStatus("error");
        });
      } catch (err) {
        console.error("Failed to send message:", err);
        setAgentStatus("error");
      }
    },
    [currentSessionId, addMessage, setAgentStatus]
  );

  return (
    <div className="flex h-full flex-col bg-ac-bg">
      <div className="flex-1 overflow-hidden">
        <MessageList />
      </div>
      <ChatInput onSend={handleSend} agentStatus={agentStatus} />
    </div>
  );
}