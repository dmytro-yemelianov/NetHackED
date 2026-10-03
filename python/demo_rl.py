#!/usr/bin/env python3
"""NetRust Gymnasium RL Demo & Benchmark.

Simulates an episode with the NetRust Gym environment, evaluating
a basic heuristic agent vs random policy and printing telemetry.
"""

import sys
import random
from pathlib import Path

# Add python directory to path
sys.path.insert(0, str(Path(__file__).resolve().parent))

# Ensure target/debug or target/release has netrust_py
repo_root = Path(__file__).resolve().parent.parent
target_debug = repo_root / "target" / "debug"
if (target_debug / "libnetrust_py.dylib").exists() and not (target_debug / "netrust_py.so").exists():
    import shutil
    shutil.copy(target_debug / "libnetrust_py.dylib", target_debug / "netrust_py.so")

from netrust_gym import NetRustGymEnv, ACTION_NAMES

def run_agent_episode(policy_name: str = "heuristic", seed: int = 42, max_steps: int = 150):
    env = NetRustGymEnv(seed=seed, max_steps=max_steps, render_mode="ansi")
    obs, info = env.reset()

    total_reward = 0.0
    steps = 0
    terminated = False
    truncated = False

    print(f"==================================================")
    print(f" Starting NetRust RL Episode: {policy_name.upper()} (Seed {seed})")
    print(f"==================================================")

    while not (terminated or truncated):
        steps += 1

        if policy_name == "heuristic":
            # Survival / Exploration heuristic:
            # 1. If HP low (< 8), pray or engrave Elbereth
            # 2. Descend if on stairs
            # 3. Otherwise explore cardinal directions
            hp = obs["player_hp"]
            if hp <= 6:
                action = 14 # Engrave Elbereth
            elif steps % 15 == 0:
                action = 9  # Try descend
            elif steps % 7 == 0:
                action = 11 # Try pick up
            else:
                action = (steps % 8) # Compass walk
        elif policy_name == "random":
            action = random.randint(0, len(ACTION_NAMES) - 1)
        else:
            action = 8 # Wait

        obs, reward, terminated, truncated, info = env.step(action)
        total_reward += reward

        if steps % 25 == 0 or terminated or truncated:
            print(f"[Step {steps:3d}] Action: {ACTION_NAMES[action]:<15} | Reward: {reward:6.2f} (Total: {total_reward:6.2f}) | HP: {obs['player_hp']}/{obs['player_max_hp']} | Depth: {obs['depth']} | Gold: {obs['gold']}")

    outcome = "VICTORY" if info.get("won") else ("DEAD" if info.get("is_dead") else "TRUNCATED")
    print(f"--------------------------------------------------")
    print(f" Episode Finished in {steps} steps | Outcome: {outcome} | Total Reward: {total_reward:.2f}")
    print(f" Final Floor View:")
    print(env.render())
    return {
        "policy": policy_name,
        "seed": seed,
        "steps": steps,
        "total_reward": total_reward,
        "outcome": outcome,
        "final_hp": obs["player_hp"],
        "depth": obs["depth"]
    }

if __name__ == "__main__":
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 42
    print("Testing Heuristic Agent...")
    h_result = run_agent_episode("heuristic", seed=seed, max_steps=100)

    print("\nTesting Random Policy...")
    r_result = run_agent_episode("random", seed=seed, max_steps=100)

    print("\nSummary Comparison:")
    print(f"Heuristic Reward: {h_result['total_reward']:.2f} (Steps: {h_result['steps']}, Depth: {h_result['depth']})")
    print(f"Random    Reward: {r_result['total_reward']:.2f} (Steps: {r_result['steps']}, Depth: {r_result['depth']})")
