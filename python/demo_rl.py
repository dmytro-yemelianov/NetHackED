#!/usr/bin/env python3
"""NetRust Gymnasium RL Autonomous Agent Demonstration & Benchmark.

Evaluates autonomous agent policies with action masking and voluntary conducts:
1. Pacifist Explorer (action-masked strict conduct obedience)
2. Tactical Delver (ranged weapons, Elbereth warding, secret searching)
3. Action-Masked Random Explorer
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

def run_agent_episode(policy_name: str = "pacifist", seed: int = 42, max_steps: int = 150):
    env = NetRustGymEnv(seed=seed, max_steps=max_steps, render_mode="ansi", conduct_masking=True)
    obs, info = env.reset(seed=seed)

    total_reward = 0.0
    steps = 0
    terminated = False
    truncated = False

    print(f"\n{'=' * 68}")
    print(f" NetRust RL Episode: {policy_name.upper()} (Seed {seed})")
    print(f"{'=' * 68}")

    while not (terminated or truncated):
        steps += 1
        mask = env.action_masks()
        valid_actions = [idx for idx, valid in enumerate(mask) if valid]

        if not valid_actions:
            valid_actions = [8] # Wait fallback

        if policy_name == "pacifist":
            # Strict Pacifist Survival & Exploration:
            # 1. If standing on stairs down and valid, descend!
            # 2. If item on floor and valid, pick up!
            # 3. If low HP, search or engrave Elbereth
            # 4. Otherwise navigate in compass directions that don't collide with monsters
            if 9 in valid_actions: # DESCEND
                action = 9
            elif 11 in valid_actions: # PICKUP
                action = 11
            elif 13 in valid_actions: # UNTRAP
                action = 13
            elif int(obs["player_hp"][0]) <= 8 and 12 in valid_actions: # SEARCH
                action = 12
            else:
                # Filter compass movement actions (0..7) that are in valid_actions
                compass_valid = [a for a in range(8) if a in valid_actions]
                if compass_valid:
                    action = compass_valid[steps % len(compass_valid)]
                else:
                    action = random.choice(valid_actions)

        elif policy_name == "tactical":
            # Tactical delver:
            # 1. If quivered item ready and fire action valid, fire projectile
            # 2. If on stairs, descend
            # 3. If item on floor, pickup
            # 4. Explore
            fire_actions = [a for a in [14, 15, 16, 17] if a in valid_actions]
            if fire_actions:
                action = random.choice(fire_actions)
            elif 9 in valid_actions:
                action = 9
            elif 18 in valid_actions: # QUIVER
                action = 18
            elif 11 in valid_actions: # PICKUP
                action = 11
            elif 12 in valid_actions: # SEARCH
                action = 12
            else:
                compass_valid = [a for a in range(8) if a in valid_actions]
                if compass_valid:
                    action = compass_valid[steps % len(compass_valid)]
                else:
                    action = random.choice(valid_actions)

        else: # "random_masked"
            action = random.choice(valid_actions)

        obs, reward, terminated, truncated, info = env.step(action)
        total_reward += reward

        if steps % 25 == 0 or terminated or truncated:
            action_name = ACTION_NAMES[action] if action < len(ACTION_NAMES) else f"Action({action})"
            conducts = info["conducts"]
            pacifist_tag = "[PACIFIST]" if conducts.get("pacifist") else "[KILLED]"
            vegan_tag = "[VEGAN]" if conducts.get("vegan") else "[CARN]"
            print(
                f"[Step {steps:3d}] {action_name:<16} | Rew: {reward:6.2f} (Tot: {total_reward:6.2f}) | "
                f"HP: {int(obs['player_hp'][0])}/{int(obs['player_max_hp'][0])} | Dlvl: {int(obs['depth'][0])} | "
                f"{pacifist_tag} {vegan_tag}"
            )

    outcome = "VICTORY" if info.get("won") else ("DEAD" if info.get("is_dead") else "TRUNCATED")
    final_conducts = info["conducts"]
    print(f"{'-' * 68}")
    print(f" Episode Finished in {steps} steps | Outcome: {outcome} | Total Reward: {total_reward:.2f}")
    print(f" Final Conducts Audit: Pacifist={final_conducts.get('pacifist')}, Vegan={final_conducts.get('vegan')}, Atheist={final_conducts.get('atheist')}, Illiterate={final_conducts.get('illiterate')}")
    print(f" Final ASCII Viewport:")
    print(env.render())

    return {
        "policy": policy_name,
        "seed": seed,
        "steps": steps,
        "total_reward": total_reward,
        "outcome": outcome,
        "final_hp": int(obs["player_hp"][0]),
        "depth": int(obs["depth"][0]),
        "conducts": final_conducts,
    }

if __name__ == "__main__":
    seed = int(sys.argv[1]) if len(sys.argv) > 1 else 42

    print("Evaluating Policy 1: Strict Pacifist Explorer (Masked)...")
    p_result = run_agent_episode("pacifist", seed=seed, max_steps=100)

    print("\nEvaluating Policy 2: Tactical Delver...")
    t_result = run_agent_episode("tactical", seed=seed, max_steps=100)

    print("\nEvaluating Policy 3: Action-Masked Random Policy...")
    r_result = run_agent_episode("random_masked", seed=seed, max_steps=100)

    print("\n" + "=" * 68)
    print(" SUMMARY BENCHMARK COMPARISON")
    print("=" * 68)
    print(f"Pacifist Explorer: Reward {p_result['total_reward']:6.2f} | Steps: {p_result['steps']:3d} | Dlvl: {p_result['depth']} | Pacifist Conduct: {p_result['conducts']['pacifist']}")
    print(f"Tactical Delver:   Reward {t_result['total_reward']:6.2f} | Steps: {t_result['steps']:3d} | Dlvl: {t_result['depth']} | Pacifist Conduct: {t_result['conducts']['pacifist']}")
    print(f"Random Masked:     Reward {r_result['total_reward']:6.2f} | Steps: {r_result['steps']:3d} | Dlvl: {r_result['depth']} | Pacifist Conduct: {r_result['conducts']['pacifist']}")
    print("=" * 68)
