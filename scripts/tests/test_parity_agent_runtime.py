"""Exercise cancellation using real local subprocesses and cumulative usage."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import signal
import subprocess
import time
import tempfile
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / "parity_agent_runtime.py"
SPEC = importlib.util.spec_from_file_location("parity_agent_runtime", SOURCE)
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.packet = {"repo": str(self.root), "model": "gpt-6.1-sol", "effort": "medium",
                       "prompt": "p", "task": "p", "ceilings": {"input_tokens": 30000, "output_tokens": 8000},
                       "execution": {"max_wall_seconds": 3, "max_commands": 3, "telemetry_deadline_seconds": 2}}
        server = Path(__file__).with_name("fake_codex_app_server.py")
        binary = self.root / "codex"
        binary.write_text(f'#!{sys.executable}\nexec(compile(open({str(server)!r}).read(), {str(server)!r}, "exec"))\n')
        binary.chmod(0o755)
        self.run = self.root / "run"
        self.run.mkdir()
        (self.run / "prompt.txt").write_text("bounded task")
        (self.run / "task.txt").write_text("bound primary task")
        (self.run / "schema.json").write_text('{"type":"object","properties":{}}')

    def invoke(self, mode):
        with patch.dict(os.environ, {"PATH": str(self.root) + os.pathsep + os.environ.get("PATH", ""),
                                     "FAKE_MODE": mode, "FAKE_INTERRUPTED": str(self.root / "interrupt")}):
            return runtime.run_live(self.packet, self.run)

    def test_completed_cached_usage_and_duplicate_snapshots(self):
        for mode in ["complete", "duplicate"]:
            with self.subTest(mode=mode):
                result = self.invoke(mode)
                self.assertIsNone(result["stop_reason"])
                self.assertTrue(result["completed"])
                self.assertEqual(result["usage"]["uncached_input_tokens"], 10)
                self.assertEqual(result["usage"]["output_tokens"], 4)
                self.assertEqual(json.loads((self.run / "answer.txt").read_text()), {})

    def test_limits_interrupt_real_server_and_do_not_write_success(self):
        for mode, reason in [("input_limit", "input_limit"), ("output_limit", "output_limit"),
                             ("commands", "command_limit"), ("no_telemetry", "telemetry_unavailable"),
                             ("wall", "wall_limit")]:
            with self.subTest(mode=mode):
                (self.run / "answer.txt").unlink(missing_ok=True)
                (self.root / "interrupt").unlink(missing_ok=True)
                result = self.invoke(mode)
                self.assertEqual(result["stop_reason"], reason)
                self.assertTrue(result["interrupt_sent"])
                self.assertTrue((self.root / "interrupt").exists())
                self.assertFalse((self.run / "answer.txt").exists())
                self.assertEqual(json.loads((self.run / "events.jsonl").read_text())["type"], "turn.failed")

    def test_bad_telemetry_is_not_success(self):
        for mode in ["decreasing", "malformed"]:
            with self.subTest(mode=mode):
                self.assertEqual(self.invoke(mode)["stop_reason"], "protocol_error")

    def test_queued_completion_cannot_skip_boundary_interrupt(self):
        result = self.invoke("queued")
        self.assertEqual(result["stop_reason"], "input_limit")
        self.assertTrue(result["interrupt_sent"])
        self.assertFalse((self.run / "answer.txt").exists())

    def test_group_cleanup_kills_resistant_child_after_leader_exit(self):
        for exit_first in (False, True):
            with self.subTest(exit_first=exit_first):
                pid_file = self.root / "child.pid"
                pid_file.unlink(missing_ok=True)
                child = ("import os,signal,time; from pathlib import Path; "
                         "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                         f"Path({str(pid_file)!r}).write_text(str(os.getpid())); time.sleep(60)")
                parent = ("import subprocess,sys,time; from pathlib import Path; "
                          f"subprocess.Popen([sys.executable,'-c',{child!r}]); "
                          f"p=Path({str(pid_file)!r})\n"
                          "while not p.exists(): time.sleep(0.01)\n"
                          + ("sys.exit(0)" if exit_first else "time.sleep(60)"))
                process = subprocess.Popen([sys.executable, "-c", parent], start_new_session=True,
                                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                           stderr=subprocess.DEVNULL)
                try:
                    deadline = time.monotonic() + 3
                    while not pid_file.exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(pid_file.exists())
                    pid = int(pid_file.read_text())
                    if exit_first:
                        process.wait(timeout=3)
                    runtime.terminate_group(process)
                    deadline = time.monotonic() + 2
                    while time.monotonic() < deadline:
                        try:
                            os.kill(pid, 0)
                        except ProcessLookupError:
                            break
                        time.sleep(0.02)
                    with self.assertRaises(ProcessLookupError):
                        os.kill(pid, 0)
                    self.assertIsNotNone(process.returncode)
                finally:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait(timeout=1)

    def test_startup_without_protocol_is_stopped(self):
        result = self.invoke("no_initialize")
        self.assertEqual(result["stop_reason"], "telemetry_unavailable")
        self.assertFalse(result["completed"])

    def test_counter_validation(self):
        good = {"inputTokens": 10, "cachedInputTokens": 2, "outputTokens": 4, "reasoningOutputTokens": 1}
        for key, value in [("inputTokens", True), ("cachedInputTokens", 11),
                           ("outputTokens", -1), ("reasoningOutputTokens", 5)]:
            with self.subTest(key=key):
                with self.assertRaises(runtime.ProtocolError):
                    runtime.usage_values(dict(good, **{key: value}))


if __name__ == "__main__":
    unittest.main()
