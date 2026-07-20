// src/components/chat/ChatView.tsx
// v0.6.0: pipeline status, approval flow, tool events, context window, gateway status

import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SquarePen, PanelRight } from "lucide-react";
import { MessageList } from "./MessageList";
import { ChatInput } from "./ChatInput";
import { useGatewayStore } from "../../stores/gatewayStore";
import type { PipelineStatus, ApprovalRequest } from "../../lib/types";
import { useTranslation } from "../../hooks/useTranslation";

export function ChatView({ historyOpen, onToggleHistory }: { historyOpen?: boolean; onToggleHistory?: () => void }) {
  const {
    currentSessionId,
    setCurrentSession,
    addMessage,
    updateMessage,
    appendToken,
    agentStatus,
    setAgentStatus,
    setPipelineStatus,
    setPendingApproval,
    } = useGatewayStore();

  const { t } = useTranslation();

  // Track currently running tool for UI
  const runningToolRef = useRef<{ name: string; msgId: string } | null>(null);
  // Stable streaming-message ID: all token chunks append to THIS message,
  // eliminating the "last message" race that lost/corrupted tokens.
  const streamingMsgIdRef = useRef<string | null>(null);

  // Fetch gateway status on mount (when connected).
  // gateway_status_cmd returns a bool (running?), not a struct. Pipeline
  // metrics (model/tokens/cost) arrive via the "pipeline_status" event instead.
  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const running = await invoke<boolean>("gateway_status_cmd");
        if (running) {
          setPipelineStatus({ backend: "connected" });
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

  // BUG-2 fix: when currentSessionId changes (e.g. user opened a session from
  // the Feed), reload its history from the backend as the source of truth.
  // This guards against store-timing races where messages set by the Feed's
  // onOpenSession get clobbered by a subsequent reset or mount order.
  const loadedSessionRef = useRef<string | null>(null);
  useEffect(() => {
    if (!currentSessionId) {
      loadedSessionRef.current = null;
      return;
    }
    if (loadedSessionRef.current === currentSessionId) return;
    loadedSessionRef.current = currentSessionId;
    let cancelled = false;
    (async () => {
      try {
        const msgs = await invoke<
          Array<{ id: number; role: string; content: string; timestamp: number }>
        >("get_session_messages_cmd", { sessionId: currentSessionId, profile: null });
        if (cancelled) return;
        const mapped = msgs
          .filter((m) => m.role === "user" || m.role === "assistant")
          .map((m) => ({
            id: `hist-${m.id}`,
            role: m.role as "user" | "assistant",
            content: m.content,
            timestamp: m.timestamp,
          }));
        // Only replace if still on the same session (avoid clobbering a stream).
        if (loadedSessionRef.current === currentSessionId) {
          useGatewayStore.setState({ messages: mapped });
        }
      } catch (e) {
        console.error("chat session load", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentSessionId]);

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
        case "token": {
          if (payload.content) {
            // Use a stable streaming-message ID (kept in a ref) so all token
            // chunks append to ONE message, regardless of what other events
            // (reasoning/tool) insert in between. Previously "last message" was
            // used, which broke when reasoning/tool messages displaced it.
            let sid = streamingMsgIdRef.current;
            if (!sid) {
              sid = crypto.randomUUID();
              streamingMsgIdRef.current = sid;
              addMessage({
                id: sid,
                role: "assistant",
                content: payload.content,
                timestamp: Date.now(),
                isStreaming: true,
              });
            } else {
              appendToken(sid, payload.content);
            }
          }
          break;
        }

        case "reasoning": {
          // Reasoning is stored as a separate field on the streaming message,
          // NOT a separate assistant message (which used to shadow the token
          // target). If there's no streaming message yet, create one.
          if (payload.content) {
            setAgentStatus("thinking");
            let sid = streamingMsgIdRef.current;
            if (!sid) {
              sid = crypto.randomUUID();
              streamingMsgIdRef.current = sid;
              addMessage({
                id: sid,
                role: "assistant",
                content: "",
                timestamp: Date.now(),
                isStreaming: true,
                thinking: payload.content,
              });
            } else {
              updateMessage(sid, {
                thinking: (useGatewayStore.getState().messages.find((m) => m.id === sid)?.thinking || "") + payload.content,
              });
            }
          }
          break;
        }

        case "tool_start": {
          setAgentStatus("tool_calling");
          const toolName = payload.name || payload.tool_name || "tool";
          const msgId = crypto.randomUUID();
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

          if (runningToolRef.current) {
            const durationText =
              durationMs !== undefined
                ? ` (${durationMs < 1000 ? `${durationMs}ms` : `${(durationMs / 1000).toFixed(1)}s`})`
                : "";
            let resultContent = `✅ **\`${toolName}\` ${t("chat.tool_completed")}**${durationText}`;
            if (output) {
              const truncated =
                output.length > 500 ? output.slice(0, 500) + "\n\n" + t("chat.output_truncated") : output;
              resultContent += `\n\`\`\`\n${truncated}\n\`\`\``;
            }
            updateMessage(runningToolRef.current.msgId, {
              content: resultContent,
              isStreaming: false,
            });
          }
          runningToolRef.current = null;
          // Reset streaming target so the next token creates a fresh message
          // (the assistant's actual reply after tool execution).
          streamingMsgIdRef.current = null;
          break;
        }

        case "done": {
          // Finalize ALL streaming messages (not just "last") — tool messages
          // or reasoning may still be flagged isStreaming.
          const msgs = useGatewayStore.getState().messages;
          useGatewayStore.setState({
            messages: msgs.map((m) => (m.isStreaming ? { ...m, isStreaming: false } : m)),
          });
          setAgentStatus("idle");
          streamingMsgIdRef.current = null;
          runningToolRef.current = null;
          // Capture the session id the backend assigned to this turn, so
          // subsequent sends continue in the same session and HistoryPanel
          // reflects the real conversation.
          if (payload.session_id) {
            setCurrentSession(payload.session_id);
          }
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

      // Reset the streaming target for the new turn.
      streamingMsgIdRef.current = null;

      // Add user message to UI immediately (with attachment previews).
      const userMsg = {
        id: crypto.randomUUID(),
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
      // Snapshot history BEFORE adding the user message, so the agent doesn't
      // see the current turn duplicated (text + history entry).
      const prior = useGatewayStore.getState().messages;
      const HISTORY_TURNS = 10;
      const history = prior
        .filter((m) => !m.isStreaming && (m.role === "user" || m.role === "assistant"))
        .slice(-HISTORY_TURNS)
        .map((m) => ({ role: m.role, content: m.content }));

      addMessage(userMsg);

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

  // Stop the current generation: tell the backend to emit chat-abort, then
  // reset the UI locally so the user sees an immediate stop. The streaming
  // message is finalized (isStreaming=false) with whatever tokens arrived.
  const handleStop = useCallback(() => {
    invoke("abort_message_cmd").catch((err) =>
      console.error("Failed to abort:", err)
    );
    const sid = streamingMsgIdRef.current;
    if (sid) {
      updateMessage(sid, { isStreaming: false });
    }
    streamingMsgIdRef.current = null;
    runningToolRef.current = null;
    setAgentStatus("idle");
  }, [updateMessage, setAgentStatus]);

  // Listen for backend-initiated abort (e.g. gateway disconnect).
  useEffect(() => {
    const unlisten = listen("chat-abort", () => {
      const sid = streamingMsgIdRef.current;
      if (sid) {
        updateMessage(sid, { isStreaming: false });
      }
      streamingMsgIdRef.current = null;
      setAgentStatus("idle");
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [updateMessage, setAgentStatus]);

  const handleNewSession = useCallback(() => {
    useGatewayStore.setState({ messages: [], currentSessionId: null });
    setAgentStatus("idle");
    streamingMsgIdRef.current = null;
    runningToolRef.current = null;
  }, [setAgentStatus]);

  return (
    <div className="flex h-full flex-col bg-ac-bg">
      {/* Chat toolbar: new-session button */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-ac-border">
        <span className="text-xs text-ac-muted">{t("nav.chat")}</span>
        <button
          onClick={handleNewSession}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs text-ac-muted hover:text-ac-brand rounded-md hover:bg-ac-surface transition-colors"
          title={t("chat.newSession")}
        >
          <SquarePen className="w-3.5 h-3.5" />
          {t("chat.newSession")}
        </button>
        {onToggleHistory && (
          <button
            onClick={onToggleHistory}
            className={`flex items-center gap-1.5 px-2.5 py-1.5 text-xs rounded-md transition-colors ${historyOpen ? "text-ac-brand bg-ac-brand-soft" : "text-ac-muted hover:bg-ac-surface"}`}
            title={t("nav.sessions")}
          >
            <PanelRight className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
      <div className="flex-1 overflow-hidden">
        <MessageList />
      </div>
      <ChatInput
        onSend={handleSend}
        onStop={handleStop}
        agentStatus={agentStatus}
      />
    </div>
  );
}