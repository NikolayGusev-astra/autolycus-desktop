// src/constants.ts — shared constants for autolycus-desktop

export interface FieldDef {
  key: string;
  label: string;
  type: string;
  hint: string;
}

export interface SectionDef {
  title: string;
  items: FieldDef[];
}

// ── Theme ───────────────────────────────────────────────

export type ThemeAppearance = "dark" | "light";

export interface ThemeDef {
  id: string;
  name: string;
  appearance: ThemeAppearance;
}

export const THEMES: ThemeDef[] = [
  { id: "dark", name: "Dark", appearance: "dark" },
  { id: "light", name: "Light", appearance: "light" },
  { id: "dracula", name: "Dracula", appearance: "dark" },
  { id: "nord", name: "Nord", appearance: "dark" },
  { id: "one-dark", name: "One Dark", appearance: "dark" },
  { id: "github-dark", name: "GitHub Dark", appearance: "dark" },
  { id: "monokai", name: "Monokai", appearance: "dark" },
  { id: "solarized-dark", name: "Solarized Dark", appearance: "dark" },
  { id: "gruvbox-dark", name: "Gruvbox Dark", appearance: "dark" },
  { id: "tokyo-night", name: "Tokyo Night", appearance: "dark" },
  { id: "github-light", name: "GitHub Light", appearance: "light" },
  { id: "solarized-light", name: "Solarized Light", appearance: "light" },
];

export const THEME_OPTIONS = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export const DEFAULT_DARK_THEME = "dark";
export const DEFAULT_LIGHT_THEME = "light";
export const THEME_STORAGE_KEY = "hermes-theme";

// ── Font ────────────────────────────────────────────────

export interface FontOption {
  value: string;
  label: string;
  stack: string;
}

