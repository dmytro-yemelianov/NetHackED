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
        self.assertIn("ascii_map", obs)
        self.assertIn("map_glyphs", obs)
        self.assertEqual(len(obs["map_glyphs"]), 80 * 21)
        self.assertIn("player_hp", obs)
        self.assertGreater(obs["player_hp"], 0)
        self.assertIn("action_mask", obs)
        self.assertEqual(len(obs["action_mask"]), len(ACTION_NAMES))
        self.assertIn("conducts", obs)
        self.assertTrue(obs["conducts"]["pacifist"])
        self.assertTrue(obs["conducts"]["illiterate"])
        self.assertTrue(obs["conducts"]["atheist"])

    def test_step_execution_and_rewards(self):
        obs, info = self.env.reset()
        init_turn = obs["turn"]
        # Step WAIT
        obs, reward, terminated, truncated, info = self.env.step(8)
        self.assertEqual(obs["turn"], init_turn + 1)
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

if __name__ == "__main__":
    unittest.main()
