# NetHackED Gym - Python Reinforcement Learning Environment

A standard [Gymnasium](https://gymnasium.farama.org/) interface for **NetHackED**, a high-performance Rust reimplementation of NetHack mechanics written in Rust.

## Features
- **Blazing Fast**: Native Rust simulation core powered by PyO3 and ABI3 bindings (>100,000 steps/sec).
- **Lean 4 Models**: Core mechanics (Elbereth, Reflection, Containers, Sokoban, Enchantment, Pets) modeled in Lean 4 (the Rust engine is tested against those models, not formally linked).
- **Gymnasium Standard**: `env.reset()`, `env.step()`, `env.render()` compatible with Stable-Baselines3, CleanRL, and custom PyTorch agents.
- **Multimodal Observations**: Structured numerical telemetry (HP, AC, depth, nutrition, visible actors) plus raw ASCII viewports.

## Installation & Setup

1. Build the Rust extension:
   ```bash
   cargo build -p nethacked-py
   # Or install via maturin
   pip install maturin
   maturin develop --manifest-path crates/nethacked-py/Cargo.toml
   ```

2. Run the demo agent:
   ```bash
   python3 python/demo_rl.py 42
   ```

## Action Space (Discrete 26)
`nethacked_gym.ACTION_NAMES` lists all 26 actions: 0-7 compass moves, 8 WAIT, 9 DESCEND, 10 ASCEND, 11 PICKUP, 12 SEARCH, 13 UNTRAP, 14-17 FIRE (N/E/S/W), 18 QUIVER, 19 EAT, 20 QUAFF, 21 READ, 22 ZAP_WAND, 23 PRAY, 24 PAY, 25 ENGRAVE_ELBERETH.
`env.action_masks()` returns the currently valid actions (including voluntary-conduct filtering).

## Observations
`observation_space` is a `gymnasium.spaces.Dict` of numeric arrays only (`map_glyphs`, `player_hp`, `depth`, `gold`, `turn`, ...). Non-numeric data (`ascii_map`, `conducts`, `afflictions`, `action_mask`, `visible_actors`, `inventory`) is returned in `info`.
`reset()` without a seed draws a fresh seed from the env RNG (reported as `info["seed"]`); `reset(seed=n)` is deterministic.

## Training
`python/train_reinforce.py` is a small dependency-free REINFORCE policy-gradient trainer (not PPO) that samples from the action mask and exports `web/policy_weights.json`.
