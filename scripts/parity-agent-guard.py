#!/usr/bin/env python3
"""Local PROCESS-GUARDS helper. Run `--help` for commands.

This checks one packet and one local runner invocation with a fixed
workspace-write sandbox and the packet's repository as the runner cwd. It cannot lock other
writers, cap live model spending, intercept agent tools, or prove that a
coordinator declared every unit. Those remain coordinator responsibilities.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


LIMITS = {"docs": (12000, 4000), "normal": (30000, 8000), "difficult": (50000, 12000)}
MODELS = {"gpt-6.1-sol", "gpt-6-astra", "gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol"}
REQUIRED = ("unit", "phase", "phase_index", "expected_head", "repo", "run_directory",
            "model", "effort", "rationale", "prompt", "output_schema", "bindings",
            "allowlist", "prerequisites", "policies", "focused_checks", "final_checks",
            "retry", "ceilings", "prior_phases", "root_usage", "adjustment")


class GuardError(Exception):
    pass


def fail(message):
    raise GuardError(message)


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def json_file(path):
    try:
        with open(path, encoding="utf-8") as stream:
            return json.load(stream)
    except (OSError, ValueError) as exc:
        fail(f"invalid JSON at {path}: {exc.__class__.__name__}")


def required_string(value, label):
    if not isinstance(value, str) or not value.strip():
        fail(f"{label} must be a nonempty string")


def require_strings(items, label, *, nonempty=True):
    if not isinstance(items, list) or (nonempty and not items):
        fail(f"{label} must be a {'nonempty ' if nonempty else ''}list")
    for item in items:
        required_string(item, label + " item")


def binding(item, label, *, check_file=True):
    if not isinstance(item, dict) or set(item) != {"path", "sha256"}:
        fail(f"{label} requires path and sha256")
    required_string(item["path"], label + " path")
    digest = item["sha256"]
    if not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
        fail(f"{label} has invalid SHA256")
    path = Path(item["path"])
    if check_file and (not path.is_file() or sha(path) != digest):
        fail(f"{label} binding changed or missing: {path}")


def prompt_tokens(path):
    try:
        import tiktoken
    except ImportError:
        fail("tiktoken is required for o200k_base preflight; install is outside this helper")
    try:
        return len(tiktoken.get_encoding("o200k_base").encode(Path(path).read_text(encoding="utf-8")))
    except (OSError, UnicodeError, ValueError) as exc:
        fail(f"cannot tokenize prompt: {exc.__class__.__name__}")


def validate_packet_structure(packet):
    """Validate saved metadata without reading live repository or evidence files."""
    if not isinstance(packet, dict) or any(key not in packet for key in REQUIRED):
        fail("packet missing mandatory fields")
    for key in ("unit", "expected_head", "repo", "run_directory", "model", "effort",
                "rationale", "prompt", "output_schema"):
        required_string(packet[key], key)
    if not isinstance(packet["phase"], str) or packet["phase"] not in LIMITS:
        fail("unsupported phase routing")
    if packet["model"] not in MODELS:
        fail("unsupported model routing")
    if packet["effort"] not in ("low", "medium", "high", "xhigh", "max", "ultra"):
        fail("unsupported effort routing")
    if not isinstance(packet["phase_index"], int) or isinstance(packet["phase_index"], bool) or packet["phase_index"] < 1:
        fail("phase_index must be positive")
    if not isinstance(packet["retry"], dict) or set(packet["retry"]) != {"attempt", "maximum"}:
        fail("retry requires attempt and maximum")
    attempt, maximum = packet["retry"]["attempt"], packet["retry"]["maximum"]
    if any(not isinstance(n, int) or isinstance(n, bool) or n < 0 for n in (attempt, maximum)) or attempt > maximum:
        fail("invalid retry counters")
    if (not isinstance(packet["ceilings"], dict) or
            any(not isinstance(n, int) or isinstance(n, bool) for n in packet["ceilings"].values()) or
            packet["ceilings"] != {"input_tokens": LIMITS[packet["phase"]][0], "output_tokens": LIMITS[packet["phase"]][1]}):
        fail("ceilings must match phase policy")
    for key in ("allowlist", "policies", "focused_checks", "final_checks"):
        require_strings(packet[key], key)
    for key in ("bindings", "prerequisites", "prior_phases"):
        if not isinstance(packet[key], list):
            fail(f"{key} must be a list")
    if not packet["bindings"]:
        fail("bindings must include prompt and schema")
    seen = set()
    for item in packet["bindings"] + packet["prerequisites"]:
        binding(item, "evidence", check_file=False)
        if item["path"] in seen:
            fail("duplicate evidence binding")
        seen.add(item["path"])
    direct_bindings = {item["path"] for item in packet["bindings"]}
    if not {packet["prompt"], packet["output_schema"]}.issubset(direct_bindings):
        fail("prompt and schema must be bound")
    if len(packet["prior_phases"]) != packet["phase_index"] - 1:
        fail("prior phase set incomplete")
    for index, item in enumerate(packet["prior_phases"], 1):
        binding(item, f"prior phase {index}", check_file=False)
        if item["path"] in seen:
            fail("duplicate evidence binding")
        seen.add(item["path"])
    for key in ("repo", "run_directory"):
        if not Path(packet[key]).is_absolute():
            fail("repo and run_directory must be absolute")
    if Path(packet["run_directory"]).is_relative_to(Path(packet["repo"])):
        fail("run_directory must be external to repository")
    adjustment = packet["adjustment"]
    if adjustment is not None and (not isinstance(adjustment, str) or not adjustment.strip()):
        fail("adjustment must be null or a nonempty string")
    if packet["root_usage"] not in ("known", "unknown"):
        fail("root_usage must be known or unknown")
    needs_adjustment = packet["phase_index"] > 1 or packet["root_usage"] == "unknown"
    if needs_adjustment and (not isinstance(adjustment, str) or
        len(adjustment.strip()) < 20 or
        not any(word in adjustment.lower() for word in ("narrow", "split", "reduce", "smaller"))):
        fail("unknown root or prior usage requires concrete narrowed-context or split adjustment")
    return packet


def validate(packet_path):
    packet = validate_packet_structure(json_file(packet_path))
    for item in packet["bindings"] + packet["prerequisites"]:
        binding(item, "evidence")
    for index, item in enumerate(packet["prior_phases"], 1):
        binding(item, f"prior phase {index}")
        report = json_file(item["path"])
        if not isinstance(report, dict) or report.get("phase_index") != index or report.get("unit") != packet["unit"]:
            fail("prior phase report mismatch")
        if report.get("status") == "pass" and not isinstance(report.get("usage"), dict):
            fail("prior phase PASS lacks usage telemetry")
    repo = Path(packet["repo"]).resolve()
    if not repo.is_dir() or not (repo / ".git").exists():
        fail("invalid repository")
    if not Path(packet["repo"]).is_absolute() or not Path(packet["run_directory"]).is_absolute():
        fail("repo and run_directory must be absolute")
    head = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], capture_output=True, text=True, check=False)
    if head.returncode or head.stdout.strip() != packet["expected_head"]:
        fail("stale HEAD")
    run = Path(packet["run_directory"])
    if run.resolve().is_relative_to(repo):
        fail("run_directory must be external to repository")
    if run.exists():
        fail("duplicate started run")
    count = prompt_tokens(packet["prompt"])
    if count > 1000:
        fail(f"prompt estimate {count} exceeds 1000 o200k_base tokens")
    schema = json_file(packet["output_schema"])
    if not isinstance(schema, dict) or schema.get("type") != "object" or not isinstance(schema.get("properties"), dict):
        fail("output_schema must define an object with properties")
    return packet, count


def launch(packet_path):
    packet, estimated = validate(packet_path)
    run = Path(packet["run_directory"])
    run.mkdir(parents=True, exist_ok=False)
    (run / "packet.json").write_text(json.dumps(packet, sort_keys=True) + "\n", encoding="utf-8")
    for name, source in (("prompt.txt", packet["prompt"]),
                         ("schema.json", packet["output_schema"])):
        (run / name).write_bytes(Path(source).read_bytes())
    snapshots = {"prompt.txt": packet["prompt"], "schema.json": packet["output_schema"]}
    for name, source in snapshots.items():
        expected = next(item["sha256"] for item in packet["bindings"] if item["path"] == source)
        if sha(run / name) != expected:
            fail(f"{name} changed after preflight")
    argv = ["codex", "-c", 'approval_policy="never"', "-c",
            f'model_reasoning_effort="{packet["effort"]}"', "exec", "--ephemeral",
            "--ignore-user-config", "-m", packet["model"], "-s", "workspace-write",
            "-C", str(Path(packet["repo"]).resolve()), "--output-schema", str(run / "schema.json"),
            "--json", "-o", str(run / "answer.txt"), "-"]
    started = time.monotonic()
    runner_error = None
    with (run / "prompt.txt").open("rb") as source, (run / "events.jsonl").open("wb") as out, (run / "stderr.log").open("wb") as err:
        try:
            result = subprocess.run(argv, stdin=source, stdout=out, stderr=err, check=False)
            exit_code = result.returncode
        except OSError as exc:
            exit_code = -1
            runner_error = exc.__class__.__name__
    status = {"exit_code": exit_code, "runner_error": runner_error,
              "wall_seconds": round(time.monotonic() - started, 3),
              "model": packet["model"], "effort": packet["effort"], "prompt_estimate": estimated,
              "bindings": packet["bindings"], "phase_index": packet["phase_index"], "unit": packet["unit"]}
    (run / "runner.json").write_text(json.dumps(status, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"run_directory": str(run), "exit_code": exit_code, "wall_seconds": status["wall_seconds"]}))
    return 0 if exit_code == 0 else 1


def usage(run_dir):
    run = Path(run_dir)
    packet = json_file(run / "packet.json")
    runner = json_file(run / "runner.json")
    if not isinstance(packet, dict) or not isinstance(runner, dict):
        fail("invalid packet or runner report")
    if not isinstance(packet.get("unit"), str) or not packet["unit"].strip() or not isinstance(packet.get("phase_index"), int) or isinstance(packet["phase_index"], bool) or packet["phase_index"] < 1:
        fail("invalid packet identity")
    phase = packet.get("phase")
    if not isinstance(phase, str) or phase not in LIMITS or packet.get("ceilings") != {"input_tokens": LIMITS[phase][0], "output_tokens": LIMITS[phase][1]}:
        fail("invalid packet ceilings")
    # Validate saved metadata only: historical reports must not depend on live
    # repository HEAD or on binding files still existing at their original paths.
    if any(key not in packet for key in REQUIRED):
        fail("packet missing mandatory fields")
    bindings = packet["bindings"]
    packet_valid = True
    try:
        validate_packet_structure(packet)
        if Path(packet["run_directory"]).absolute() != run.absolute():
            fail("saved run_directory mismatch")
        for name, source in (("prompt.txt", packet["prompt"]),
                             ("schema.json", packet["output_schema"])):
            expected = next(item["sha256"] for item in bindings if item["path"] == source)
            if sha(run / name) != expected:
                fail("captured prompt or schema binding mismatch")
        schema = json_file(run / "schema.json")
        if not isinstance(schema, dict) or schema.get("type") != "object" or not isinstance(schema.get("properties"), dict):
            fail("invalid captured schema")
    except (GuardError, OSError):
        packet_valid = False
    wall = runner.get("wall_seconds")
    estimate = runner.get("prompt_estimate")
    error = runner.get("runner_error")
    valid_error = "runner_error" in runner and (error is None or isinstance(error, str) and bool(error.strip()))
    metadata_valid = (
        packet_valid and runner.get("bindings") == bindings
        and isinstance(packet.get("model"), str) and bool(packet["model"].strip())
        and isinstance(packet.get("effort"), str) and bool(packet["effort"].strip())
        and all(runner.get(key) == packet.get(key) for key in ("unit", "phase_index", "model", "effort"))
        and isinstance(runner.get("phase_index"), int) and not isinstance(runner["phase_index"], bool)
        and isinstance(wall, (int, float)) and not isinstance(wall, bool) and 0 <= wall < float("inf")
        and isinstance(estimate, int) and not isinstance(estimate, bool) and 0 <= estimate <= 1000
        and valid_error)
    completed = []
    malformed = False
    runner_failed_event = False
    actual_model = actual_effort = None
    telemetry_conflict = False
    try:
        with (run / "events.jsonl").open(encoding="utf-8") as stream:
            for line in stream:
                try:
                    event = json.loads(line)
                except ValueError:
                    malformed = True
                    continue
                if not isinstance(event, dict) or not isinstance(event.get("type"), str):
                    malformed = True
                    continue
                if event["type"] in ("turn.failed", "thread.failed", "error"):
                    runner_failed_event = True
                if isinstance(event.get("model"), str) and event["model"]:
                    if actual_model is not None and actual_model != event["model"]:
                        telemetry_conflict = True
                    actual_model = event["model"]
                effort = event.get("reasoning_effort", event.get("effort"))
                if isinstance(effort, str) and effort:
                    if actual_effort is not None and actual_effort != effort:
                        telemetry_conflict = True
                    actual_effort = effort
                if event["type"] == "turn.completed":
                    completed.append(event.get("usage"))
    except (OSError, UnicodeError):
        malformed = True
    status = "unknown"
    values = None
    exit_code = runner.get("exit_code")
    valid_exit = isinstance(exit_code, int) and not isinstance(exit_code, bool)
    if valid_exit and exit_code == 0 and metadata_valid and error is None and not malformed and not runner_failed_event and not telemetry_conflict and len(completed) == 1 and isinstance(completed[0], dict):
        raw = completed[0]
        keys = ("input_tokens", "cached_input_tokens", "output_tokens", "reasoning_output_tokens")
        if all(isinstance(raw.get(k), int) and not isinstance(raw.get(k), bool) and raw[k] >= 0 for k in keys):
            values = {k: raw[k] for k in keys}
            values["uncached_input_tokens"] = values["input_tokens"] - values["cached_input_tokens"]
            if values["uncached_input_tokens"] >= 0 and values["reasoning_output_tokens"] <= values["output_tokens"]:
                ceiling = packet["ceilings"]
                status = "pass" if values["input_tokens"] <= ceiling["input_tokens"] and values["output_tokens"] <= ceiling["output_tokens"] else "exceeded"
            else:
                values = None
    if valid_exit and exit_code != 0 or runner_failed_event or valid_error and error is not None:
        status = "failed"
    report = {"unit": packet["unit"], "phase_index": packet["phase_index"], "status": status,
              "root_usage": packet.get("root_usage", "unknown"), "usage": values,
              "requested_model": runner.get("model"), "requested_effort": runner.get("effort"),
              "actual_model": actual_model if not telemetry_conflict else None,
              "actual_effort": actual_effort if not telemetry_conflict else None,
              "wall_seconds": runner.get("wall_seconds"),
              "bindings": runner.get("bindings"), "runner_exit_code": runner.get("exit_code"),
              "runner_error": error}
    target = run / "usage.json"
    target.write_text(json.dumps(report, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"status": status, "report": str(target), "usage": values}))
    return 0 if status == "pass" else 1


def excerpt(path):
    length = lines = longest = current = 0
    digest = hashlib.sha256()
    sample = bytearray()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            digest.update(chunk)
            length += len(chunk)
            if len(sample) < 512:
                sample.extend(chunk[:512 - len(sample)])
            for part in chunk.split(b"\n")[:-1]:
                current += len(part) + 1
                longest = max(longest, current)
                lines += 1
                current = 0
            current += len(chunk.rsplit(b"\n", 1)[-1])
    if current:
        lines += 1
        longest = max(longest, current)
    print(json.dumps({"bytes": length, "lines": lines, "longest_line_bytes": longest,
                      "sha256": digest.hexdigest(), "excerpt": sample.decode("utf-8", "replace"),
                      "truncated": length > len(sample)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for command, argument in (("preflight", "packet"), ("launch", "packet"),
                              ("usage", "run_dir"), ("excerpt", "file")):
        sub.add_parser(command).add_argument(argument)
    args = parser.parse_args()
    try:
        if args.command == "preflight":
            _, count = validate(args.packet)
            print(json.dumps({"status": "pass", "prompt_estimate": count}))
        elif args.command == "launch":
            return launch(args.packet)
        elif args.command == "usage":
            return usage(args.run_dir)
        else:
            excerpt(args.file)
    except (GuardError, OSError) as exc:
        print(json.dumps({"status": "error", "error": str(exc)[:300]}), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
