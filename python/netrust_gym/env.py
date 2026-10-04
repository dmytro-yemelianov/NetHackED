"""Gymnasium-compatible Environment for NetRust.

Provides standard step(), reset(), render(), and action_masks() interfaces
for RL algorithms (PPO, MaskablePPO, DQN, A2C, Stable-Baselines3, PettingZoo).
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

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

import gymnasium as gym
import numpy as np
from gymnasium import spaces

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


MAP_CELLS = 80 * 21
WAIT_ACTION = ACTION_NAMES.index("WAIT")

# Numeric observation scalars: (key, low, high). Values are clipped into range.
_SCALARS = [
    ("player_x", 0, 80),
    ("player_y", 0, 24),
    ("player_hp", 0, 9999),
    ("player_max_hp", 0, 9999),
    ("player_ac", -128, 127),
    ("depth", 1, 100),
    ("gold", 0, 100_000_000),
    ("turn", 0, 10_000_000),
    ("nutrition", 0, 5000),
    ("pw", 0, 1000),
    ("max_pw", 0, 1000),
    ("is_dead", 0, 1),
    ("num_visible_actors", 0, 100),
    ("inventory_count", 0, 52),
]
# Everything else the Rust env reports (ascii_map, conducts, ...) goes to `info`.
_NUMERIC_KEYS = {k for k, _, _ in _SCALARS} | {"map_glyphs"}


def sample_masked_action(mask, rng) -> int:
    """Sample uniformly among valid actions; falls back to WAIT if none are valid."""
    valid = np.flatnonzero(np.asarray(mask, dtype=bool))
    if valid.size == 0:
        return WAIT_ACTION
    return int(rng.choice(valid))


class NetRustGymEnv(gym.Env):
    """NetRust Gymnasium environment with action masks and voluntary conducts.

    Observations are numeric arrays only; string / structured data (``ascii_map``,
    ``conducts``, ``action_mask``, ...) is returned in ``info``.
    """

    metadata = {"render_modes": ["ansi"]}

    def __init__(
        self,
        seed: Optional[int] = None,
        max_steps: int = 1000,
        conduct_masking: bool = True,
        render_mode: Optional[str] = None,
    ):
        super().__init__()
        self.max_steps = max_steps
        self.render_mode = render_mode
        self.conduct_masking = conduct_masking
        if seed is not None:
            # Deterministic stream of per-episode seeds for unseeded resets.
            super().reset(seed=seed)
        self._env = netrust_py.NetRustEnv(
            seed=0 if seed is None else seed,
            max_steps=max_steps,
            conduct_masking=conduct_masking,
        )

        self.action_space = spaces.Discrete(len(ACTION_NAMES))
        obs_spaces: Dict[str, spaces.Space] = {
            k: spaces.Box(low=lo, high=hi, shape=(1,), dtype=np.int64)
            for k, lo, hi in _SCALARS
        }
        obs_spaces["map_glyphs"] = spaces.Box(
            low=0, high=255, shape=(MAP_CELLS,), dtype=np.uint8
        )
        self.observation_space = spaces.Dict(obs_spaces)

    def _split(self, raw: Dict[str, Any]) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        obs: Dict[str, Any] = {}
        for key, lo, hi in _SCALARS:
            obs[key] = np.array([min(max(int(raw[key]), lo), hi)], dtype=np.int64)
        glyphs = np.zeros(MAP_CELLS, dtype=np.uint8)
        flat = np.asarray(raw["map_glyphs"], dtype=np.int64)[:MAP_CELLS]
        glyphs[: flat.size] = np.clip(flat, 0, 255)
        obs["map_glyphs"] = glyphs
        extra = {k: v for k, v in raw.items() if k not in _NUMERIC_KEYS}
        return obs, extra

    def reset(
        self, seed: Optional[int] = None, options: Optional[Dict[str, Any]] = None
    ) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Reset; uses ``seed`` if given, otherwise draws a fresh one from the env RNG."""
        super().reset(seed=seed)
        if seed is None:
            seed = int(self.np_random.integers(0, 2**31 - 1))
        raw, info = self._env.reset(seed=seed)
        obs, extra = self._split(raw)
        info = dict(info)
        info.update(extra)
        info["seed"] = seed
        return obs, info

    def step(
        self, action: int
    ) -> Tuple[Dict[str, Any], float, bool, bool, Dict[str, Any]]:
        """Execute a step in the simulation."""
        action = int(action)
        if not (0 <= action < len(ACTION_NAMES)):
            raise ValueError(
                f"Action {action} out of bounds (0..{len(ACTION_NAMES)-1})"
            )
        raw, reward, terminated, truncated, info = self._env.step(action)
        obs, extra = self._split(raw)
        info = dict(info)
        info.update(extra)
        return obs, float(reward), bool(terminated), bool(truncated), info

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

    def render(self) -> str:
        """Return the dungeon map as an ASCII string."""
        return self._env.render()

    def get_observation_json(self) -> str:
        """Return the structured observation payload as JSON."""
        return self._env.get_observation_json()
