#!/usr/bin/env python3
"""
NetRust MCP Agent Client Example.

Demonstrates connecting an LLM agent / script to NetRust via Model Context Protocol (MCP)
over stdio JSON-RPC 2.0.
"""

import json
import subprocess
import sys

def send_rpc(proc, msg_id, method, params=None):
    payload = {
        "jsonrpc": "2.0",
        "id": msg_id,
        "method": method,
    }
    if params is not None:
        payload["params"] = params
    
    line = json.dumps(payload) + "\n"
    proc.stdin.write(line)
    proc.stdin.flush()
    
    resp_line = proc.stdout.readline()
    if not resp_line:
        return None
    return json.loads(resp_line)

def main():
    print("🚀 Launching NetRust MCP Server (netrust-mcp)...")
    cmd = ["cargo", "run", "-q", "-p", "netrust-agent", "--bin", "netrust-mcp"]
    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        # 1. Initialize MCP Protocol
        print("\n[1] Initializing MCP Session...")
        init_resp = send_rpc(proc, 1, "initialize", {"protocolVersion": "2024-11-05"})
        server_info = init_resp["result"]["serverInfo"]
        print(f"    Connected to: {server_info['name']} v{server_info['version']}")

        # 2. Query available MCP tools
        print("\n[2] Discovering Available Tools...")
        tools_resp = send_rpc(proc, 2, "tools/list", {})
        tools = tools_resp["result"]["tools"]
        for t in tools:
            print(f"    🛠️  Tool: {t['name']} — {t['description']}")

        # 3. Create a Wizard character
        print("\n[3] Creating Character via MCP tool 'netrust_reset_with_character'...")
        create_resp = send_rpc(proc, 3, "tools/call", {
            "name": "netrust_reset_with_character",
            "arguments": {
                "seed": 9999,
                "name": "Gandalf",
                "role": "wizard",
                "race": "elf",
                "gender": "male",
                "alignment": "chaotic",
            }
        })
        obs = json.loads(create_resp["result"]["content"][0]["text"])
        print(f"    ✨ Spawned: Gandalf the Wizard at ({obs['player_coord']['x']}, {obs['player_coord']['y']})")
        print(f"    ❤️  HP: {obs['player_hp']}/{obs['player_max_hp']} | AC: {obs['player_ac']}")

        # 4. Display initial ASCII Viewport
        print("\n[4] Initial Viewport:")
        print(obs["ascii_map"])

        # 5. Execute consecutive actions
        actions = ["move_east", "wait", "move_south"]
        for idx, act in enumerate(actions, start=4):
            print(f"\n[{idx}] Agent stepping: {act}...")
            step_resp = send_rpc(proc, idx, "tools/call", {
                "name": "netrust_step",
                "arguments": { "action": act }
            })
            step_obs = json.loads(step_resp["result"]["content"][0]["text"])
            print(f"    Turn: {step_obs['turn']} | HP: {step_obs['player_hp']} | Pos: ({step_obs['player_coord']['x']}, {step_obs['player_coord']['y']})")
            if step_obs["last_events"]:
                print(f"    Events: {step_obs['last_events']}")

        print("\n✅ MCP interaction test succeeded!")

    finally:
        proc.stdin.close()
        proc.terminate()
        proc.wait()

if __name__ == "__main__":
    main()
