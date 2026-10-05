# AGENTS.md - NetRust Workspace Conventions

Keep workspace conventions here.

## Session Startup
- Use runtime-provided startup context first.
- Directives are stored in `USER.md`.
- Durable facts and decisions are stored in `MEMORY.md`.

## Task Execution
- Follow Subagent-Driven Development / Executing Plans workflow.
- Update `.superpowers/sdd/2026-10-04-rule-packs-p1/progress.md` before and after each task.
- Strictly adhere to `global-constraints.md`.
- Always run the full gate checks and golden determinism tests before committing.
