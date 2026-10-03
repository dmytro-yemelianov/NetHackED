"""Gymnasium-compatible Environment for NetRust.

Provides standard step(), reset(), render() interfaces for RL algorithms
(PPO, DQN, A2C, Stable-Baselines3).
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, Dict, Tuple, Optional

# Dynamically locate netrust_py binary if not installed via pip
try:
    import netrust_py
except ImportError:
    # Check target/release or target/debug
    repo_root = Path(__file__).resolve().parent.parent.parent
    for build_type in ("release", "debug"):
        candidate = repo_root / "target" / build_type
        if candidate.exists() and str(candidate) not in sys.path:
            sys.path.insert(0, str(candidate))
    try:
        import netrust_py
    except ImportError as e:
        raise ImportError(
            f"Could not load netrust_py extension module. Run 'cargo build -p netrust-py' first. Error: {e}"
        )

# Attempt to import gymnasium if available
try:
    import gymnasium as gym
    from gymnasium import spaces
    HAS_GYMNASIUM = True
except ImportError:
    HAS_GYMNASIUM = False

ACTION_NAMES = [
    "North", "East", "South", "West",
    "NorthEast", "SouthEast", "SouthWest", "NorthWest",
    "Wait", "Descend", "Ascend", "PickUp", "Pay", "Pray", "EngraveElbereth"
]


class NetRustGymEnv:
    """NetRust Gymnasium Environment wrapper."""

    metadata = {"render_modes": ["ansi", "human"]}

    def __init__(self, seed: int = 42, max_steps: int = 1000, render_mode: str = "ansi"):
        self.seed_val = seed
        self.max_steps = max_steps
        self.render_mode = render_mode
        self._env = netrust_py.NetRustEnv(seed=seed, max_steps=max_steps)

        if HAS_GYMNASIUM:
            self.action_space = spaces.Discrete(len(ACTION_NAMES))
            self.observation_space = spaces.Dict({
                "player_x": spaces.Box(low=0, high=80, shape=(), dtype=int),
                "player_y": spaces.Box(low=0, high=24, shape=(), dtype=int),
                "player_hp": spaces.Box(low=0, high=999, shape=(), dtype=int),
                "player_max_hp": spaces.Box(low=0, high=999, shape=(), dtype=int),
                "player_ac": spaces.Box(low=-50, high=50, shape=(), dtype=int),
                "depth": spaces.Box(low=1, high=100, shape=(), dtype=int),
                "gold": spaces.Box(low=0, high=1_000_000, shape=(), dtype=int),
                "turn": spaces.Box(low=0, high=10_000_000, shape=(), dtype=int),
                "nutrition": spaces.Box(low=0, high=2000, shape=(), dtype=int),
                "pw": spaces.Box(low=0, high=500, shape=(), dtype=int),
                "is_dead": spaces.Discrete(2),
                "num_visible_actors": spaces.Box(low=0, high=100, shape=(), dtype=int),
            })
        else:
            self.action_space = list(range(len(ACTION_NAMES)))

    def reset(self, seed: Optional[int] = None, options: Optional[Dict[str, Any]] = None) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Reset environment to initial state."""
        if seed is not None:
            self.seed_val = seed
        obs, info = self._env.reset(seed=self.seed_val)
        return obs, info

    def step(self, action: int) -> Tuple[Dict[str, Any], float, bool, bool, Dict[str, Any]]:
        """Execute a step in the simulation."""
        if not (0 <= action < len(ACTION_NAMES)):
            raise ValueError(f"Action {action} out of bounds (0..{len(ACTION_NAMES)-1})")
        obs, reward, terminated, truncated, info = self._env.step(action)
        return obs, reward, terminated, truncated, info

    def render(self) -> Optional[str]:
        """Render the dungeon map."""
        ascii_frame = self._env.render()
        if self.render_mode == "human":
            print(ascii_frame)
            return None
        return ascii_frame

    def get_observation_json(self) -> str:
        """Return the structured observation payload as JSON."""
        return self._env.get_observation_json()
