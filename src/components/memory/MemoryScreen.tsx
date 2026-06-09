// src/components/memory/MemoryScreen.tsx
// v0.5.0: Memory screen — read/write memory.md, user.md, stats via Rust

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Brain,
  Plus,
  Trash2,
  Edit3,
  Check,
  X,
  Save,
  RefreshCw,
  Loader,
  User,
  FileText,
  BarChart3,
} from "lucide-react";

interface MemoryInfo {
  content: string;
  exists: boolean;
  last_modified: number | null;
}

interface UserProfile {
  content: string;
  exists: boolean;
  last_modified: number | null;
}

interface MemoryStats {
  total_sessions: number;
  total_messages: number;
}

interface MemoryReadResult {
  memory: MemoryInfo;
  user: UserProfile;
  stats: MemoryStats;
}

const CAPACITY_LIMIT = 100_000; // ~100KB char limit for display

export function MemoryScreen() {
  const [memory, setMemory] = useState<MemoryReadResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [newEntry, setNewEntry] = useState("");
  const [editIndex, setEditIndex] = useState<number | null>(null);
  const [editValue, setEditValue] = useState("");
  const [userContent, setUserContent] = useState("");
  const [userDirty, setUserDirty] = useState(false);
  const [saving, setSaving] = useState(false);

  const loadMemory = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<MemoryReadResult>("read_memory_cmd", {
        profile: null,
      });
      setMemory(result);
      setUserContent(result.user.content || "");
      setUserDirty(false);
    } catch (err) {
      console.error("Failed to load memory:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadMemory();
  }, [loadMemory]);

  const handleAddEntry = async () => {
    if (!newEntry.trim()) return;
    try {
      await invoke("add_memory_entry_cmd", {
        content: newEntry.trim(),
        profile: null,
      });
      setNewEntry("");
      await loadMemory();
    } catch (err) {
      console.error("Failed to add entry:", err);
    }
  };

  const handleUpdateEntry = async (index: number) => {
    if (!editValue.trim()) return;
    try {
      await invoke("update_memory_entry_cmd", {
        index,
        content: editValue.trim(),
        profile: null,
      });
      setEditIndex(null);
      setEditValue("");
      await loadMemory();
    } catch (err) {
      console.error("Failed to update entry:", err);
    }
  };

  const handleRemoveEntry = async (index: number) => {
    try {
      await invoke("remove_memory_entry_cmd", {
        index,
        profile: null,
      });
      await loadMemory();
    } catch (err) {
      console.error("Failed to remove entry:", err);
    }
  };

  const handleSaveUser = async () => {
    try {
      setSaving(true);
      await invoke("write_user_profile_cmd", {
        content: userContent,
        profile: null,
      });
      setUserDirty(false);
    } catch (err) {
      console.error("Failed to save user profile:", err);
    } finally {
      setSaving(false);
    }
  };

  const memoryEntries = memory?.memory.content
    ? memory.memory.content
        .split("\n")
        .filter((l) => l.trim().length > 0)
    : [];

  const capacityUsed = memory?.memory.content.length ?? 0;
  const capacityPercent = Math.min(
    Math.round((capacityUsed / CAPACITY_LIMIT) * 100),
    100
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader className="w-5 h-5 text-ac-amber animate-spin" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto">
        {/* Header */}
        <div className="px-4 py-3 border-b border-ac-border">
          <h2 className="text-sm font-semibold text-ac-ivory flex items-center gap-2">
            <Brain className="w-4 h-4 text-ac-amber" />
            Memory
          </h2>
        </div>

        {/* Stats cards */}
        <div className="grid grid-cols-2 gap-3 px-4 py-3">
          <div className="bg-ac-surface rounded-lg p-3 border border-ac-border">
            <div className="flex items-center gap-2 mb-1">
              <BarChart3 className="w-3.5 h-3.5 text-ac-amber" />
              <span className="text-[10px] text-ac-stone uppercase tracking-wider">
                Sessions
              </span>
            </div>
            <p className="text-lg font-semibold text-ac-ivory">
              {memory?.stats.total_sessions ?? 0}
            </p>
          </div>
          <div className="bg-ac-surface rounded-lg p-3 border border-ac-border">
            <div className="flex items-center gap-2 mb-1">
              <FileText className="w-3.5 h-3.5 text-ac-blue" />
              <span className="text-[10px] text-ac-stone uppercase tracking-wider">
                Messages
              </span>
            </div>
            <p className="text-lg font-semibold text-ac-ivory">
              {memory?.stats.total_messages ?? 0}
            </p>
          </div>
        </div>

        {/* Capacity bar */}
        <div className="px-4 pb-3">
          <div className="flex items-center justify-between text-[10px] text-ac-stone mb-1">
            <span>Memory capacity</span>
            <span>
              {capacityUsed.toLocaleString()} /{" "}
              {CAPACITY_LIMIT.toLocaleString()} chars
            </span>
          </div>
          <div className="h-1.5 bg-ac-surface rounded-full overflow-hidden">
            <div
              className={`h-full rounded-full transition-all ${
                capacityPercent > 90
                  ? "bg-ac-red"
                  : capacityPercent > 70
                  ? "bg-ac-yellow"
                  : "bg-ac-amber"
              }`}
              style={{ width: `${capacityPercent}%` }}
            />
          </div>
        </div>

        {/* Memory entries */}
        <div className="px-4 pb-3">
          <h3 className="text-xs font-medium text-ac-stone uppercase tracking-wider mb-2">
            Memory Entries
          </h3>

          {/* Add new entry */}
          <div className="flex gap-2 mb-3">
            <input
              type="text"
              value={newEntry}
              onChange={(e) => setNewEntry(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleAddEntry()}
              placeholder="Add memory entry..."
              className="ac-input flex-1 text-sm px-3 py-1.5"
            />
            <button
              onClick={handleAddEntry}
              disabled={!newEntry.trim()}
              className="ac-btn px-3 py-1.5 text-sm disabled:opacity-40"
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Entry list */}
          <div className="space-y-1">
            {memoryEntries.length === 0 ? (
              <p className="text-xs text-ac-stone/50 italic py-2">
                No memory entries yet
              </p>
            ) : (
              memoryEntries.map((entry, i) => (
                <div
                  key={i}
                  className="group flex items-start gap-2 py-1.5 px-2 rounded hover:bg-ac-surface/50"
                >
                  {editIndex === i ? (
                    <div className="flex-1 flex gap-1">
                      <input
                        type="text"
                        value={editValue}
                        onChange={(e) => setEditValue(e.target.value)}
                        onKeyDown={(e) =>
                          e.key === "Enter" && handleUpdateEntry(i)
                        }
                        className="ac-input flex-1 text-xs px-2 py-1"
                        autoFocus
                      />
                      <button
                        onClick={() => handleUpdateEntry(i)}
                        className="text-ac-green hover:text-ac-green/80 p-0.5"
                      >
                        <Check className="w-3 h-3" />
                      </button>
                      <button
                        onClick={() => setEditIndex(null)}
                        className="text-ac-stone hover:text-ac-ivory p-0.5"
                      >
                        <X className="w-3 h-3" />
                      </button>
                    </div>
                  ) : (
                    <>
                      <span className="flex-1 text-xs text-ac-ivory/80 leading-relaxed">
                        {entry.replace(/^-\s*/, "")}
                      </span>
                      <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          onClick={() => {
                            setEditIndex(i);
                            setEditValue(
                              entry.replace(/^-\s*/, "")
                            );
                          }}
                          className="text-ac-stone hover:text-ac-blue p-0.5"
                        >
                          <Edit3 className="w-3 h-3" />
                        </button>
                        <button
                          onClick={() => handleRemoveEntry(i)}
                          className="text-ac-stone hover:text-ac-red p-0.5"
                        >
                          <Trash2 className="w-3 h-3" />
                        </button>
                      </div>
                    </>
                  )}
                </div>
              ))
            )}
          </div>
        </div>

        {/* User profile */}
        <div className="px-4 pb-4">
          <h3 className="text-xs font-medium text-ac-stone uppercase tracking-wider mb-2 flex items-center gap-2">
            <User className="w-3 h-3" />
            User Profile (user.md)
          </h3>
          <textarea
            value={userContent}
            onChange={(e) => {
              setUserContent(e.target.value);
              setUserDirty(true);
            }}
            placeholder="Enter user profile information..."
            className="ac-input w-full text-xs px-3 py-2 min-h-[120px] resize-y font-mono"
          />
          {userDirty && (
            <div className="flex justify-end mt-2">
              <button
                onClick={handleSaveUser}
                disabled={saving}
                className="ac-btn px-4 py-1.5 text-xs flex items-center gap-1.5"
              >
                {saving ? (
                  <Loader className="w-3 h-3 animate-spin" />
                ) : (
                  <Save className="w-3 h-3" />
                )}
                Save Profile
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Footer */}
      <div className="px-4 py-2 border-t border-ac-border text-[10px] text-ac-stone/50 flex justify-between">
        <span>
          Last modified:{" "}
          {memory?.memory.last_modified
            ? new Date(memory.memory.last_modified * 1000).toLocaleString()
            : "N/A"}
        </span>
        <button
          onClick={loadMemory}
          className="text-ac-stone hover:text-ac-amber transition-colors"
          title="Refresh"
        >
          <RefreshCw className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}
