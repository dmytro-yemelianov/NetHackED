#!/usr/bin/env python3
"""Unit tests for NetRust Gymnasium Environment and PyO3 bindings."""

import unittest
import sys
from pathlib import Path

# Add python directory to path
repo_root = Path(__file__).resolve().parent.parent.parent
python_dir = repo_root / "python"
target_debug = repo_root / "target" / "debug"

sys.path.insert(0, str(python_dir))
try:
    import netrust_py  # noqa: F401  (installed, e.g. via `maturin develop`)
except ImportError:
    if (target_debug / "libnetrust_py.dylib").exists() and not (target_debug / "netrust_py.so").exists():
        import shutil
        shutil.copy(target_debug / "libnetrust_py.dylib", target_debug / "netrust_py.so")
    sys.path.insert(0, str(target_debug))

from netrust_gym import NetRustGymEnv, ACTION_NAMES

class TestNetRustGymEnv(unittest.TestCase):
    def setUp(self):
        self.env = NetRustGymEnv(seed=42, max_steps=50, conduct_masking=True)

    def test_reset_shape_and_keys(self):
        obs, info = self.env.reset()
        self.assertIn("ascii_map", info)
        self.assertNotIn("ascii_map", obs)
        self.assertIn("map_glyphs", obs)
        self.assertEqual(len(obs["map_glyphs"]), 80 * 21)
        self.assertIn("player_hp", obs)
        self.assertGreater(obs["player_hp"], 0)
        self.assertIn("action_mask", info)
        self.assertEqual(len(info["action_mask"]), len(ACTION_NAMES))
        self.assertIn("conducts", info)
        self.assertTrue(info["conducts"]["pacifist"])
        self.assertTrue(info["conducts"]["illiterate"])
        self.assertTrue(info["conducts"]["atheist"])

    def test_step_execution_and_rewards(self):
        obs, info = self.env.reset()
        init_turn = obs["turn"]
        # Step WAIT
        obs, reward, terminated, truncated, info = self.env.step(8)
        self.assertEqual(int(obs["turn"][0]), int(init_turn[0]) + 1)
        self.assertFalse(terminated)
        self.assertIsInstance(reward, float)

    def test_action_masking_conduct_enforcement(self):
        obs, info = self.env.reset()
        masks = self.env.action_masks()
        self.assertEqual(len(masks), len(ACTION_NAMES))
        # With illiterate conduct active, Read (action 21) must be masked out
        self.assertFalse(masks[21], "Read action must be masked out while Illiterate conduct is active")
        # With atheist conduct active, Pray (action 23) must be masked out
        self.assertFalse(masks[23], "Pray action must be masked out while Atheist conduct is active")

    def test_render_ansi(self):
        obs, info = self.env.reset()
        ascii_frame = self.env.render()
        self.assertIsInstance(ascii_frame, str)
        self.assertIn("@", ascii_frame)

    def test_env_checker_passes(self):
        from gymnasium.utils.env_checker import check_env
        check_env(NetRustGymEnv(max_steps=20), skip_render_check=True)

    def test_is_real_gymnasium_env(self):
        import gymnasium
        self.assertIsInstance(self.env, gymnasium.Env)

    def test_reset_without_seed_varies(self):
        env = NetRustGymEnv(max_steps=5)
        seeds = {env.reset()[1]["seed"] for _ in range(5)}
        self.assertGreater(len(seeds), 1)

    def test_reset_with_seed_is_deterministic(self):
        env = NetRustGymEnv(max_steps=5)
        a, _ = env.reset(seed=123)
        b, _ = env.reset(seed=123)
        self.assertTrue((a["map_glyphs"] == b["map_glyphs"]).all())

    def test_masked_sampling_respects_mask(self):
        import numpy as np
        from netrust_gym.env import sample_masked_action
        rng = np.random.default_rng(0)
        mask = [False] * 26
        mask[3] = mask[23] = True
        for _ in range(200):
            self.assertIn(sample_masked_action(mask, rng), (3, 23))

    def test_masked_sampling_all_false_falls_back_to_wait(self):
        import numpy as np
        from netrust_gym.env import sample_masked_action
        self.assertEqual(sample_masked_action([False] * 26, np.random.default_rng(0)), 8)

    def test_explored_count_positive_and_resets(self):
        import netrust_py
        raw = netrust_py.NetRustEnv(seed=7, max_steps=10)
        self.assertGreater(raw.explored_count, 0)
        raw.reset(seed=7)
        n = raw.explored_count
        self.assertGreater(n, 0)
        # Per-level reset on depth change is covered by the Rust unit test
        # `exploration_resets_per_level` in crates/netrust-py/src/lib.rs.

if __name__ == "__main__":
    unittest.main()
