// src/components/tools/ToolsScreen.tsx
import { useState } from "react";
import { Wrench } from "lucide-react";

interface ToolInfo {
  name: string;
  description: string;
  enabled: boolean;
  category: string;
}

const BUILTIN_TOOLS: ToolInfo[] = [
  { name: "web_search", description: "Search the web using configured search providers", enabled: true, category: "Search" },
  { name: "web_fetch", description: "Fetch and extract content from URLs", enabled: true, category: "Web" },
  { name: "terminal", description: "Execute shell commands on the local system", enabled: true, category: "System" },
  { name: "file_read", description: "Read files from the local filesystem", enabled: true, category: "Files" },
  { name: "file_write", description: "Write files to the local filesystem", enabled: true, category: "Files" },
  { name: "code_exec", description: "Execute Python code in a sandboxed environment", enabled: true, category: "Code" },
  { name: "image_gen", description: "Generate images using configured image models", enabled: false, category: "Media" },
  { name: "browser", description: "Control a headless browser for web automation", enabled: false, category: "Web" },
  { name: "memory_search", description: "Search through agent memory and notes", enabled: true, category: "Memory" },
  { name: "cron", description: "Schedule recurring tasks and reminders", enabled: true, category: "Automation" },
  { name: "spotify", description: "Control Spotify playback and search", enabled: false, category: "Media" },
  { name: "hue", description: "Control Philips Hue smart lights", enabled: false, category: "Smart Home" },
];

export function ToolsScreen() {
  const [tools, setTools] = useState<ToolInfo[]>(BUILTIN_TOOLS);
  const [filter, setFilter] = useState("all");

  const categories = ["all", ...new Set(BUILTIN_TOOLS.map((t) => t.category))];

  const filteredTools = filter === "all" ? tools : tools.filter((t) => t.category === filter);

  const toggleTool = (name: string) => {
    setTools((prev) =>
      prev.map((t) => (t.name === name ? { ...t, enabled: !t.enabled } : t))
    );
  };

  const enabledCount = tools.filter((t) => t.enabled).length;

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="max-w-3xl mx-auto">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-2xl font-bold">Tools</h1>
            <p className="text-sm text-ac-muted mt-1">
              {enabledCount} of {tools.length} tools enabled
            </p>
          </div>
        </div>

        {/* Category Filter */}
        <div className="flex gap-2 mb-6 flex-wrap">
          {categories.map((cat) => (
            <button
              key={cat}
              className={`px-3 py-1.5 rounded-lg text-sm transition-colors ${
                filter === cat
                  ? "bg-ac-blue text-white"
                  : "bg-ac-bg border border-ac-border hover:border-ac-stone"
              }`}
              onClick={() => setFilter(cat)}
            >
              {cat === "all" ? "All" : cat}
            </button>
          ))}
        </div>

        {/* Tools Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {filteredTools.map((tool) => (
            <div
              key={tool.name}
              className={`p-4 rounded-xl border transition-colors ${
                tool.enabled
                  ? "bg-ac-bg border-ac-border"
                  : "bg-ac-bg/50 border-ac-border/50 opacity-60"
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex items-start gap-3">
                  <Wrench className="w-4 h-4 text-ac-muted mt-0.5 shrink-0" />
                  <div>
                    <p className="font-medium text-sm font-mono">{tool.name}</p>
                    <p className="text-xs text-ac-muted mt-1">{tool.description}</p>
                    <span className="inline-block mt-2 text-xs bg-ac-surface px-2 py-0.5 rounded">
                      {tool.category}
                    </span>
                  </div>
                </div>
                <button
                  className={`relative w-10 h-5 rounded-full transition-colors shrink-0 ml-3 ${
                    tool.enabled ? "bg-ac-blue" : "bg-ac-border"
                  }`}
                  onClick={() => toggleTool(tool.name)}
                >
                  <div
                    className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                      tool.enabled ? "translate-x-5" : "translate-x-0.5"
                    }`}
                  />
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default ToolsScreen;
