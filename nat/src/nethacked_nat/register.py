"""Registration entry point for nethacked_nat components.

Importing this module triggers registration with the NAT global type registry.
"""

from nethacked_nat.tools import (
    cargo_runner_function,
    golden_verifier_function,
    rulepacks_auditor_function,
    workspace_reader_function,
)

__all__ = [
    "workspace_reader_function",
    "cargo_runner_function",
    "golden_verifier_function",
    "rulepacks_auditor_function",
]
