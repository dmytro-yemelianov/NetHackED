# NetRust Gym - Python Reinforcement Learning Environment

A standard [Gymnasium](https://gymnasium.farama.org/) interface for **NetRust**, the formally verified, high-performance NetHack engine written in Rust.

## Features
- **Blazing Fast**: Native Rust simulation core powered by PyO3 and ABI3 bindings (>100,000 steps/sec).
- **Formal Verification Guarantees**: Core mechanics (Elbereth, Reflection, Containers, Sokoban, Enchantment, Pets) mathematically verified in Lean 4.
- **Gymnasium Standard**: `env.reset()`, `env.step()`, `env.render()` compatible with Stable-Baselines3, CleanRL, and custom PyTorch agents.
- **Multimodal Observations**: Structured numerical telemetry (HP, AC, depth, nutrition, visible actors) plus raw ASCII viewports.

## Installation & Setup

1. Build the Rust extension:
   ```bash
   cargo build -p netrust-py
   # Or install via maturin
   pip install maturin
   maturin develop --manifest-path crates/netrust-py/Cargo.toml
   ```

2. Run the demo agent:
   ```bash
   python3 python/demo_rl.py 42
   ```

## Action Space (Discrete 15)
| ID | Action | Description |
|---|---|---|
| 0..7 | North, East, South, West, NE, SE, SW, NW | Cardinal and diagonal movement / melee attacks |
| 8 | Wait | Pass turn, regenerate energy |
| 9 | Descend | Go down stairs (`>`) |
| 10 | Ascend | Go up stairs (`<`) |
| 11 | PickUp | Pick up items on the ground (`,` / `g`) |
| 12 | Pay | Pay shopkeeper for carried items |
| 13 | Pray | Pray at altar or to god |
| 14 | EngraveElbereth | Dust ward of Elbereth on the ground |
