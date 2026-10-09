"""Local protocol fixture; never connects to a model."""
import json
import os
from pathlib import Path
import sys
import time


mode = os.environ.get("FAKE_MODE", "complete")
queued = []


def emit(message):
    if mode == "queued" and message.get("method") in {"thread/tokenUsage/updated", "item/completed", "turn/completed"}:
        queued.append(message)
        if message["method"] == "turn/completed":
            sys.stdout.write("".join(json.dumps(m) + "\n" for m in queued))
            sys.stdout.flush()
            queued.clear()
        return
    print(json.dumps(message), flush=True)


def notify(method, params):
    emit({"method": method, "params": params})


def usage(input_tokens=1000000, cached=999990, output=4):
    notify("thread/tokenUsage/updated", {
        "threadId": "t", "turnId": "u", "tokenUsage": {"total": {
            "inputTokens": input_tokens, "cachedInputTokens": cached,
            "outputTokens": output, "reasoningOutputTokens": 1}}})


for line in sys.stdin:
    request = json.loads(line)
    method, params = request["method"], request.get("params", {})
    if method == "initialize":
        if mode == "no_initialize":
            time.sleep(60)
        emit({"id": request["id"], "result": {}})
    elif method == "thread/start":
        emit({"id": request["id"], "result": {
            "thread": {"id": "t"}, "model": params["model"], "reasoningEffort": "medium",
            "modelProvider": "openai", "cwd": params["cwd"],
            "approvalPolicy": "never", "sandbox": {"type": "workspaceWrite"}}})
    elif method == "turn/start":
        if os.environ.get("FAKE_STDIN"):
            Path(os.environ["FAKE_STDIN"]).write_text(params["input"][0]["text"])
        notify("turn/started", {"threadId": "t", "turn": {"id": "u"}})
        emit({"id": request["id"], "result": {"turn": {"id": "u"}}})
        if mode == "no_telemetry":
            continue
        if mode == "input_limit":
            usage(cached=970000)
            continue
        if mode == "output_limit":
            usage(output=8000)
            continue
        usage(cached=970000 if mode == "queued" else 999990)
        if mode == "duplicate":
            usage()
        elif mode == "decreasing":
            usage(input_tokens=9, cached=0)
            continue
        elif mode == "malformed":
            notify("thread/tokenUsage/updated", {"threadId": "t", "turnId": "u", "tokenUsage": None})
            continue
        elif mode == "commands":
            for i in range(3):
                notify("item/started", {"threadId": "t", "turnId": "u", "item": {"type": "commandExecution", "id": str(i)}})
            continue
        elif mode == "wall":
            continue
        notify("item/completed", {"threadId": "t", "turnId": "u",
                                  "item": {"type": "agentMessage", "text": '{}', "phase": "final_answer"}})
        notify("turn/completed", {"threadId": "t", "turn": {"id": "u", "status": "completed"}})
    elif method == "turn/interrupt":
        if os.environ.get("FAKE_INTERRUPTED"):
            Path(os.environ["FAKE_INTERRUPTED"]).write_text("interrupted")
        emit({"id": request["id"], "result": {}})
        notify("turn/completed", {"threadId": "t", "turn": {"id": "u", "status": "interrupted"}})
