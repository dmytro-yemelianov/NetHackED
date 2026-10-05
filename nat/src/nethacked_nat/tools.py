"""Custom tools for NetHackED development, verification, and rule-packs workflow.

These tools integrate with NVIDIA NeMo Agent Toolkit (NAT) and provide
declarative agents with capabilities to inspect the codebase, run Cargo and Lean
toolchains, verify golden determinism fingerprints, and audit rule pack constraints.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import subprocess
from pathlib import Path
from typing import AsyncGenerator

from pydantic import Field

from nat.builder.builder import Builder
from nat.builder.framework_enum import LLMFrameworkEnum
from nat.builder.function_info import FunctionInfo
from nat.cli.register_workflow import register_function
from nat.data_models.function import FunctionBaseConfig

logger = logging.getLogger(__name__)

# Repository root (parent of nat/)
REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent


def _resolve_safe_path(rel_path: str) -> Path:
    """Resolve a relative path ensuring it stays within REPO_ROOT."""
    clean = (REPO_ROOT / rel_path).resolve()
    if not str(clean).startswith(str(REPO_ROOT)):
        raise ValueError(f"Access denied: path '{rel_path}' is outside repository root.")
    return clean


# ---------------------------------------------------------------------------
# 1. Workspace Reader Tool
# ---------------------------------------------------------------------------

class WorkspaceReaderConfig(FunctionBaseConfig, name="nethacked_workspace_reader"):
    """Tool to read, list, and search files safely within the NetHackED repository."""
    max_read_lines: int = Field(default=500, description="Maximum number of lines returned in read mode.")


@register_function(config_type=WorkspaceReaderConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def workspace_reader_function(config: WorkspaceReaderConfig, builder: Builder) -> AsyncGenerator[FunctionInfo, None]:
    async def _workspace_reader(action: str, target: str = "", query: str = "", start_line: int = 1, end_line: int = 0) -> str:
        """Inspect the NetHackED repository workspace.

        Args:
            action: One of 'read' (read file lines), 'list' (list directory), 'search' (grep file contents), 'stat' (file size/status).
            target: Relative path inside the repository (e.g. 'crates/nethacked-data/src/ruleset.rs', 'Cargo.toml').
            query: Search query for 'search' action.
            start_line: 1-indexed starting line number when action is 'read'.
            end_line: 1-indexed ending line number when action is 'read' (0 reads up to max_read_lines).

        Returns:
            Formatted string of directory contents, file lines with numbers, or search matches.
        """
        try:
            target_path = _resolve_safe_path(target) if target else REPO_ROOT
            action_lower = action.strip().lower()

            if action_lower == "list":
                if not target_path.exists():
                    return f"Error: Path '{target}' does not exist."
                if not target_path.is_dir():
                    return f"Error: Path '{target}' is not a directory."
                entries = sorted(target_path.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower()))
                lines = [f"Directory listing of: {target_path.relative_to(REPO_ROOT)}"]
                for entry in entries:
                    if entry.name.startswith(".git") and entry.name != ".gitignore":
                        continue
                    if entry.name in (".venv", "target", ".nat"):
                        continue
                    suffix = "/" if entry.is_dir() else ""
                    lines.append(f"  {entry.name}{suffix}")
                return "\n".join(lines)

            elif action_lower == "read":
                if not target_path.is_file():
                    return f"Error: File '{target}' does not exist or is a directory."
                try:
                    content = target_path.read_text(encoding="utf-8", errors="replace")
                except Exception as exc:
                    return f"Error reading file '{target}': {exc}"

                file_lines = content.splitlines()
                total_lines = len(file_lines)
                s_idx = max(1, start_line)
                e_idx = end_line if end_line > 0 else (s_idx + config.max_read_lines - 1)
                e_idx = min(total_lines, e_idx)

                output = [f"File: {target_path.relative_to(REPO_ROOT)} (Lines {s_idx}..{e_idx} of {total_lines})"]
                for idx in range(s_idx - 1, e_idx):
                    output.append(f"{idx + 1:5d} | {file_lines[idx]}")
                return "\n".join(output)

            elif action_lower == "search":
                if not query:
                    return "Error: 'query' parameter is required for action='search'."
                cmd = ["grep", "-rn", "--exclude-dir=.git", "--exclude-dir=target", "--exclude-dir=.venv", "--exclude-dir=.nat", query]
                search_target = str(target_path) if target else "."
                cmd.append(search_target)
                proc = await asyncio.create_subprocess_exec(
                    *cmd,
                    cwd=str(REPO_ROOT),
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.PIPE
                )
                stdout, stderr = await proc.communicate()
                matches = stdout.decode("utf-8", errors="replace").splitlines()
                if not matches:
                    return f"No matches found for query '{query}' in '{target or '.'}'."
                return f"Found {len(matches)} match(es):\n" + "\n".join(matches[:40])

            elif action_lower == "stat":
                if not target_path.exists():
                    return f"Path '{target}' does not exist."
                st = target_path.stat()
                return (
                    f"Path: {target_path.relative_to(REPO_ROOT)}\n"
                    f"Type: {'Directory' if target_path.is_dir() else 'File'}\n"
                    f"Size: {st.st_size} bytes\n"
                    f"Modified: {st.st_mtime}"
                )

            return f"Error: Unknown action '{action}'. Choose from: list, read, search, stat."

        except Exception as exc:
            return f"Exception in workspace_reader: {exc}"

    yield FunctionInfo.from_fn(_workspace_reader, description="Safely lists, reads, stats, and greps files in the NetHackED repository.")


# ---------------------------------------------------------------------------
# 2. Cargo & Toolchain Runner Tool
# ---------------------------------------------------------------------------

class CargoRunnerConfig(FunctionBaseConfig, name="nethacked_cargo_runner"):
    """Tool to execute Cargo and Lean gates for NetHackED."""
    timeout_seconds: int = Field(default=300, description="Command execution timeout in seconds.")


@register_function(config_type=CargoRunnerConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def cargo_runner_function(config: CargoRunnerConfig, builder: Builder) -> AsyncGenerator[FunctionInfo, None]:
    async def _cargo_runner(command: str = "all_gates", extra_args: str = "") -> str:
        """Run toolchain verification commands on NetHackED.

        Args:
            command: One of 'all_gates' (run all standard PR verification gates),
                     'test' (cargo test --workspace --exclude nethacked-py),
                     'clippy' (cargo clippy --workspace --all-targets --locked -- -D warnings),
                     'fmt' (cargo fmt --all --check),
                     'wasm' (cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked),
                     'lean' (lake build),
                     'doc_links' (bash scripts/check-doc-links.sh).
            extra_args: Additional command line arguments (e.g. '--test ruleset_vanilla').

        Returns:
            Command stdout/stderr output and exit code.
        """
        cmd_map = {
            "fmt": "cargo fmt --all --check",
            "clippy": "cargo clippy --workspace --all-targets --locked -- -D warnings",
            "test": "cargo test --workspace --exclude nethacked-py --locked",
            "wasm": "cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked",
            "nethacked_py": "cargo check -p nethacked-py",
            "doc_links": "bash scripts/check-doc-links.sh",
            "lean": "lake build",
            "all_gates": (
                "cargo fmt --all --check && "
                "cargo clippy --workspace --all-targets --locked -- -D warnings && "
                "cargo test --workspace --exclude nethacked-py --locked && "
                "cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked && "
                "cargo check -p nethacked-py && "
                "bash scripts/check-doc-links.sh && "
                "lake build"
            ),
        }

        full_cmd = cmd_map.get(command.strip().lower())
        if not full_cmd:
            return f"Error: Unknown command '{command}'. Supported: {', '.join(cmd_map.keys())}"

        if extra_args:
            full_cmd = f"{full_cmd} {extra_args}"

        try:
            proc = await asyncio.create_subprocess_shell(
                full_cmd,
                cwd=str(REPO_ROOT),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            try:
                stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=config.timeout_seconds)
            except asyncio.TimeoutError:
                proc.kill()
                return f"Command timed out after {config.timeout_seconds} seconds."

            out = stdout.decode("utf-8", errors="replace")
            err = stderr.decode("utf-8", errors="replace")
            status = "SUCCESS" if proc.returncode == 0 else f"FAILED (exit code {proc.returncode})"

            # Truncate large output
            combined = f"=== STATUS: {status} ===\n"
            if out:
                combined += f"--- STDOUT ---\n{out[-3000:] if len(out) > 3000 else out}\n"
            if err:
                combined += f"--- STDERR ---\n{err[-3000:] if len(err) > 3000 else err}\n"
            return combined

        except Exception as exc:
            return f"Failed to execute cargo command: {exc}"

    yield FunctionInfo.from_fn(_cargo_runner, description="Executes cargo and toolchain checks (fmt, clippy, test, wasm, doc_links, lean, all_gates).")


# ---------------------------------------------------------------------------
# 3. Golden Determinism Verifier Tool
# ---------------------------------------------------------------------------

class GoldenVerifierConfig(FunctionBaseConfig, name="nethacked_golden_verifier"):
    """Tool to verify the frozen golden determinism fingerprints."""
    timeout_seconds: int = Field(default=180, description="Verification timeout.")


@register_function(config_type=GoldenVerifierConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def golden_verifier_function(config: GoldenVerifierConfig, builder: Builder) -> AsyncGenerator[FunctionInfo, None]:
    async def _golden_verifier(release_mode: bool = False) -> str:
        """Run the golden determinism tests to verify vanilla gameplay invariants are unchanged.

        Args:
            release_mode: If True, tests with '--release' flag.

        Returns:
            Result of the golden determinism test suite.
        """
        cmd = "cargo test -p nethacked-agent --test golden_determinism"
        if release_mode:
            cmd += " --release"

        try:
            proc = await asyncio.create_subprocess_shell(
                cmd,
                cwd=str(REPO_ROOT),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=config.timeout_seconds)
            out = stdout.decode("utf-8", errors="replace")
            err = stderr.decode("utf-8", errors="replace")
            status = "PASSED (Deterministic fingerprints intact)" if proc.returncode == 0 else f"FAILED (Determinism broken, exit {proc.returncode})"
            return f"=== GOLDEN DETERMINISM: {status} ===\n{out}\n{err}"
        except Exception as exc:
            return f"Failed to run golden determinism test: {exc}"

    yield FunctionInfo.from_fn(_golden_verifier, description="Verifies that vanilla gameplay fingerprints match frozen Task 1 constants.")


# ---------------------------------------------------------------------------
# 4. Rule Packs Auditor Tool
# ---------------------------------------------------------------------------

class RulePacksAuditorConfig(FunctionBaseConfig, name="nethacked_rulepacks_auditor"):
    """Tool to audit rule pack constraints and progress."""


@register_function(config_type=RulePacksAuditorConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def rulepacks_auditor_function(config: RulePacksAuditorConfig, builder: Builder) -> AsyncGenerator[FunctionInfo, None]:
    async def _rulepacks_auditor(check: str = "all") -> str:
        """Audit constraints for Rule Packs P1 implementation.

        Args:
            check: One of 'progress' (reads SDD progress log),
                   'wasm_deps' (verifies wasm has no toml/clap/schemars/nethacked-pack),
                   'static_tables' (checks for forbidden static table access outside nethacked-data),
                   'all' (runs all auditor checks).

        Returns:
            Audit report.
        """
        results = []

        if check in ("all", "progress"):
            prog_path = REPO_ROOT / ".superpowers/sdd/2026-10-04-rule-packs-p1/progress.md"
            if prog_path.exists():
                results.append(f"--- SDD Progress Log ---\n{prog_path.read_text(encoding='utf-8')}")
            else:
                results.append("Progress log not found.")

        if check in ("all", "wasm_deps"):
            cmd = "cargo tree -p nethacked-wasm -e normal"
            proc = await asyncio.create_subprocess_shell(
                cmd, cwd=str(REPO_ROOT), stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
            )
            stdout, _ = await proc.communicate()
            tree = stdout.decode("utf-8", errors="replace")
            import re
            forbidden = [pkg for pkg in ["toml", "clap", "schemars", "nethacked-pack"] if re.search(rf"\b{re.escape(pkg)} v", tree)]
            if forbidden:
                results.append(f"--- WASM Hygiene: VIOLATION! Found forbidden dependencies: {forbidden} ---")
            else:
                results.append("--- WASM Hygiene: CLEAN (no toml, clap, schemars, nethacked-pack in wasm graph) ---")

        if check in ("all", "static_tables"):
            script_path = REPO_ROOT / "scripts/check-no-static-tables.sh"
            if script_path.exists():
                proc = await asyncio.create_subprocess_shell(
                    "bash scripts/check-no-static-tables.sh",
                    cwd=str(REPO_ROOT), stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
                )
                stdout, stderr = await proc.communicate()
                out = (stdout + stderr).decode("utf-8", errors="replace")
                results.append(f"--- Static Tables Check ---\n{out}")
            else:
                results.append("--- Static Tables Check: scripts/check-no-static-tables.sh not yet created (scheduled in Task 4) ---")

        return "\n\n".join(results)

    yield FunctionInfo.from_fn(_rulepacks_auditor, description="Audits rule pack constraints, progress ledger, and wasm dependency hygiene.")
