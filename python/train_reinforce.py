#!/usr/bin/env python3
"""NetRust REINFORCE (vanilla policy gradient) training script.

Trains a compact 2-layer neural policy on NetRustGymEnv with the REINFORCE
algorithm (Monte-Carlo returns, normalized advantages, no baseline network, no
clipping -- this is not PPO). The policy math is pure Python; only the
environment needs gymnasium + numpy. Actions are sampled from the policy
distribution restricted to the env's action mask.
Exports trained weights to JSON for real-time 60 FPS in-browser WebAssembly evaluation.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import shutil
import sys
import time
from pathlib import Path
from typing import List, Tuple, Dict, Any, Optional

import numpy as np

# Ensure python directory and netrust_py library are loaded
script_dir = Path(__file__).resolve().parent
repo_root = script_dir.parent
sys.path.insert(0, str(script_dir))

try:
    import netrust_py  # noqa: F401  (installed via `maturin develop`)
except ImportError:
    target_debug = repo_root / "target" / "debug"
    target_release = repo_root / "target" / "release"
    for target_dir in (target_release, target_debug):
        dylib = target_dir / "libnetrust_py.dylib"
        so = target_dir / "netrust_py.so"
        if dylib.exists() and not so.exists():
            shutil.copy(dylib, so)
        if so.exists() and str(target_dir) not in sys.path:
            sys.path.insert(0, str(target_dir))

from netrust_gym import NetRustGymEnv, ACTION_NAMES, sample_masked_action

PRAY = ACTION_NAMES.index("PRAY")

NUM_FEATURES = 12
NUM_HIDDEN = 16
NUM_ACTIONS = len(ACTION_NAMES)

def _val(obs: Dict[str, Any], key: str, default: float) -> float:
    """Read a scalar observation (stored as a 1-element array)."""
    v = obs.get(key)
    if v is None:
        return float(default)
    return float(np.asarray(v).reshape(-1)[0])


def extract_features(obs: Dict[str, Any]) -> List[float]:
    """Convert NetRust gym observation dict into normalized feature vector."""
    hp = _val(obs, "player_hp", 18)
    hp_norm = hp / max(1.0, _val(obs, "player_max_hp", 1))
    hp_danger = 1.0 if hp <= 6 else 0.0
    ac_norm = _val(obs, "player_ac", 10) / 10.0
    depth_norm = _val(obs, "depth", 1) / 10.0
    gold_norm = min(5.0, _val(obs, "gold", 0) / 100.0)
    nutr_norm = _val(obs, "nutrition", 900) / 1000.0
    pw_norm = _val(obs, "pw", 5) / 20.0
    hostiles = min(2.0, max(0.0, _val(obs, "num_visible_actors", 1) - 1))
    x_norm = _val(obs, "player_x", 40) / 80.0
    y_norm = _val(obs, "player_y", 12) / 24.0
    t = _val(obs, "turn", 1)
    turn_sin = math.sin(t * 0.1)
    turn_cos = math.cos(t * 0.1)

    return [
        hp_norm, hp_danger, ac_norm, depth_norm,
        gold_norm, nutr_norm, pw_norm, hostiles,
        x_norm, y_norm, turn_sin, turn_cos
    ]

class NeuralPolicy:
    """Compact 2-Layer Multi-Layer Perceptron Policy Network."""

    def __init__(self, in_dim: int = NUM_FEATURES, h_dim: int = NUM_HIDDEN, out_dim: int = NUM_ACTIONS, seed: int = 42):
        rng = random.Random(seed)
        # Xavier/He initialization
        scale1 = math.sqrt(2.0 / in_dim)
        scale2 = math.sqrt(2.0 / h_dim)
        self.w1 = [[rng.gauss(0, scale1) for _ in range(in_dim)] for _ in range(h_dim)]
        self.b1 = [0.0] * h_dim
        self.w2 = [[rng.gauss(0, scale2) for _ in range(h_dim)] for _ in range(out_dim)]
        self.b2 = [0.0] * out_dim

    def forward(
        self, x: List[float], mask: Optional[List[bool]] = None
    ) -> Tuple[List[float], List[float], List[float]]:
        """Forward pass: returns (hidden_pre_act, hidden_post_act, action_probs).

        If `mask` is given, invalid actions get probability 0 (masked softmax).
        """
        # Layer 1: Linear + ReLU
        h_pre = [
            sum(w * xi for w, xi in zip(row, x)) + b
            for row, b in zip(self.w1, self.b1)
        ]
        h_post = [max(0.0, v) for v in h_pre]

        # Layer 2: Linear + Softmax
        logits = [
            sum(w * hi for w, hi in zip(row, h_post)) + b
            for row, b in zip(self.w2, self.b2)
        ]
        if mask is not None and any(mask):
            logits = [l if m else -math.inf for l, m in zip(logits, mask)]
        max_l = max(logits)
        exp_logits = [math.exp(l - max_l) for l in logits]
        sum_exp = sum(exp_logits)
        probs = [e / sum_exp for e in exp_logits]

        return h_pre, h_post, probs

    def sample_action(self, probs: List[float], rng: random.Random) -> int:
        r = rng.random()
        cum = 0.0
        last = None
        for i, p in enumerate(probs):
            if p > 0:
                last = i
            cum += p
            if p > 0 and r <= cum:
                return i
        # Rounding fallback: never return a masked (zero-probability) action.
        return last if last is not None else len(probs) - 1

    def export_weights(self) -> Dict[str, Any]:
        """Export serialized weights dictionary."""
        return {
            "model_type": "MLP-Policy",
            "in_dim": NUM_FEATURES,
            "hidden_dim": NUM_HIDDEN,
            "out_dim": NUM_ACTIONS,
            "action_names": ACTION_NAMES,
            "w1": self.w1,
            "b1": self.b1,
            "w2": self.w2,
            "b2": self.b2,
            "timestamp": time.time()
        }

def train_rl_agent(
    episodes: int = 60,
    max_steps_per_episode: int = 120,
    lr: float = 0.015,
    gamma: float = 0.98,
    seed: int = 42,
    out_path: Optional[Path] = None,
) -> Dict[str, Any]:
    print(f"============================================================")
    print(f" NetRust REINFORCE Training Engine")
    print(f" Features: {NUM_FEATURES} | Hidden: {NUM_HIDDEN} | Actions: {NUM_ACTIONS} | Episodes: {episodes}")
    print(f"============================================================")

    policy = NeuralPolicy(NUM_FEATURES, NUM_HIDDEN, NUM_ACTIONS, seed=seed)
    rng = random.Random(seed)
    np_rng = np.random.default_rng(seed)
    env = NetRustGymEnv(seed=seed, max_steps=max_steps_per_episode)
    episode_history = []
    start_time = time.time()

    for ep in range(1, episodes + 1):
        ep_seed = seed + ep * 17
        obs, _ = env.reset(seed=ep_seed)

        trajectory: List[Tuple[List[float], List[float], List[float], int, float, List[bool]]] = []
        total_reward = 0.0
        steps = 0
        terminated = False
        truncated = False

        while not (terminated or truncated):
            steps += 1
            x = extract_features(obs)
            mask = [bool(m) for m in env.action_masks()]
            h_pre, h_post, probs = policy.forward(x, mask)
            if any(mask):
                act = policy.sample_action(probs, rng)
            else:
                act = sample_masked_action(mask, np_rng)  # WAIT fallback
            prev_hp = _val(obs, "player_hp", 18)
            prev_depth = _val(obs, "depth", 1)

            obs, reward, terminated, truncated, info = env.step(act)

            # Reward shaping for dungeon progression
            shaped_reward = reward
            if prev_hp <= 6 and act == PRAY:  # Prayed when low
                shaped_reward += 0.8
            if _val(obs, "depth", 1) > prev_depth:  # Successfully descended
                shaped_reward += 3.0

            total_reward += shaped_reward
            trajectory.append((x, h_pre, h_post, act, shaped_reward, mask))

        # Compute discounted returns G_t
        T = len(trajectory)
        returns = [0.0] * T
        running_g = 0.0
        for t in reversed(range(T)):
            running_g = trajectory[t][4] + gamma * running_g
            returns[t] = running_g

        # Normalize returns (Advantage estimation)
        if T > 1:
            mean_ret = sum(returns) / T
            var_ret = sum((r - mean_ret) ** 2 for r in returns) / T
            std_ret = math.sqrt(var_ret) + 1e-7
            advantages = [(r - mean_ret) / std_ret for r in returns]
        else:
            advantages = [0.0] * T

        # Policy gradient backpropagation
        grad_w1 = [[0.0] * NUM_FEATURES for _ in range(NUM_HIDDEN)]
        grad_b1 = [0.0] * NUM_HIDDEN
        grad_w2 = [[0.0] * NUM_HIDDEN for _ in range(NUM_ACTIONS)]
        grad_b2 = [0.0] * NUM_ACTIONS

        for t in range(T):
            x, h_pre, h_post, act, _, mask = trajectory[t]
            adv = advantages[t]
            _, _, probs = policy.forward(x, mask)

            # Softmax policy gradient: dLogPi/dz_i = (1{i == a} - probs[i]) * adv
            d_logits = [0.0] * NUM_ACTIONS
            for i in range(NUM_ACTIONS):
                target_prob = 1.0 if i == act else 0.0
                d_logits[i] = (target_prob - probs[i]) * adv

            # Layer 2 gradients
            for a in range(NUM_ACTIONS):
                dl = d_logits[a]
                grad_b2[a] += dl
                for h in range(NUM_HIDDEN):
                    grad_w2[a][h] += dl * h_post[h]

            # Layer 1 backprop through ReLU
            d_h_post = [0.0] * NUM_HIDDEN
            for h in range(NUM_HIDDEN):
                d_h_post[h] = sum(d_logits[a] * policy.w2[a][h] for a in range(NUM_ACTIONS))

            for h in range(NUM_HIDDEN):
                if h_pre[h] > 0: # ReLU gradient
                    dh = d_h_post[h]
                    grad_b1[h] += dh
                    for f in range(NUM_FEATURES):
                        grad_w1[h][f] += dh * x[f]

        # Parameter update (Gradient Ascent)
        step_lr = lr / max(1, math.sqrt(ep))
        for a in range(NUM_ACTIONS):
            policy.b2[a] += step_lr * (grad_b2[a] / max(1, T))
            for h in range(NUM_HIDDEN):
                policy.w2[a][h] += step_lr * (grad_w2[a][h] / max(1, T))

        for h in range(NUM_HIDDEN):
            policy.b1[h] += step_lr * (grad_b1[h] / max(1, T))
            for f in range(NUM_FEATURES):
                policy.w1[h][f] += step_lr * (grad_w1[h][f] / max(1, T))

        episode_history.append({
            "episode": ep,
            "steps": steps,
            "reward": round(total_reward, 2),
            "final_hp": _val(obs, "player_hp", 0),
            "depth": _val(obs, "depth", 1)
        })

        if ep % 10 == 0 or ep == episodes:
            mean_rew = sum(e["reward"] for e in episode_history[-10:]) / min(len(episode_history), 10)
            mean_steps = sum(e["steps"] for e in episode_history[-10:]) / min(len(episode_history), 10)
            print(f"[Episode {ep:3d}/{episodes}] Mean Reward (last 10): {mean_rew:6.2f} | Mean Steps: {mean_steps:5.1f} | Final HP: {_val(obs, 'player_hp', 0):.0f}")

    elapsed = time.time() - start_time
    print(f"------------------------------------------------------------")
    print(f" Training complete in {elapsed:.2f}s ({episodes / elapsed:.1f} eps/sec)")
    print(f"============================================================")

    # Export weights to JSON
    weights_data = policy.export_weights()
    web_weights_path = out_path or (repo_root / "web" / "policy_weights.json")

    with open(web_weights_path, "w") as f:
        json.dump(weights_data, f, indent=2)

    print(f"Exported policy weights to: {web_weights_path}")
    return weights_data

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--episodes", type=int, default=60)
    parser.add_argument("--max-steps", type=int, default=120)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--out", type=Path, default=None,
                        help="weights JSON path (default: web/policy_weights.json)")
    args = parser.parse_args()
    train_rl_agent(
        episodes=args.episodes,
        max_steps_per_episode=args.max_steps,
        seed=args.seed,
        out_path=args.out,
    )
