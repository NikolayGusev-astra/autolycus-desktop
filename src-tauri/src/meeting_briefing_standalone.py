#!/usr/bin/env python3
"""
Standalone Meeting Briefing Generator

Runs independently of Hermes Agent — reads Hermes config.yaml for MCP
server definitions, spawns MCP servers with proper env vars, calls tools
directly via Python SDK, builds prompt, calls LLM via provider SDK.

Usage:
  python meeting_briefing_standalone.py --event-uid <uid> --hermes-home <path> --profile <name>

Output: JSON with {briefing_text, meeting_type, summary, event_uid}
"""

import asyncio
import json
import os
import sys
import subprocess
from pathlib import Path
from typing import Any, Dict, List, Optional

# Add hermes-python to path if needed
HERMES_PYTHON = os.environ.get("HERMES_PYTHON", "hermes_python")
sys.path.insert(0, HERMES_PYTHON)

try:
    import yaml
except ImportError:
    print(json.dumps({"error": "PyYAML not installed"}))
    sys.exit(1)


def load_hermes_config(hermes_home: Path, profile: Optional[str]) -> Dict[str, Any]:
    """Load config.yaml and resolve profile."""
    config_path = hermes_home / "config.yaml"
    if not config_path.exists():
        config_path = hermes_home / "config.yml"
    if not config_path.exists():
        raise FileNotFoundError(f"config.yaml not found in {hermes_home}")

    with open(config_path) as f:
        config = yaml.safe_load(f)

    # Resolve profile
    if profile is None:
        profile = config.get("active_profile", "default")

    # Merge profile settings into root
    profile_cfg = config.get("profiles", {}).get(profile, {})
    merged = {**config, **profile_cfg, "profile": profile}

    return merged


def resolve_env_vars(config: Dict[str, Any], hermes_home: Path) -> Dict[str, str]:
    """Build environment variables for MCP servers from config."""
    env = os.environ.copy()

    # HERMES_HOME
    env["HERMES_HOME"] = str(config.get("hermes_home", Path.home() / ".hermes"))

    # Provider configs -> env vars (e.g., OPENAI_API_KEY)
    providers = config.get("providers", {})
    for prov_name, prov_cfg in providers.items():
        if isinstance(prov_cfg, dict):
            for key, val in prov_cfg.items():
                env_key = f"{prov_name.upper()}_{key.upper()}"
                if isinstance(val, str) and val:
                    env[env_key] = val

    # MCP server env blocks
    mcp_servers = config.get("mcp_servers", {})
    for server_name, server_cfg in mcp_servers.items():
        if isinstance(server_cfg, dict) and "env" in server_cfg:
            for key, val in server_cfg["env"].items():
                if isinstance(val, str):
                    env[key] = val

    # Python path
    env["PYTHONUNBUFFERED"] = "1"
    env["PYTHONPATH"] = str(Path(__file__).parent.parent)  # hermes-python root

    return env


async def spawn_mcp_server(command: str, args: List[str], env: Dict[str, str]) -> asyncio.subprocess.Process:
    """Spawn MCP server as subprocess with stdin/stdout pipes."""
    proc = await asyncio.create_subprocess_exec(
        command, *args,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env,
    )
    return proc


class MCPClient:
    """Minimal JSON-RPC MCP client over stdio."""

    def __init__(self, proc: asyncio.subprocess.Process):
        self.proc = proc
        self._request_id = 0
        self._pending: Dict[int, asyncio.Future] = {}
        self._reader_task = asyncio.create_task(self._read_loop())

    async def _read_loop(self):
        while True:
            line = await self.proc.stdout.readline()
            if not line:
                break
            try:
                msg = json.loads(line.decode().strip())
            except json.JSONDecodeError:
                continue

            msg_id = msg.get("id")
            if msg_id is not None and msg_id in self._pending:
                fut = self._pending.pop(msg_id)
                if not fut.done():
                    if "error" in msg:
                        fut.set_exception(Exception(msg["error"].get("message", "MCP error")))
                    else:
                        fut.set_result(msg.get("result"))

    async def initialize(self) -> Dict[str, Any]:
        """Send initialize request."""
        return await self._request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "meeting-briefing", "version": "1.0.0"},
        })

    async def call_tool(self, name: str, arguments: Dict[str, Any]) -> Any:
        """Call a tool via tools/call."""
        result = await self._request("tools/call", {"name": name, "arguments": arguments})
        # FastMCP returns {"content": [{"type": "text", "text": "..."}]}
        if isinstance(result, dict) and "content" in result:
            text = "".join(c.get("text", "") for c in result["content"] if c.get("type") == "text")
            try:
                return json.loads(text)
            except json.JSONDecodeError:
                return text
        return result

    async def list_tools(self) -> List[Dict[str, Any]]:
        result = await self._request("tools/list", {})
        return result.get("tools", [])

    async def _request(self, method: str, params: Dict[str, Any]) -> Dict[str, Any]:
        self._request_id += 1
        msg_id = self._request_id
        fut = asyncio.get_event_loop().create_future()
        self._pending[msg_id] = fut

        msg = {"jsonrpc": "2.0", "id": msg_id, "method": method, "params": params}
        self.proc.stdin.write((json.dumps(msg) + "\n").encode())
        await self.proc.stdin.drain()

        return await fut

    async def shutdown(self):
        self.proc.terminate()
        await self.proc.wait()


