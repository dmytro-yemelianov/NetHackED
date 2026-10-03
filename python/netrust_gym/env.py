"""Gymnasium-compatible Environment for NetRust.

Provides standard step(), reset(), render(), and action_masks() interfaces
for RL algorithms (PPO, MaskablePPO, DQN, A2C, Stable-Baselines3, PettingZoo).
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, Dict, Tuple, Optional, List

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
    "MOVE_N",
    "MOVE_E",
    "MOVE_S",
    "MOVE_W",
    "MOVE_NE",
    "MOVE_SE",
    "MOVE_SW",
    "MOVE_NW",
    "WAIT",
    "DESCEND",
    "ASCEND",
    "PICKUP",
    "SEARCH",
    "UNTRAP",
    "FIRE_N",
    "FIRE_E",
    "FIRE_S",
    "FIRE_W",
    "QUIVER",
    "EAT",
    "QUAFF",
    "READ",
    "ZAP_WAND",
    "PRAY",
    "PAY",
    "ENGRAVE_ELBERETH",
]


class NetRustGymEnv:
    """NetRust Gymnasium Environment wrapper supporting action masks and voluntary conducts."""

    metadata = {"render_modes": ["ansi", "human"]}

    def __init__(
        self,
        seed: int = 42,
        max_steps: int = 1000,
        render_mode: str = "ansi",
        conduct_masking: bool = True,
    ):
        self.seed_val = seed
        self.max_steps = max_steps
        self.render_mode = render_mode
        self.conduct_masking = conduct_masking
        self._env = netrust_py.NetRustEnv(
            seed=seed, max_steps=max_steps, conduct_masking=conduct_masking
        )

        if HAS_GYMNASIUM:
            self.action_space = spaces.Discrete(len(ACTION_NAMES))
            self.observation_space = spaces.Dict({
                "player_x": spaces.Box(low=0, high=80, shape=(), dtype=int),
                "player_y": spaces.Box(low=0, high=24, shape=(), dtype=int),
                "player_hp": spaces.Box(low=0, high=9999, shape=(), dtype=int),
                "player_max_hp": spaces.Box(low=0, high=9999, shape=(), dtype=int),
                "player_ac": spaces.Box(low=-128, high=127, shape=(), dtype=int),
                "depth": spaces.Box(low=1, high=100, shape=(), dtype=int),
                "gold": spaces.Box(low=0, high=100_000_000, shape=(), dtype=int),
                "turn": spaces.Box(low=0, high=10_000_000, shape=(), dtype=int),
                "nutrition": spaces.Box(low=0, high=5000, shape=(), dtype=int),
                "pw": spaces.Box(low=0, high=1000, shape=(), dtype=int),
                "max_pw": spaces.Box(low=0, high=1000, shape=(), dtype=int),
                "is_dead": spaces.Discrete(2),
                "num_visible_actors": spaces.Box(low=0, high=100, shape=(), dtype=int),
                "inventory_count": spaces.Box(low=0, high=52, shape=(), dtype=int),
                "map_glyphs": spaces.Box(low=0, high=255, shape=(1680,), dtype=int),
            })
        else:
            self.action_space = list(range(len(ACTION_NAMES)))

    def reset(
        self, seed: Optional[int] = None, options: Optional[Dict[str, Any]] = None
    ) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Reset environment to initial state."""
        if seed is not None:
            self.seed_val = seed
        obs, info = self._env.reset(seed=self.seed_val)
        return obs, info

    def step(
        self, action: int
    ) -> Tuple[Dict[str, Any], float, bool, bool, Dict[str, Any]]:
        """Execute a step in the simulation."""
        if not (0 <= action < len(ACTION_NAMES)):
            raise ValueError(
                f"Action {action} out of bounds (0..{len(ACTION_NAMES)-1})"
            )
        obs, reward, terminated, truncated, info = self._env.step(action)
        return obs, reward, terminated, truncated, info

    def action_masks(self) -> List[bool]:
        """Return boolean mask of valid actions for the current state."""
        return self._env.get_action_mask()

    def get_conducts(self) -> Dict[str, bool]:
        """Return active NetHack voluntary conducts."""
        return self._env.get_active_conducts()

    def set_conduct_masking(self, enabled: bool) -> None:
        """Enable or disable strict voluntary conduct filtering in action masks."""
        self.conduct_masking = enabled
        self._env.set_conduct_masking(enabled)

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
