#!/usr/bin/env python3
"""
TCP server transport for tui_gateway.

Usage:
    python tcp_server.py --port 0

Wire protocol: newline-delimited JSON-RPC (same as stdio).
The server prints "READY:<port>" to stdout after binding,
so the parent process (Tauri/Rust) knows which port to connect to.

This reuses tui_gateway.server.dispatch() — all RPC handlers,
session management, and plugins work identically to stdio mode.
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import signal
import socket
import sys
import threading
import time
from typing import Optional

# ── Path setup ────────────────────────────────────────────────────────
# Add common autolycus install locations to sys.path so we can import
# tui_gateway.server regardless of where this script is bundled.

_SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
_HERE = _SCRIPT_DIR

# Try to find the autolycus/hermes-agent package
_search_roots = [
    # Bundled: python/ next to the Tauri binary
    _HERE,
    # Development: autolycus repo at ~/autolycus
    os.path.join(os.path.expanduser("~"), "autolycus"),
    os.path.join(os.path.expanduser("~"), ".autolycus"),
    # Hermes agent install
    os.path.join(os.path.expanduser("~"), ".hermes", "hermes-agent"),
    # System
    "/opt/autolycus",
    "/usr/local/lib/autolycus",
]

for _root in _search_roots:
    if _root and _root not in sys.path and os.path.isdir(_root):
        sys.path.insert(0, _root)

# Also respect HERMES_PYTHON_SRC_ROOT if set
_src_root = os.environ.get("HERMES_PYTHON_SRC_ROOT", "")
if _src_root and _src_root not in sys.path:
    sys.path.insert(0, _src_root)

# Strip CWD to avoid shadowing
sys.path = [p for p in sys.path if p not in {"", "."}]

# ── Imports ────────────────────────────────────────────────────────────

try:
    from tui_gateway import server as gw_server
    from tui_gateway.server import dispatch, resolve_skin
except ImportError as e:
    print(f"[tcp_server] FATAL: cannot import tui_gateway: {e}", file=sys.stderr, flush=True)
    print(f"[tcp_server] sys.path = {sys.path}", file=sys.stderr, flush=True)
    sys.exit(1)

log = logging.getLogger("tui_gateway.tcp_server")


# ── Transport ──────────────────────────────────────────────────────────

class TcpTransport:
    """Per-connection TCP transport that writes JSON lines to a socket."""

    __slots__ = ("_sock", "_lock", "_closed")

    def __init__(self, sock: socket.socket) -> None:
        self._sock = sock
        self._lock = threading.Lock()
        self._closed = False

    def write(self, obj: dict) -> bool:
        if self._closed:
            return False
        line = json.dumps(obj, ensure_ascii=False) + "\n"
        with self._lock:
            try:
                self._sock.sendall(line.encode("utf-8"))
                return True
            except (BrokenPipeError, ConnectionResetError, OSError):
                self._closed = True
                return False

    def close(self) -> None:
        self._closed = True
        try:
            self._sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        try:
            self._sock.close()
        except OSError:
            pass


# ── Client handler ─────────────────────────────────────────────────────

def handle_client(sock: socket.socket, addr: str) -> None:
    """Handle one TCP client connection (same protocol as stdio mode)."""
    transport = TcpTransport(sock)

    # Send gateway.ready
    if not transport.write({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": "gateway.ready", "payload": {"skin": resolve_skin()}},
    }):
        log.warning("Failed to send gateway.ready to %s", addr)
        transport.close()
        return

    buf = ""
    try:
        while True:
            data = sock.recv(65536)
            if not data:
                break

            buf += data.decode("utf-8", errors="replace")

            while "\n" in buf:
                line, buf = buf.split("\n", 1)
                line = line.strip()
                if not line:
                    continue

                try:
                    req = json.loads(line)
                except json.JSONDecodeError:
                    transport.write({
                        "jsonrpc": "2.0",
                        "error": {"code": -32700, "message": "parse error"},
                        "id": None,
                    })
                    continue

                resp = dispatch(req, transport)
                if resp is not None:
                    if not transport.write(resp):
                        break
    except (ConnectionResetError, BrokenPipeError):
        log.info("Client %s disconnected", addr)
    except Exception as exc:
        log.error("Error handling client %s: %s", addr, exc, exc_info=True)
    finally:
        transport.close()
        for _, sess in list(gw_server._sessions.items()):
            if sess.get("transport") is transport:
                sess["transport"] = gw_server._stdio_transport


# ── Main ───────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description="TUI Gateway TCP Server")
    parser.add_argument("--port", type=int, default=0, help="Port (0 = OS picks)")
    parser.add_argument("--host", default="127.0.0.1", help="Bind address")
    args = parser.parse_args()

    # MCP tool discovery (same as entry.py)
    try:
        from hermes_cli.config import read_raw_config
        mcp_servers = (read_raw_config() or {}).get("mcp_servers")
        has_mcp = isinstance(mcp_servers, dict) and len(mcp_servers) > 0
    except Exception:
        has_mcp = True

    if has_mcp:
        try:
            from tools.mcp_tool import discover_mcp_tools
            discover_mcp_tools()
        except Exception:
            pass

    # Bind TCP socket
    server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server_sock.bind((args.host, args.port))
    server_sock.listen(1)

    actual_port = server_sock.getsockname()[1]

    # Signal readiness to parent
    print(f"READY:{actual_port}", flush=True)
    log.info("TUI Gateway TCP server listening on %s:%d", args.host, actual_port)

    # Graceful shutdown
    shutdown_event = threading.Event()

    def _handle_signal(signum, frame):
        shutdown_event.set()

    if hasattr(signal, "SIGTERM"):
        signal.signal(signal.SIGTERM, _handle_signal)
    if hasattr(signal, "SIGINT"):
        signal.signal(signal.SIGINT, signal.SIG_IGN)

    server_sock.settimeout(1.0)

    while not shutdown_event.is_set():
        try:
            client_sock, client_addr = server_sock.accept()
        except socket.timeout:
            continue
        except OSError:
            break

        log.info("Client connected from %s:%d", *client_addr)
        handle_client(client_sock, f"{client_addr[0]}:{client_addr[1]}")

    server_sock.close()
    log.info("TUI Gateway TCP server shut down")


if __name__ == "__main__":
    main()
