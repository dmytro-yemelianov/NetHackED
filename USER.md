# USER.md - User Model

Store stable user preferences and profile facts as directives that can guide future sessions.

Use one directive per entry:

```md
<!-- observed: YYYY-MM-DD | status: active -->

- Prefer concise progress updates during implementation work.
```

## Directives

<!-- observed: 2026-10-04 | status: active -->
- Always use `uv` and the local virtual environment (`.venv`) for Python tooling, NVIDIA NAT, and related dependencies.

<!-- observed: 2026-10-04 | status: active -->
- Always strictly maintain the Rule Packs P1 constraints and verify with golden determinism tests (`EXPECTED_RUNS`, `EXPECTED_LOGS`, and bestiary combat).

<!-- observed: 2026-10-04 | status: active -->
- Prefer using NeMo Agent Toolkit (`nat`) workflows and domain tools (`nethacked_cargo_runner`, `nethacked_golden_verifier`, `nethacked_rulepacks_auditor`) for verification, invariant auditing, and reviews.