async def spawn_and_connect(command: str, args: List[str], env: Dict[str, str]) -> MCPClient:
    """Spawn MCP server and return connected client."""
    proc = await spawn_mcp_server(command, args, env)
    client = MCPClient(proc)
    await client.initialize()
    return client


async def run_meeting_briefing(
    hermes_home: str,
    event_uid: str,
    profile: Optional[str] = None,
) -> Dict[str, Any]:
    """Main entry point: generate meeting briefing."""

    home = Path(hermes_home)
    config = load_hermes_config(home, profile)
    env = resolve_env_vars(config, home)

    # Find calendar MCP server
    mcp_servers = config.get("mcp_servers", {})
    calendar_server = None
    for name, cfg in mcp_servers.items():
        if "calendar" in name.lower() or (isinstance(cfg, dict) and cfg.get("type") == "calendar"):
            calendar_server = (name, cfg)
            break
    if not calendar_server:
        calendar_server = list(mcp_servers.items())[0]  # fallback

    cal_name, cal_cfg = calendar_server
    cal_cmd = cal_cfg.get("command", "python")
    cal_args = cal_cfg.get("args", ["-m", "rupost_calendar"])

    # Connect to calendar MCP
    cal_client = await spawn_and_connect(cal_cmd, cal_args, env)

    # 1. Find the event
    events_result = await cal_client.call_tool("list_inbox", {
        "unread_only": False,
        "days": 7,
        "limit": 50,
    })
    # Calendar returns different structure - handle both
    events = []
    if isinstance(events_result, dict):
        if "messages" in events_result:
            events = events_result["messages"]
        elif "events" in events_result:
            events = events_result["events"]
        elif "result" in events_result:
            events = events_result["result"]
    elif isinstance(events_result, list):
        events = events_result

    event = next((e for e in events if e.get("uid") == event_uid or e.get("id") == event_uid), None)
    if not event:
        # Try list_events with specific date range
        events_result = await cal_client.call_tool("list_events", {
            "calendar_url": config.get("mcp_servers", {}).get(cal_name, {}).get("env", {}).get("CALENDAR_URL", ""),
            "since": "2024-01-01T00:00:00",
            "until": "2025-12-31T23:59:59",
            "limit": 100,
        })
        if isinstance(events_result, dict):
            events = events_result.get("events", events_result.get("result", []))
        else:
            events = events_result
        event = next((e for e in events if e.get("uid") == event_uid or e.get("id") == event_uid), None)

    if not event:
        raise ValueError(f"Event {event_uid} not found")

    # 2. Classify meeting
    meeting_type = classify_meeting(event.get("summary", ""), event.get("organizer", ""))

    # 2b. Connect to productivity MCP (tasks, sessions)
    # We'll use the same mcp_servers config - find productivity-related servers
    productivity_client = None
    for name, cfg in mcp_servers.items():
        if "productivity" in name.lower() or "task" in name.lower():
            prod_cmd = cfg.get("command", "python")
            prod_args = cfg.get("args", ["-m", "steersman_mcp"])
            productivity_client = await spawn_and_connect(prod_cmd, prod_args, env)
            break

    # 3. Gather related tasks
    related_tasks = []
    if productivity_client:
        try:
            tasks_result = await productivity_client.call_tool("list_tasks", {"status": "active"})
            if isinstance(tasks_result, dict):
                all_tasks = tasks_result.get("tasks", tasks_result.get("result", []))
            else:
                all_tasks = tasks_result

            keyword = event.get("organizer", "").split("@")[0].lower()
            summary_lower = event.get("summary", "").lower()
            for t in all_tasks:
                title = t.get("title", "").lower()
                if summary_lower in title or (keyword and keyword in title):
                    related_tasks.append(t.get("title", ""))
    related_tasks = related_tasks[:10]

    # 3b. Gather recent session previews
    recent_sessions = []
    if productivity_client:
        try:
            sessions_result = await productivity_client.call_tool("search_sessions", {"query": "", "limit": 20})
            if isinstance(sessions_result, dict):
                recent_sessions = sessions_result.get("sessions", sessions_result.get("result", []))
            else:
                recent_sessions = sessions_result
        except Exception:
            pass

    # 4. Build prompt
    prompt = build_meeting_briefing_prompt(
        event.get("summary", ""),
        event.get("description", ""),
        event.get("organizer", ""),
        event.get("attendees", []),
        meeting_type,
        related_tasks,
        recent_sessions[:10],
    )

    # 5. Call LLM directly via provider SDK
    briefing_text = await call_llm_direct(config, prompt)

    return {
        "event_uid": event_uid,
        "meeting_type": meeting_type,
        "briefing_text": briefing_text,
        "summary": event.get("summary", ""),
    }


