"""An MCP server over standard input and output, for the tests: `say` says its words back, `fail` fails, `where` tells
the folder it runs in and the variable GREETING, and `picture` gives a picture of one pixel."""

import json
import os
import sys

PIXEL = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="

TOOLS = [
    {
        "name": "say",
        "description": "Says the words back.",
        "inputSchema": {
            "type": "object",
            "properties": {"words": {"type": "string", "description": "What to say."}},
            "required": ["words"],
        },
    },
    {"name": "fail", "description": "Fails.", "inputSchema": {"type": "object"}},
    {"name": "where", "description": "Tells where it runs.", "inputSchema": {"type": "object"}},
    {"name": "picture", "description": "Gives a picture.", "inputSchema": {"type": "object"}},
]


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def called(name, arguments):
    if name == "say":
        return {"content": [{"type": "text", "text": arguments["words"]}]}
    if name == "fail":
        return {"content": [{"type": "text", "text": "it failed"}], "isError": True}
    if name == "where":
        return {"content": [{"type": "text", "text": os.getcwd() + " " + os.environ.get("GREETING", "")}]}
    return {"content": [{"type": "image", "data": PIXEL, "mimeType": "image/png"}]}


while True:
    line = sys.stdin.readline()
    if not line:
        break
    message = json.loads(line)
    if "id" not in message or "method" not in message:
        continue
    method, params = message["method"], message.get("params") or {}
    if method == "initialize":
        result = {
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "test", "version": "1"},
        }
    elif method == "tools/list":
        result = {"tools": TOOLS}
    elif method == "tools/call":
        result = called(params["name"], params.get("arguments") or {})
    else:
        send({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32601, "message": "no method " + method}})
        continue
    send({"jsonrpc": "2.0", "id": message["id"], "result": result})
