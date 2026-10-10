"""Focused local tests for PROCESS-GUARDS; no model invocation."""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / "parity-agent-guard.py"
SPEC = importlib.util.spec_from_file_location("parity_agent_guard", SOURCE)
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)


class GuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.dir = Path(self.temp.name)
        self.repo = self.dir / "repo"
        self.repo.mkdir()
        (self.repo / ".git").mkdir()
        self.prompt = self.dir / "prompt.txt"
        self.prompt.write_text("Do the bounded task.\n", encoding="utf-8")
        self.schema = self.dir / "schema.json"
        self.schema.write_text('{"type":"object","properties":{}}\n', encoding="utf-8")
        self.run = self.dir / "run"
        self.packet_file = self.dir / "packet.json"
        self.packet = {
            "unit": "PROCESS-GUARDS", "phase": "normal", "phase_index": 1,
            "expected_head": "abc123", "repo": str(self.repo), "run_directory": str(self.run),
            "model": "gpt-6.1-sol", "effort": "medium", "rationale": "bounded source task",
            "prompt": str(self.prompt), "output_schema": str(self.schema),
            "bindings": [self.bind(self.prompt), self.bind(self.schema)], "allowlist": ["scripts/a.py"],
            "prerequisites": [], "policies": ["one writer"], "focused_checks": ["unittest"],
            "final_checks": ["full gates"], "retry": {"attempt": 0, "maximum": 1},
            "ceilings": {"input_tokens": 30000, "output_tokens": 8000},
            "prior_phases": [], "root_usage": "known", "adjustment": None,
            "task": str(self.prompt), "execution": {"backend": "app-server", "max_wall_seconds": 600,
                "max_commands": 24, "telemetry_deadline_seconds": 120}}
        self.save()
        self.head = patch.object(guard.subprocess, "run", side_effect=self.fake_run)
        self.head.start()
        self.addCleanup(self.head.stop)
        self.tokens = patch.object(guard, "prompt_tokens", return_value=5)
        self.tokens.start()
        self.addCleanup(self.tokens.stop)
        self.live_original = guard.run_live
        self.live = patch.object(guard, "run_live", side_effect=self.fake_live)
        self.live.start()
        self.addCleanup(self.live.stop)

    @staticmethod
    def bind(path):
        return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

    def save(self):
        self.packet_file.write_text(json.dumps(self.packet), encoding="utf-8")

    def fake_run(self, argv, **kwargs):
        if argv[0] == "git":
            return type("Result", (), {"returncode": 0, "stdout": "abc123\n"})()
        kwargs["stdout"].write(b'{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":4,"reasoning_output_tokens":1}}\n')
        return type("Result", (), {"returncode": 0})()

    def fake_live(self, packet, run):
        (run / "events.jsonl").write_text('{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":4,"reasoning_output_tokens":1}}\n')
        (run / "control.json").write_text('{"stop_reason":null,"usage":{}}')
        return {"stop_reason": None, "runner_error": None}

    def test_preflight_rejects_missing_oversize_changed_and_stale(self):
        self.assertEqual(guard.validate(self.packet_file)[1], 5)
        del self.packet["policies"]
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "mandatory"):
            guard.validate(self.packet_file)
        self.packet["policies"] = ["one writer"]
        self.save()
        with patch.object(guard, "prompt_tokens", return_value=1001):
            with self.assertRaisesRegex(guard.GuardError, "exceeds"):
                guard.validate(self.packet_file)
        self.prompt.write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(guard.GuardError, "binding changed"):
            guard.validate(self.packet_file)
        self.prompt.write_text("Do the bounded task.\n", encoding="utf-8")
        self.packet["expected_head"] = "stale"
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "stale HEAD"):
            guard.validate(self.packet_file)

    def test_bound_inline_source_keeps_instruction_limit_and_archives_exact_bytes(self):
        source = self.dir / "source.txt"
        source.write_text("def changed(): return 1\n", encoding="utf-8")
        self.packet["source_packet"] = str(source)
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "direct binding"):
            guard.validate(self.packet_file)
        self.packet["bindings"].append(self.bind(source))
        self.save()
        with patch.object(guard, "prompt_tokens", side_effect=lambda path: 20001 if str(path) == str(source) else 5):
            with self.assertRaisesRegex(guard.GuardError, "bounded initial context"):
                guard.validate(self.packet_file)
        self.assertEqual(guard.launch(self.packet_file), 0)
        self.assertEqual((self.run / "source-packet.txt").read_bytes(), source.read_bytes())
        self.assertEqual(json.loads((self.run / "runner.json").read_text())["source_estimate"], 5)

    def test_malformed_routing_and_unknown_root_need_adjustment(self):
        self.packet["phase"] = []
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "phase routing"):
            guard.validate(self.packet_file)
        self.packet["phase"] = "normal"
        self.packet["root_usage"] = "unknown"
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "adjustment"):
            guard.validate(self.packet_file)
        self.packet["adjustment"] = "Narrow source context to this helper and split review into its own phase."
        self.save()
        guard.validate(self.packet_file)

    def test_prompt_and_schema_must_be_direct_bindings(self):
        self.packet["prerequisites"] = [self.packet["bindings"].pop(0)]
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "prompt and schema"):
            guard.validate(self.packet_file)

    def test_launch_controls_and_primary_task_are_mandatory_and_bounded(self):
        for execution in (None, {}, dict(self.packet["execution"], max_wall_seconds=601),
                          dict(self.packet["execution"], max_commands=True),
                          dict(self.packet["execution"], telemetry_deadline_seconds=181)):
            with self.subTest(execution=execution):
                original = self.packet["execution"]
                self.packet["execution"] = execution
                self.save()
                with self.assertRaises(guard.GuardError):
                    guard.validate(self.packet_file)
                self.packet["execution"] = original
        self.packet["task"] = str(self.dir / "unbound-task.txt")
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "primary task"):
            guard.validate(self.packet_file)

    def test_primary_task_counts_toward_context_limit(self):
        task = self.dir / "task.txt"
        task.write_text("large task")
        self.packet["task"] = str(task)
        self.packet["bindings"].append(self.bind(task))
        self.save()
        with patch.object(guard, "prompt_tokens", return_value=600):
            with self.assertRaisesRegex(guard.GuardError, "exceeds"):
                guard.validate(self.packet_file)

    def test_launch_snapshots_argv_and_replay(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(guard.launch(self.packet_file), 0)
        self.assertEqual((self.run / "prompt.txt").read_bytes(), self.prompt.read_bytes())
        self.assertEqual((self.run / "schema.json").read_bytes(), self.schema.read_bytes())
        self.assertEqual(json.loads((self.run / "runner.json").read_text())["exit_code"], 0)
        packet, run = guard.run_live.call_args.args
        self.assertEqual(packet["execution"]["backend"], "app-server")
        self.assertEqual(run, self.run)
        with self.assertRaisesRegex(guard.GuardError, "duplicate started run"):
            guard.validate(self.packet_file)

    def test_launch_snapshots_validated_packet_not_later_file_bytes(self):
        original_validate = guard.validate
        def changed_after_validation(path):
            result = original_validate(path)
            self.packet_file.write_text('{"tampered":true}', encoding="utf-8")
            return result
        with patch.object(guard, "validate", side_effect=changed_after_validation):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(guard.launch(self.packet_file), 0)
        self.assertEqual(json.loads((self.run / "packet.json").read_text())["unit"], "PROCESS-GUARDS")

    def test_real_local_fake_codex_receives_prompt_and_writes_events(self):
        fake = self.dir / "codex"
        server = Path(__file__).with_name("fake_codex_app_server.py")
        fake.write_text(f'#!{sys.executable}\nexec(compile(open({str(server)!r}).read(), {str(server)!r}, "exec"))\n', encoding="utf-8")
        fake.chmod(0o755)
        def git_or_real(argv, **kwargs):
            if argv[0] == "git":
                return self.fake_run(argv, **kwargs)
            process = subprocess.Popen(argv, stdin=kwargs["stdin"], stdout=kwargs["stdout"], stderr=kwargs["stderr"])
            return subprocess.CompletedProcess(argv, process.wait())
        env = {"PATH": str(self.dir) + os.pathsep + os.environ.get("PATH", ""),
               "FAKE_STDIN": str(self.dir / "received-prompt.txt")}
        with patch.dict(os.environ, env), patch.object(guard.subprocess, "run", side_effect=git_or_real), patch.object(guard, "run_live", wraps=self.live_original):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(guard.launch(self.packet_file), 0)
        self.assertEqual((self.dir / "received-prompt.txt").read_text(), self.prompt.read_text())
        self.assertEqual(self.usage_result()[1]["status"], "pass")

    def test_runner_start_failure_is_persisted_and_replay_rejected(self):
        def unavailable(argv, **kwargs):
            if argv[0] == "git":
                return self.fake_run(argv, **kwargs)
            raise FileNotFoundError("codex")
        with patch.object(guard, "run_live", side_effect=FileNotFoundError("codex")):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(guard.launch(self.packet_file), 1)
        self.assertEqual(json.loads((self.run / "runner.json").read_text())["runner_error"], "FileNotFoundError")
        with self.assertRaisesRegex(guard.GuardError, "duplicate started run"):
            guard.validate(self.packet_file)

    def write_run(self, events, exit_code=0):
        self.run.mkdir()
        (self.run / "packet.json").write_text(json.dumps(self.packet), encoding="utf-8")
        (self.run / "prompt.txt").write_bytes(self.prompt.read_bytes())
        (self.run / "schema.json").write_bytes(self.schema.read_bytes())
        (self.run / "runner.json").write_text(json.dumps({"exit_code": exit_code, "runner_error": None, "unit": self.packet["unit"], "phase_index": self.packet["phase_index"], "prompt_estimate": 5, "model": self.packet["model"], "effort": "medium", "wall_seconds": 1, "bindings": self.packet["bindings"]}), encoding="utf-8")
        # Historical usage fixtures intentionally exercise the exec report
        # format; new launch/controller integrity has separate tests.
        saved = json.loads((self.run / "packet.json").read_text())
        saved.pop("execution", None)
        (self.run / "packet.json").write_text(json.dumps(saved))
        (self.run / "events.jsonl").write_text(events, encoding="utf-8")

    def usage_result(self):
        with contextlib.redirect_stdout(io.StringIO()):
            return guard.usage(self.run), json.loads((self.run / "usage.json").read_text())

    def test_bound_control_must_agree_with_successful_runner_and_events(self):
        values = {"input_tokens": 10, "cached_input_tokens": 2,
                  "output_tokens": 4, "reasoning_output_tokens": 1}
        self.write_run(json.dumps({"type": "turn.completed", "usage": values}) + "\n")
        (self.run / "packet.json").write_text(json.dumps(self.packet))
        (self.run / "task.txt").write_bytes(self.prompt.read_bytes())
        runner = json.loads((self.run / "runner.json").read_text())
        good = {"stop_reason": None, "completed": True, "interrupt_sent": False,
                "runner_error": None, "thread_id": "t", "turn_id": "u",
                "commands": 0, "usage": dict(values, uncached_input_tokens=8)}
        cases = [dict(good, stop_reason="protocol_error", completed=False),
                 dict(good, completed=False), dict(good, interrupt_sent=True),
                 dict(good, runner_error="ProtocolError"), dict(good, thread_id=""),
                 dict(good, commands=True), dict(good, commands=24),
                 dict(good, usage=dict(good["usage"], uncached_input_tokens=9)),
                 {k: v for k, v in good.items() if k != "stop_reason"}]
        for control in [good] + cases:
            with self.subTest(control=control):
                (self.run / "control.json").write_text(json.dumps(control))
                runner["control_sha256"] = guard.sha(self.run / "control.json")
                (self.run / "runner.json").write_text(json.dumps(runner))
                code, report = self.usage_result()
                self.assertEqual(report["status"], "pass" if control == good else "failed")
                self.assertEqual(code, 0 if control == good else 1)
                if control != good:
                    self.assertIsNone(report["usage"])

    def test_usage_rejects_nonpositive_and_malformed_snapshot_identity(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        (self.run / "runner.json").write_text('{"exit_code":0}', encoding="utf-8")
        for key, value in (("phase_index", 0), ("phase_index", -1),
                           ("phase_index", True), ("phase_index", 1.0), ("unit", " ")):
            with self.subTest(key=key, value=value):
                packet = dict(self.packet, **{key: value})
                (self.run / "packet.json").write_text(json.dumps(packet), encoding="utf-8")
                with self.assertRaisesRegex(guard.GuardError, "identity"):
                    guard.usage(self.run)

    def test_usage_rejects_every_missing_mandatory_packet_field(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        for key in guard.REQUIRED:
            with self.subTest(missing=key):
                packet = {k: v for k, v in self.packet.items() if k != key}
                (self.run / "packet.json").write_text(json.dumps(packet), encoding="utf-8")
                with self.assertRaises(guard.GuardError):
                    guard.usage(self.run)

    def test_usage_rejects_invalid_packet_structure_even_with_matching_runner(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        original = json.loads((self.run / "runner.json").read_text())
        changes = [(key, []) for key in guard.REQUIRED]
        changes += [(key, " ") for key in ("unit", "expected_head", "repo", "run_directory", "model", "effort", "rationale", "prompt", "output_schema")]
        changes += [
            ("phase", "unsupported"), ("model", "unsupported"), ("effort", "unsupported"),
            ("phase_index", True), ("phase_index", 2),
            ("repo", "relative"), ("run_directory", "relative"),
            ("run_directory", str(self.repo / "run")),
            ("run_directory", str(self.dir / "other-run")),
            ("prompt", str(self.dir / "unbound")), ("output_schema", str(self.dir / "unbound")),
            ("retry", {}), ("retry", {"attempt": True, "maximum": 1}),
            ("retry", {"attempt": -1, "maximum": 1}),
            ("retry", {"attempt": 2, "maximum": 1}),
            ("retry", {"attempt": 0, "maximum": 1.0}),
            ("ceilings", {"input_tokens": 30000.0, "output_tokens": 8000}),
            ("bindings", [self.packet["bindings"][0]]),
            ("prerequisites", [{}]), ("prerequisites", [self.packet["bindings"][0]]),
            ("prior_phases", [self.packet["bindings"][0]]),
            ("root_usage", "invalid"), ("root_usage", "unknown"),
            ("adjustment", False), ("adjustment", ""),
        ]
        changes += [(key, [False]) for key in ("allowlist", "policies", "focused_checks", "final_checks")]
        for key, value in changes:
            # Empty prerequisites/prior phases and a null adjustment are valid.
            if (key, value) in (("prerequisites", []), ("prior_phases", [])):
                continue
            with self.subTest(key=key, value=value):
                packet = dict(self.packet, **{key: value})
                runner = dict(original)
                if key in runner:
                    runner[key] = value
                (self.run / "packet.json").write_text(json.dumps(packet), encoding="utf-8")
                (self.run / "runner.json").write_text(json.dumps(runner), encoding="utf-8")
                if key != "run_directory" or value != str(self.dir / "other-run"):
                    with self.assertRaises(guard.GuardError):
                        guard.validate_packet_structure(packet)
                try:
                    code, report = self.usage_result()
                except guard.GuardError:
                    continue
                self.assertEqual((code, report["status"], report["usage"]), (1, "unknown", None))

    def test_usage_validates_prior_bindings_without_reading_original_reports(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.packet.update(phase_index=2, prior_phases=[{"path": str(self.dir / "removed-prior.json"), "sha256": "a" * 64}],
                           adjustment="Narrow context to one function and split the review.")
        self.write_run(json.dumps(event) + "\n")
        self.assertEqual(self.usage_result()[1]["status"], "pass")
        for prior in ([], [{}], [{"path": "prior", "sha256": "bad"}],
                      [self.packet["bindings"][0]], self.packet["prior_phases"] * 2):
            with self.subTest(prior=prior):
                packet = dict(self.packet, prior_phases=prior)
                (self.run / "packet.json").write_text(json.dumps(packet), encoding="utf-8")
                self.assertEqual(self.usage_result()[1]["status"], "unknown")

    def test_usage_requires_captured_prompt_and_schema_consistency(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        for name in ("prompt.txt", "schema.json"):
            snapshot = self.run / name
            original = snapshot.read_bytes()
            with self.subTest(snapshot=name):
                snapshot.write_bytes(b"changed")
                self.assertEqual(self.usage_result()[1]["status"], "unknown")
                snapshot.unlink()
                self.assertEqual(self.usage_result()[1]["status"], "unknown")
                snapshot.write_bytes(original)
        self.assertEqual(self.usage_result()[1]["status"], "pass")

    def test_usage_requires_complete_runner_metadata_and_agreement(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        original = json.loads((self.run / "runner.json").read_text())
        cases = [("exit_only", {"exit_code": 0})]
        cases.extend(("missing_" + key, {k: v for k, v in original.items() if k != key}) for key in original)
        changes = (("unit", "OTHER"), ("phase_index", 2), ("phase_index", True),
                   ("model", "other-model"), ("effort", "high"), ("bindings", []),
                   ("wall_seconds", -1), ("wall_seconds", True),
                   ("wall_seconds", float("nan")), ("wall_seconds", float("inf")),
                   ("prompt_estimate", -1), ("prompt_estimate", True),
                   ("runner_error", ""), ("runner_error", False), ("exit_code", "0"))
        cases.extend(("invalid_" + key + "_" + repr(value), dict(original, **{key: value})) for key, value in changes)
        for label, runner in cases:
            with self.subTest(fixture=label):
                (self.run / "runner.json").write_text(json.dumps(runner), encoding="utf-8")
                code, report = self.usage_result()
                self.assertEqual((code, report["status"], report["usage"]), (1, "unknown", None))

    def test_usage_rejects_malformed_binding_snapshots(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        original = json.loads((self.run / "runner.json").read_text())
        for bindings in ([], [{}], [{"path": " ", "sha256": "a" * 64}],
                         [{"path": "old", "sha256": "invalid"}],
                         [self.packet["bindings"][0]] * 2):
            with self.subTest(bindings=bindings):
                packet = dict(self.packet, bindings=bindings)
                runner = dict(original, bindings=bindings)
                (self.run / "packet.json").write_text(json.dumps(packet), encoding="utf-8")
                (self.run / "runner.json").write_text(json.dumps(runner), encoding="utf-8")
                self.assertEqual(self.usage_result()[1]["status"], "unknown")

    def test_usage_runner_error_and_exit_never_pass(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        original = json.loads((self.run / "runner.json").read_text())
        for exit_code, error in ((0, "OSError"), (1, None), (-1, "FileNotFoundError"),
                                 (-9, None), (None, "OSError")):
            with self.subTest(exit_code=exit_code, error=error):
                runner = dict(original, exit_code=exit_code, runner_error=error)
                (self.run / "runner.json").write_text(json.dumps(runner), encoding="utf-8")
                code, report = self.usage_result()
                self.assertEqual((code, report["status"], report["usage"]), (1, "failed", None))
                self.assertEqual(report["runner_error"], error)

    def test_usage_historical_snapshot_does_not_revalidate_live_files_or_head(self):
        event = {"type": "turn.completed", "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(event) + "\n")
        self.prompt.unlink()
        self.schema.write_text("changed after run", encoding="utf-8")
        with patch.object(guard.subprocess, "run", side_effect=AssertionError("historical usage must not check HEAD")):
            code, report = self.usage_result()
        self.assertEqual((code, report["status"]), (0, "pass"))
        self.assertEqual(report["bindings"], self.packet["bindings"])

    def test_usage_pass_exceeded_negative_incomplete_multiturn_and_failure(self):
        base = {"type": "turn.completed", "usage": {"input_tokens": 10, "cached_input_tokens": 2, "output_tokens": 4, "reasoning_output_tokens": 1}}
        self.write_run(json.dumps(base) + "\n")
        code, report = self.usage_result()
        self.assertEqual((code, report["status"], report["usage"]["uncached_input_tokens"]), (0, "pass", 8))
        base["usage"]["input_tokens"] = 30003  # 30001 uncached after the two cached tokens.
        (self.run / "events.jsonl").write_text(json.dumps(base) + "\n", encoding="utf-8")
        self.assertEqual(self.usage_result()[1]["status"], "exceeded")
        for usage in ({"input_tokens": -1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0},
                      {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1}):
            base["usage"] = usage
            (self.run / "events.jsonl").write_text(json.dumps(base) + "\n", encoding="utf-8")
            self.assertEqual(self.usage_result()[1]["status"], "unknown")
        base["usage"] = {"input_tokens": 10, "cached_input_tokens": 2, "output_tokens": 4, "reasoning_output_tokens": 1}
        (self.run / "events.jsonl").write_text((json.dumps(base) + "\n") * 2, encoding="utf-8")
        self.assertEqual(self.usage_result()[1]["status"], "unknown")
        (self.run / "runner.json").write_text(json.dumps({"exit_code": 1}), encoding="utf-8")
        self.assertEqual(self.usage_result()[1]["status"], "failed")

    def test_usage_rejects_bad_events_exit_and_packet_and_records_actual(self):
        valid = {"type": "turn.completed", "model": "gpt-6.1-sol", "reasoning_effort": "medium",
                 "usage": {"input_tokens": 1, "cached_input_tokens": 0, "output_tokens": 1, "reasoning_output_tokens": 0}}
        self.write_run(json.dumps(valid) + "\n")
        report = self.usage_result()[1]
        self.assertEqual((report["actual_model"], report["actual_effort"]), ("gpt-6.1-sol", "medium"))
        for prefix in ("[]\n", "{}\n", '{"type":"turn.failed"}\n', '{"type":"error"}\n'):
            (self.run / "events.jsonl").write_text(prefix + json.dumps(valid) + "\n", encoding="utf-8")
            self.assertNotEqual(self.usage_result()[1]["status"], "pass")
        (self.run / "events.jsonl").write_bytes(b"\xff\n" + (json.dumps(valid) + "\n").encode())
        self.assertEqual(self.usage_result()[1]["status"], "unknown")
        (self.run / "events.jsonl").write_text(json.dumps(valid) + "\n", encoding="utf-8")
        (self.run / "runner.json").write_text('{"exit_code":false}', encoding="utf-8")
        self.assertEqual(self.usage_result()[1]["status"], "unknown")
        (self.run / "packet.json").write_text('{"unit":"PROCESS-GUARDS","phase":"normal","phase_index":1}', encoding="utf-8")
        with self.assertRaisesRegex(guard.GuardError, "ceilings"):
            guard.usage(self.run)

    def test_usage_limits_uncached_input_and_output_independently(self):
        event = {"type": "turn.completed", "usage": {
            "input_tokens": 1000000, "cached_input_tokens": 970000,
            "output_tokens": 8000, "reasoning_output_tokens": 1000}}
        self.write_run(json.dumps(event) + "\n")
        code, report = self.usage_result()
        self.assertEqual(code, 0)
        self.assertEqual(report["status"], "pass")
        self.assertEqual(report["usage"]["uncached_input_tokens"], 30000)
        for field, value in (("cached_input_tokens", 969999), ("output_tokens", 8001)):
            with self.subTest(field=field):
                changed = json.loads(json.dumps(event))
                changed["usage"][field] = value
                (self.run / "events.jsonl").write_text(json.dumps(changed) + "\n")
                code, report = self.usage_result()
                self.assertEqual(code, 1)
                self.assertEqual(report["status"], "exceeded")

    def test_prior_usage_requires_adjustment_and_complete_set(self):
        prior = self.dir / "prior.json"
        prior.write_text(json.dumps({"unit": "PROCESS-GUARDS", "phase_index": 1, "status": "unknown", "root_usage": "unknown"}), encoding="utf-8")
        self.packet["phase_index"] = 2
        self.packet["prior_phases"] = [self.bind(prior)]
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "adjustment"):
            guard.validate(self.packet_file)
        self.packet["adjustment"] = "Narrow context to one function and split review from implementation."
        self.save()
        guard.validate(self.packet_file)
        self.packet["prior_phases"] = []
        self.save()
        with self.assertRaisesRegex(guard.GuardError, "incomplete"):
            guard.validate(self.packet_file)

    def test_excerpt_huge_one_line_is_bounded(self):
        path = self.dir / "huge.log"
        path.write_bytes(b"x" * 4_000_000)
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            guard.excerpt(path)
        data = json.loads(output.getvalue())
        self.assertEqual(data["longest_line_bytes"], 4_000_000)
        self.assertLess(len(output.getvalue()), 1000)
        self.assertTrue(data["truncated"])


if __name__ == "__main__":
    unittest.main()