def classify_meeting(summary: str, organizer: str) -> str:
    s = summary.lower()
    o = organizer.lower()
    if any(k in s for k in ["daily", "standup", "scrum", "дейли", "стендап", "утренний статус", "статус-митинг"]):
        return "daily"
    external = "@" in o and not o.endswith("@corp.ru") and not o.endswith("@rupost.ru")
    if external or any(k in s for k in ["заказчик", "клиент", "customer", "demo", "презент", "встреча по проект"]):
        return "customer"
    return "other"


def build_meeting_briefing_prompt(
    summary: str,
    description: str,
    organizer: str,
    attendees: List[str],
    meeting_type: str,
    related_tasks: List[str],
    recent_sessions: List[Dict[str, Any]],
) -> str:
    attendee_list = ", ".join(attendees) if attendees else "(нет данных об участниках)"
    tasks_block = "\n".join(f"- {t}" for t in related_tasks) if related_tasks else "(нет активных задач, связанных с этой темой)"
    sessions_block = "\n".join(f"- {s.get('title', '')} — {s.get('preview', '')[:160]}" for s in recent_sessions) if recent_sessions else "(нет связанных прошлых сессий)"

    focus = {
        "daily": "Это ДЕЙЛИ-статус. Сделай упор на: мои активные задачи и их статусы, что было проделано (по прошлым сессиям), блокеры, что сказать на статусе. Кратко — формат «что сделал / что делаю / блокеры».",
        "customer": "Это ВСТРЕЧА ПО ЗАКАЗЧИКУ. Сделай упор на: задачи, связанные с этим заказчиком/проектом, их статусы и открытые вопросы, что подготовить к встрече, какие решения нужны. Найди задачи по названию заказчика/проекта.",
        "other": "Сделай краткий бриф по встрече: контекст, что обсудить, статусы задач по теме.",
    }[meeting_type]

    return f"""Ты — персональный ИИ-ассистент руководителя. Подготовь КРАТКИЙ брифинг перед встречей.

ВСТРЕЧА:
- Название: {summary}
- Организатор (источник): {organizer}
- Участники: {attendee_list}
- Описание/повестка: {description if description else "(повестка не указана)"}

КОНТЕКСТ (уже собран):
Мои активные задачи по теме:
{tasks_block}

Прошлые сессии по теме:
{sessions_block}

ФОКУС БРИФИНГА: {focus}

Шаги:
1. Проанализируй задачи и сессии выше.
2. Сформулируй: что обсудить, статусы по задачам, открытые вопросы, что подготовить.
3. Для встреч по заказчику — найди задачи, связанные с этим заказчиком (по названию в задачах).

Формат: деловой стиль, русский, **жирным** для имён и статусов («Ждёт вас», «Мяч у вас», «БЕЗ ДВИЖЕНИЯ») и критичных проблем. Без длинных полотен.
Если данных нет — честно пиши «нет данных».
"""


async def call_llm_direct(config: Dict[str, Any], prompt: str) -> str:
    """Call LLM directly using provider SDK from config."""
    providers = config.get("providers", {})
    profile = config.get("profile", "default")

    # Find active provider
    active_provider = None
    for name, cfg in providers.items():
        if isinstance(cfg, dict) and cfg.get("enabled", True):
            active_provider = name
            break

    if not active_provider:
        return "Ошибка: не настроен провайдер LLM"

    provider_cfg = providers[active_provider]
    api_key = provider_cfg.get("api_key") or os.environ.get(f"{active_provider.upper()}_API_KEY")
    base_url = provider_cfg.get("base_url")
    model = provider_cfg.get("model", "gpt-4o-mini")

    if not api_key:
        return f"Ошибка: нет API ключа для {active_provider}"

    # Simple direct call via httpx
    import httpx

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }

    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": "Ты — деловой ассистент. Пиши кратко, по-русски, структурированно."},
            {"role": "user", "content": prompt},
        ],
        "temperature": 0.3,
        "max_tokens": 2000,
    }

    url = f"{base_url.rstrip('/')}/chat/completions" if base_url else "https://api.openai.com/v1/chat/completions"

    async with httpx.AsyncClient(timeout=60.0) as client:
        resp = await client.post(url, headers=headers, json=payload)
        if resp.status_code != 200:
            return f"Ошибка LLM ({resp.status_code}): {resp.text}"
        data = resp.json()
        return data["choices"][0]["message"]["content"]


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--event-uid", required=True)
    parser.add_argument("--hermes-home", required=True)
    parser.add_argument("--profile", default=None)
    args = parser.parse_args()

    try:
        result = asyncio.run(run_meeting_briefing(
            hermes_home=args.hermes_home,
            event_uid=args.event_uid,
            profile=args.profile,
        ))
        print(json.dumps(result, ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)