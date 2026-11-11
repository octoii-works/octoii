#!/usr/bin/env python3
"""A Claude Code of the tests' own: speaks `claude`'s stream-json control protocol as far as the plugin uses it.

Each start appends its arguments, as JSON, to the file `FAKE_CLAUDE_LOG` names. A session is a file under the
configuration's projects, its id carried on resume, a new one on a fork. To words that ask to "call echo <text>" it
answers with a call of the thread's tool `echo`, made through the MCP server it was told of, then answers with what the
call gave; to any other words it answers "heard: <words>".
"""

import json
import os
import sys
import uuid

arguments = sys.argv[1:]
with open(os.environ["FAKE_CLAUDE_LOG"], "a") as log:
    log.write(json.dumps(arguments) + "\n")


def flag(name):
    for argument in arguments:
        if argument.startswith(f"--{name}="):
            return argument.split("=", 1)[1]
    return None


config = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(os.environ["HOME"], ".claude")
folder = os.path.join(config, "projects", "".join(c if c.isalnum() else "-" for c in os.getcwd()))
resumed = flag("resume")
session = resumed if resumed and "--fork-session" not in arguments else str(uuid.uuid4())
kept = "--no-session-persistence" not in arguments
if kept:
    os.makedirs(folder, exist_ok=True)
    open(os.path.join(folder, f"{session}.jsonl"), "a").close()


def send(value):
    sys.stdout.write(json.dumps(value) + "\n")
    sys.stdout.flush()


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


asked = 0


def mcp(message):
    """Asks the plugin's MCP server `message`, and waits for its answer."""
    global asked
    asked += 1
    request = f"cli-{asked}"
    send({"type": "control_request", "request_id": request, "request": {"subtype": "mcp_message", "server_name": "tools", "message": message}})
    while True:
        value = read()
        if value.get("type") == "control_response" and value["response"]["request_id"] == request:
            return value["response"]["response"]["mcp_response"]


def stream(message_id, blocks, stop_reason):
    send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "message_start", "message": {"id": message_id, "type": "message", "role": "assistant", "content": [], "usage": {"input_tokens": 10, "output_tokens": 0}}}})
    for index, block in enumerate(blocks):
        if block["type"] == "text":
            send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}}})
            send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": block["text"]}}})
        else:
            send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "content_block_start", "index": index, "content_block": {"type": "tool_use", "id": block["id"], "name": block["name"], "input": {}}}})
            send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "content_block_delta", "index": index, "delta": {"type": "input_json_delta", "partial_json": json.dumps(block["input"])}}})
        send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "content_block_stop", "index": index}})
        send({"type": "assistant", "parent_tool_use_id": None, "uuid": str(uuid.uuid4()), "message": {"id": message_id, "role": "assistant", "content": [block]}})
    send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "message_delta", "delta": {"stop_reason": stop_reason}, "usage": {"output_tokens": 5}}})
    send({"type": "stream_event", "parent_tool_use_id": None, "event": {"type": "message_stop"}})


def ended(text):
    send({"type": "result", "subtype": "success", "is_error": False, "result": text, "usage": {"input_tokens": 10, "output_tokens": 5}, "modelUsage": {"claude-fake-1": {"contextWindow": 300000}}})


def words(content):
    texts = [block.get("text", "") for block in content if block.get("type") == "text"]
    return texts[-1] if texts else ""


said = 0
while True:
    value = read()
    kind = value.get("type")
    if kind == "control_request":
        request = value["request"]
        if request["subtype"] == "initialize":
            answer = {
                "claude_code_version": "9.9.9",
                "account": {"email": "person@example.com", "subscriptionType": "Claude Test"},
                "models": [
                    {"value": "fake", "resolvedModel": "claude-fake-1", "displayName": "Fake", "supportsEffort": True, "supportedEffortLevels": ["low", "high"], "supportsAdaptiveThinking": True, "supportsThinkingOff": False},
                ],
            }
        elif request["subtype"] == "get_context_usage":
            answer = {"rawMaxTokens": 200000}
        else:
            answer = {}
        send({"type": "control_response", "response": {"subtype": "success", "request_id": value["request_id"], "response": answer}})
    elif kind == "user":
        content = value["message"]["content"]
        if said == 0:
            send({"type": "system", "subtype": "init", "session_id": session})
        said += 1
        text = words(content)
        if text.startswith("call echo "):
            call = f"toolu_{said}"
            stream(f"msg_{said}a", [{"type": "tool_use", "id": call, "name": "mcp__tools__echo", "input": {"text": text[len("call echo "):]}}], "tool_use")
            mcp({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}}})
            listed = mcp({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
            assert [tool["name"] for tool in listed["result"]["tools"]] == ["echo"], listed
            given = mcp({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "echo", "arguments": {}, "_meta": {"claudecode/toolUseId": call}}})
            told = given["result"]["content"][0]["text"]
            stream(f"msg_{said}b", [{"type": "text", "text": f"done: {told}"}], "end_turn")
            ended(f"done: {told}")
        else:
            stream(f"msg_{said}", [{"type": "text", "text": f"heard: {text}"}], "end_turn")
            ended(f"heard: {text}")
