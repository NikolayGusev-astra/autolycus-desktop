// src/components/skills/SkillsScreen.tsx
// v0.5.0: Skills screen — list, install, uninstall, view content via Rust

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Puzzle,
  Download,
  Trash2,
  Eye,
  EyeOff,
  RefreshCw,
  Loader,
} from "lucide-react";

interface InstalledSkill {
  name: string;
  category: string;
  description: string;
  path: string;
}

export function SkillsScreen() {
  const [skills, setSkills] = useState<InstalledSkill[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState<string>("");
  const [contentLoading, setContentLoading] = useState(false);
  const [installName, setInstallName] = useState("");
  const [installing, setInstalling] = useState(false);

  const loadSkills = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<InstalledSkill[]>(
        "list_installed_skills_cmd",
        { profile: null }
      );
      setSkills(result);
    } catch (err) {
      console.error("Failed to load skills:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSkills();
  }, [loadSkills]);

  const handleViewSkill = async (skill: InstalledSkill) => {
    if (selectedSkill === skill.name) {
      setSelectedSkill(null);
      setSkillContent("");
      return;
    }
    try {
      setContentLoading(true);
      setSelectedSkill(skill.name);
      const content = await invoke<string>("get_skill_content_cmd", {
        skillPath: skill.path,
      });
      setSkillContent(content);
    } catch (err) {
      console.error("Failed to load skill content:", err);
      setSkillContent(`Error loading SKILL.md: ${err}`);
    } finally {
      setContentLoading(false);
    }
  };

  const handleInstall = async () => {
    if (!installName.trim()) return;
    try {
      setInstalling(true);
      await invoke("install_skill_cmd", {
        identifier: installName.trim(),
        profile: null,
      });
      setInstallName("");
      await loadSkills();
    } catch (err) {
      console.error("Failed to install skill:", err);
    } finally {
      setInstalling(false);
    }
  };

  const handleUninstall = async (name: string) => {
    try {
      await invoke("uninstall_skill_cmd", {
        name,
        profile: null,
      });
      if (selectedSkill === name) {
        setSelectedSkill(null);
        setSkillContent("");
      }
      await loadSkills();
    } catch (err) {
      console.error("Failed to uninstall skill:", err);
    }
  };

  const categories = [...new Set(skills.map((s) => s.category || "uncategorized"))].sort();

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto">
        {/* Header */}
        <div className="px-4 py-3 border-b border-ac-border">
          <h2 className="text-sm font-semibold text-ac-ivory flex items-center gap-2">
            <Puzzle className="w-4 h-4 text-ac-amber" />
            Skills
          </h2>
        </div>

        {/* Install skill */}
        <div className="px-4 py-3 border-b border-ac-border/50">
          <div className="flex gap-2">
            <input
              type="text"
              value={installName}
              onChange={(e) => setInstallName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleInstall()}
              placeholder="Install skill by name..."
              className="ac-input flex-1 text-sm px-3 py-1.5"
            />
            <button
              onClick={handleInstall}
              disabled={!installName.trim() || installing}
              className="ac-btn px-3 py-1.5 text-sm disabled:opacity-40 flex items-center gap-1"
            >
              {installing ? (
                <Loader className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Download className="w-3.5 h-3.5" />
              )}
              Install
            </button>
          </div>
        </div>

        {/* Skills list */}
        <div className="px-4 py-3">
          {loading ? (
            <div className="flex items-center justify-center h-32">
              <Loader className="w-5 h-5 text-ac-amber animate-spin" />
            </div>
          ) : skills.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-32 text-ac-stone text-sm">
              <Puzzle className="w-8 h-8 mb-2 opacity-30" />
              <p>No skills installed</p>
              <p className="text-[10px] mt-1">Install a skill using the field above</p>
            </div>
          ) : (
            categories.map((cat) => {
              const catSkills = skills.filter(
                (s) => (s.category || "uncategorized") === cat
              );
              if (catSkills.length === 0) return null;
              return (
                <div key={cat} className="mb-4">
                  <h3 className="text-[10px] font-medium text-ac-stone uppercase tracking-wider mb-2">
                    {cat}
                  </h3>
                  <div className="space-y-1">
                    {catSkills.map((skill) => (
                      <div key={skill.name}>
                        <div className="group flex items-start gap-2 py-2 px-3 rounded-lg bg-ac-surface/30 border border-ac-border/50 hover:bg-ac-surface/60 transition-colors">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2">
                              <span className="text-xs font-medium text-ac-ivory">
                                {skill.name}
                              </span>
                              {skill.category && (
                                <span className="text-[10px] px-1.5 py-0.5 bg-ac-amber/10 text-ac-amber rounded">
                                  {skill.category}
                                </span>
                              )}
                            </div>
                            {skill.description && (
                              <p className="text-[11px] text-ac-stone mt-0.5 line-clamp-2">
                                {skill.description}
                              </p>
                            )}
                          </div>
                          <div className="flex gap-1 shrink-0">
                            <button
                              onClick={() => handleViewSkill(skill)}
                              className="text-ac-stone hover:text-ac-amber p-1 rounded transition-colors"
                              title={
                                selectedSkill === skill.name
                                  ? "Hide content"
                                  : "View SKILL.md"
                              }
                            >
                              {selectedSkill === skill.name ? (
                                <EyeOff className="w-3.5 h-3.5" />
                              ) : (
                                <Eye className="w-3.5 h-3.5" />
                              )}
                            </button>
                            <button
                              onClick={() => handleUninstall(skill.name)}
                              className="text-ac-stone hover:text-ac-red p-1 rounded transition-colors"
                              title="Uninstall"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </div>

                        {/* Expanded content */}
                        {selectedSkill === skill.name && (
                          <div className="mt-1 mb-2 ml-2 pl-3 border-l-2 border-ac-amber/30">
                            {contentLoading ? (
                              <div className="flex items-center gap-2 py-2">
                                <Loader className="w-3 h-3 text-ac-amber animate-spin" />
                                <span className="text-xs text-ac-stone">
                                  Loading...
                                </span>
                              </div>
                            ) : (
                              <pre className="text-[11px] text-ac-ivory/70 font-mono whitespace-pre-wrap p-2 bg-ac-surface/50 rounded max-h-48 overflow-y-auto">
                                {skillContent}
                              </pre>
                            )}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Footer */}
      <div className="px-4 py-2 border-t border-ac-border text-[10px] text-ac-stone/50 flex justify-between items-center">
        <span>{skills.length} skills installed</span>
        <button
          onClick={loadSkills}
          className="text-ac-stone hover:text-ac-amber transition-colors"
          title="Refresh"
        >
          <RefreshCw className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}
