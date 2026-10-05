# NVIDIA NeMo Agent Toolkit (NAT) Configuration for NetHackED

Production-grade agent orchestration setup using [NVIDIA NeMo Agent Toolkit](https://github.com/NVIDIA/NeMo-Agent-Toolkit) (`nvidia-nat`) to automate verification, architectural reviews, golden determinism auditing, and rule packs implementation across the **NetHackED** repository.

---

## Registered Custom Tools

The `nethacked-nat` package registers 4 domain-specific tools into NAT's component registry:

| Tool Name | Type | Purpose |
|-----------|------|---------|
| `nethacked_workspace_reader` | Function / LangChain Tool | Safely lists, reads, stats, and greps files within the repository with line numbers. |
| `nethacked_cargo_runner` | Function / LangChain Tool | Executes `cargo fmt`, `cargo clippy`, `cargo test`, `cargo build -p nethacked-wasm`, `lake build`, or `all_gates`. |
| `nethacked_golden_verifier` | Function / LangChain Tool | Runs `cargo test -p nethacked-agent --test golden_determinism` to verify vanilla gameplay invariants. |
| `nethacked_rulepacks_auditor` | Function / LangChain Tool | Audits rule packs constraints, wasm dependency hygiene, and SDD progress log. |

---

## Workflow Configurations

All configurations are validated with `nat validate`:

- `nat/configs/config.yml`: Primary ReAct agent workflow using NVIDIA NIM (`meta/llama-3.2-90b-vision-instruct`).
- `nat/configs/review_config.yml`: Low-temperature architectural and invariant reviewer.
- `nat/configs/dev_config.yml`: Implementation and bugfixing assistant.
- `nat/configs/openai_config.yml`: OpenAI backend alternative.

---

## Quick Start & Usage

```bash
# Activate virtual environment
source .venv/bin/activate

# Validate configurations
nat validate --config_file nat/configs/config.yml

# Run verification queries
nat run --config_file nat/configs/config.yml \
  --input "Verify golden determinism and report results."
```