export const FONT_OPTIONS: FontOption[] = [
  {
    value: "manrope",
    label: "Manrope",
    stack:
      '"Manrope", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  },
  {
    value: "system",
    label: "System",
    stack:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Ubuntu, Cantarell, "Helvetica Neue", sans-serif',
  },
];

export const DEFAULT_FONT = "manrope";
export const FONT_STORAGE_KEY = "hermes-font";

// ── Providers ───────────────────────────────────────────

export const PROVIDERS = {
  options: [
    { value: "auto", label: "Auto-detect" },
    { value: "openrouter", label: "OpenRouter" },
    { value: "aimlapi", label: "AIMLAPI" },
    { value: "anthropic", label: "Anthropic" },
    { value: "openai", label: "OpenAI" },
    { value: "openai-codex", label: "OpenAI Codex" },
    { value: "ollama-cloud", label: "Ollama Cloud" },
    { value: "google", label: "Google" },
    { value: "xai", label: "xAI" },
    { value: "xiaomi", label: "Xiaomi MiMo" },
    { value: "mistral", label: "Mistral" },
    { value: "deepseek", label: "DeepSeek" },
    { value: "groq", label: "Groq" },
    { value: "together", label: "Together AI" },
    { value: "fireworks", label: "Fireworks AI" },
    { value: "cerebras", label: "Cerebras" },
    { value: "perplexity", label: "Perplexity" },
    { value: "huggingface", label: "Hugging Face" },
    { value: "nvidia", label: "NVIDIA NIM" },
    { value: "zai", label: "Z.ai / GLM" },
    { value: "qwen", label: "Qwen" },
    { value: "minimax", label: "MiniMax" },
    { value: "nous", label: "Nous" },
    { value: "lmstudio", label: "LM Studio" },
    { value: "atomicchat", label: "AtomicChat" },
    { value: "ollama", label: "Ollama" },
    { value: "vllm", label: "vLLM" },
    { value: "llamacpp", label: "llama.cpp" },
    { value: "xai-oauth", label: "xAI Grok (OAuth)" },
    { value: "qwen-oauth", label: "Qwen (OAuth)" },
    { value: "google-gemini-cli", label: "Gemini (CLI OAuth)" },
    { value: "minimax-oauth", label: "MiniMax (OAuth)" },
    { value: "kimi-coding", label: "Kimi (Coding Plan)" },
    { value: "custom", label: "OpenAI Compatible / Local" },
  ],
  labels: {
    openrouter: "OpenRouter",
    aimlapi: "AIMLAPI",
    anthropic: "Anthropic",
    openai: "OpenAI",
    "openai-codex": "OpenAI Codex",
    "ollama-cloud": "Ollama Cloud",
    google: "Google",
    xai: "xAI",
    xiaomi: "Xiaomi MiMo",
    mistral: "Mistral",
    deepseek: "DeepSeek",
    groq: "Groq",
    together: "Together AI",
    fireworks: "Fireworks AI",
    cerebras: "Cerebras",
    perplexity: "Perplexity",
    huggingface: "Hugging Face",
    nvidia: "NVIDIA NIM",
    zai: "Z.ai / GLM",
    qwen: "Qwen",
    minimax: "MiniMax",
    nous: "Nous",
    lmstudio: "LM Studio",
    atomicchat: "AtomicChat",
    ollama: "Ollama",
    vllm: "vLLM",
    llamacpp: "llama.cpp",
    "xai-oauth": "xAI Grok (OAuth)",
    "qwen-oauth": "Qwen (OAuth)",
    "google-gemini-cli": "Gemini (CLI OAuth)",
    "minimax-oauth": "MiniMax (OAuth)",
    "kimi-coding": "Kimi (Coding Plan)",
    custom: "OpenAI Compatible / Local",
  } as Record<string, string>,
  setup: [
    {
      id: "openrouter",
      name: "OpenRouter",
      desc: "Access 200+ models via a single API key",
      tag: "Recommended",
      envKey: "OPENROUTER_API_KEY",
      url: "https://openrouter.ai/keys",
      placeholder: "sk-or-v1-...",
      configProvider: "openrouter",
      baseUrl: "https://openrouter.ai/api/v1",
      needsKey: true,
    },
    {
      id: "anthropic",
      name: "Anthropic",
      desc: "Claude models by Anthropic",
      tag: "",
      envKey: "ANTHROPIC_API_KEY",
      url: "https://console.anthropic.com/settings/keys",
      placeholder: "sk-ant-...",
      configProvider: "anthropic",
      baseUrl: "",
      needsKey: true,
    },
    {
      id: "aimlapi",
      name: "AIMLAPI",
      desc: "Access 100+ models via a single API key",
      tag: "",
      envKey: "AIMLAPI_API_KEY",
      url: "https://aimlapi.com/app/keys",
      placeholder: "sk-...",
      configProvider: "aimlapi",
      baseUrl: "https://api.aimlapi.com/v1",
      needsKey: true,
    },
    {
      id: "openai",
      name: "OpenAI",
      desc: "GPT models by OpenAI",
      tag: "",
      envKey: "OPENAI_API_KEY",
      url: "https://platform.openai.com/api-keys",
      placeholder: "sk-...",
      configProvider: "custom",
      baseUrl: "https://api.openai.com/v1",
      needsKey: true,
    },
    {
      id: "openai-codex",
      name: "OpenAI Codex",
      desc: "ChatGPT subscription (OAuth sign-in)",
      tag: "Subscription",
      envKey: "",
      url: "",
      placeholder: "",
      configProvider: "openai-codex",
      baseUrl: "",
      needsKey: false,
    },
    {
      id: "ollama-cloud",
      name: "Ollama Cloud",
      desc: "Managed Ollama inference",
      tag: "Cloud",
      envKey: "OLLAMA_API_KEY",
      url: "https://ollama.com/settings/keys",
      placeholder: "ollama_...",
      configProvider: "ollama-cloud",
      baseUrl: "https://ollama.com/v1",
      needsKey: true,
    },
    {
      id: "google",
      name: "Google",
      desc: "Gemini models via Google AI Studio",
      tag: "",
      envKey: "GOOGLE_API_KEY",
      url: "https://aistudio.google.com/app/apikey",
      placeholder: "AIza...",
      configProvider: "google",
      baseUrl: "",
      needsKey: true,
    },
    {
      id: "xai",
      name: "xAI",
      desc: "Grok models by xAI",
      tag: "",
      envKey: "XAI_API_KEY",
      url: "https://console.x.ai",
      placeholder: "xai-...",
      configProvider: "xai",
      baseUrl: "",
      needsKey: true,
    },
    {
      id: "xiaomi",
      name: "Xiaomi MiMo",
      desc: "MiMo models",
      tag: "",
      envKey: "XIAOMI_API_KEY",
      url: "https://platform.xiaomimimo.com",
      placeholder: "sk-...",
      configProvider: "xiaomi",
      baseUrl: "https://api.xiaomimimo.com/v1",
      needsKey: true,
    },
    {
      id: "nous",
      name: "Nous",
      desc: "Nous Portal — free access to fine-tuned models",
      tag: "Free",
      envKey: "",
      url: "",
      placeholder: "",
      configProvider: "nous",
      baseUrl: "",
      needsKey: false,
    },
    {
      id: "local",
      name: "Local",
      desc: "Any OpenAI-compatible local server",
      tag: "Local",
      envKey: "",
      url: "",
      placeholder: "sk-...",
      configProvider: "custom",
      baseUrl: "http://localhost:1234/v1",
      needsKey: false,
    },
  ],
};

export interface OAuthProviderDef {
  id: string;
  name: string;
  desc: string;
}

export const OAUTH_PROVIDERS: OAuthProviderDef[] = [
  { id: "openai-codex", name: "ChatGPT (Codex Plan)", desc: "Sign in with your ChatGPT subscription" },
  { id: "xai-oauth", name: "xAI Grok (OAuth)", desc: "Sign in with your xAI account" },
  { id: "qwen-oauth", name: "Qwen (OAuth)", desc: "Sign in with your Qwen account" },
  { id: "google-gemini-cli", name: "Gemini (CLI OAuth)", desc: "Sign in with your Google account" },
  { id: "minimax-oauth", name: "MiniMax (OAuth)", desc: "Sign in with your MiniMax account" },
  { id: "nous", name: "Nous Portal (OAuth)", desc: "Sign in with your Nous Portal account" },
];

export interface LocalPreset {
  id: string;
  name: string;
  baseUrl: string;
  group: "local" | "remote";
  envKey?: string;
}

export const LOCAL_PRESETS: LocalPreset[] = [
  { id: "lmstudio", name: "LM Studio", baseUrl: "http://localhost:1234/v1", group: "local" },
  { id: "atomicchat", name: "AtomicChat", baseUrl: "http://localhost:1337/v1", group: "local" },
  { id: "ollama", name: "Ollama", baseUrl: "http://localhost:11434/v1", group: "local" },
  { id: "vllm", name: "vLLM", baseUrl: "http://localhost:8000/v1", group: "local" },
  { id: "llamacpp", name: "llama.cpp", baseUrl: "http://localhost:8080/v1", group: "local" },
  { id: "groq", name: "Groq", baseUrl: "https://api.groq.com/openai/v1", group: "remote", envKey: "GROQ_API_KEY" },
  { id: "aimlapi", name: "AIMLAPI", baseUrl: "https://api.aimlapi.com/v1", group: "remote", envKey: "AIMLAPI_API_KEY" },
  { id: "deepseek", name: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", group: "remote", envKey: "DEEPSEEK_API_KEY" },
  { id: "together", name: "Together", baseUrl: "https://api.together.xyz/v1", group: "remote", envKey: "TOGETHER_API_KEY" },
  { id: "fireworks", name: "Fireworks", baseUrl: "https://api.fireworks.ai/inference/v1", group: "remote", envKey: "FIREWORKS_API_KEY" },
  { id: "cerebras", name: "Cerebras", baseUrl: "https://api.cerebras.ai/v1", group: "remote", envKey: "CEREBRAS_API_KEY" },
  { id: "mistral", name: "Mistral", baseUrl: "https://api.mistral.ai/v1", group: "remote", envKey: "MISTRAL_API_KEY" },
];

// ── Settings API Key Sections ───────────────────────────

export const SETTINGS_SECTIONS: SectionDef[] = [
  {
    title: "LLM Providers",
    items: [
      { key: "OPENROUTER_API_KEY", label: "OpenRouter API Key", type: "password", hint: "sk-or-v1-..." },
      { key: "OPENAI_API_KEY", label: "OpenAI API Key", type: "password", hint: "sk-..." },
      { key: "OLLAMA_API_KEY", label: "Ollama Cloud API Key", type: "password", hint: "ollama_..." },
      { key: "AIMLAPI_API_KEY", label: "AIMLAPI API Key", type: "password", hint: "sk-..." },
      { key: "ANTHROPIC_API_KEY", label: "Anthropic API Key", type: "password", hint: "sk-ant-..." },
      { key: "GROQ_API_KEY", label: "Groq API Key", type: "password", hint: "gsk_..." },
      { key: "DEEPSEEK_API_KEY", label: "DeepSeek API Key", type: "password", hint: "sk-..." },
      { key: "TOGETHER_API_KEY", label: "Together API Key", type: "password", hint: "..." },
      { key: "FIREWORKS_API_KEY", label: "Fireworks API Key", type: "password", hint: "..." },
      { key: "CEREBRAS_API_KEY", label: "Cerebras API Key", type: "password", hint: "..." },
      { key: "MISTRAL_API_KEY", label: "Mistral API Key", type: "password", hint: "..." },
      { key: "GOOGLE_API_KEY", label: "Google API Key", type: "password", hint: "AIza..." },
      { key: "XAI_API_KEY", label: "xAI API Key", type: "password", hint: "xai-..." },
      { key: "XIAOMI_API_KEY", label: "Xiaomi API Key", type: "password", hint: "sk-..." },
      { key: "NOUS_API_KEY", label: "Nous API Key", type: "password", hint: "..." },
      { key: "HF_TOKEN", label: "HuggingFace Token", type: "password", hint: "hf_..." },
      { key: "CUSTOM_API_KEY", label: "Custom API Key", type: "password", hint: "..." },
    ],
  },
  {
    title: "Tool API Keys",
    items: [
      { key: "EXA_API_KEY", label: "Exa Search API Key", type: "password", hint: "..." },
      { key: "TAVILY_API_KEY", label: "Tavily API Key", type: "password", hint: "tvly-..." },
      { key: "FIRECRAWL_API_KEY", label: "Firecrawl API Key", type: "password", hint: "..." },
      { key: "FAL_KEY", label: "Fal.ai Key", type: "password", hint: "..." },
    ],
  },
  {
    title: "Voice / STT",
    items: [
      { key: "VOICE_TOOLS_OPENAI_KEY", label: "OpenAI Voice Key", type: "password", hint: "sk-..." },
    ],
  },
];

// ── Gateway Sections ────────────────────────────────────

export const GATEWAY_SECTIONS: SectionDef[] = [
  {
    title: "Messaging Platforms",
    items: [
      { key: "TELEGRAM_BOT_TOKEN", label: "Telegram Bot Token", type: "password", hint: "..." },
      { key: "TELEGRAM_ALLOWED_USERS", label: "Telegram Allowed Users", type: "text", hint: "user1,user2" },
      { key: "DISCORD_BOT_TOKEN", label: "Discord Bot Token", type: "password", hint: "..." },
      { key: "SLACK_BOT_TOKEN", label: "Slack Bot Token", type: "password", hint: "..." },
      { key: "SLACK_APP_TOKEN", label: "Slack App Token", type: "password", hint: "..." },
    ],
  },
];

// ── Install ─────────────────────────────────────────────

export const UNIX_INSTALL_CMD =
  "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash";

export function getInstallCmd(): string {
  return UNIX_INSTALL_CMD;
}
