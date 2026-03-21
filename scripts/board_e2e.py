#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
from pathlib import Path


OUTPUT_RE = re.compile(r"outputs=(0x[0-9a-f]+)")


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_resource_root(root: Path) -> Path:
    bundled = root / "resources" / "hw_lib"
    if bundled.is_dir():
        return bundled
    return root / "tests" / "fixtures" / "hw_lib"


def load_manifest(root: Path) -> dict:
    manifest_path = root / "examples" / "board-e2e" / "manifest.json"
    return json.loads(manifest_path.read_text())


def case_map(manifest: dict) -> dict[str, dict]:
    return {case["name"]: case for case in manifest["cases"]}


def find_probe_command(root: Path, override: str | None) -> list[str]:
    if override:
        return shlex.split(override)

    env_probe = os.environ.get("FDE_WAVE_PROBE")
    if env_probe:
        return shlex.split(env_probe)

    internal_binary = root / "tools" / "wave_probe" / "target" / "debug" / "wave_probe"
    if internal_binary.is_file():
        return [str(internal_binary)]

    internal_manifest = root / "tools" / "wave_probe" / "Cargo.toml"
    if internal_manifest.is_file():
        return [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            str(internal_manifest),
            "--",
        ]

    path_probe = shutil.which("wave_probe")
    if path_probe:
        return [path_probe]

    raise SystemExit(
        "could not find the in-repo wave_probe tool; set --wave-probe or FDE_WAVE_PROBE only if you intentionally want a custom probe command"
    )


def run_command(command: list[str], cwd: Path, log_path: Path) -> tuple[int, str]:
    proc = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    log_path.write_text(proc.stdout)
    return proc.returncode, proc.stdout


def parse_outputs(text: str) -> list[str]:
    return OUTPUT_RE.findall(text)


def run_case(
    root: Path,
    case: dict,
    resource_root: Path,
    out_root: Path,
    probe_command: list[str],
) -> tuple[bool, str]:
    case_dir = root / "examples" / "board-e2e"
    edf = case_dir / case["edf"]
    constraints = case_dir / case["constraints"]
    out_dir = out_root / case["name"]

    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    impl_cmd = [
        "cargo",
        "run",
        "--quiet",
        "--bin",
        "fde",
        "--",
        "impl",
        "--input",
        str(edf),
        "--constraints",
        str(constraints),
        "--resource-root",
        str(resource_root),
        "--out-dir",
        str(out_dir),
    ]
    code, impl_output = run_command(impl_cmd, root, out_dir / "impl.log")
    if code != 0:
        return False, f"{case['name']}: impl failed"

    bitstream = out_dir / "06-output.bit"
    probe_cmd = [*probe_command, str(bitstream)]
    code, probe_output = run_command(probe_cmd, root, out_dir / "wave_probe.log")
    if code != 0:
        return False, f"{case['name']}: wave_probe failed"

    actual = parse_outputs(probe_output)
    expected = case["expected_outputs"]
    if actual != expected:
        return (
            False,
            f"{case['name']}: expected {','.join(expected)} but saw {','.join(actual)}",
        )

    return True, f"{case['name']}: {','.join(actual)}"


def cmd_list(root: Path) -> int:
    manifest = load_manifest(root)
    for case in manifest["cases"]:
        print(case["name"])
    return 0


def cmd_run(args: argparse.Namespace, root: Path) -> int:
    manifest = load_manifest(root)
    cases = case_map(manifest)
    selected = args.cases or list(cases.keys())
    unknown = [name for name in selected if name not in cases]
    if unknown:
        raise SystemExit(f"unknown cases: {', '.join(unknown)}")

    resource_root = Path(args.resource_root).resolve()
    out_root = Path(args.out_root).resolve()
    out_root.mkdir(parents=True, exist_ok=True)
    probe_command = find_probe_command(root, args.wave_probe)

    failures: list[str] = []
    print(f"probe={' '.join(probe_command)}")
    for name in selected:
        ok, message = run_case(root, cases[name], resource_root, out_root, probe_command)
        print(("PASS " if ok else "FAIL ") + message)
        if not ok:
            failures.append(name)

    if failures:
        print(f"{len(failures)} case(s) failed: {', '.join(failures)}")
        return 1

    print(f"all {len(selected)} case(s) passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run board-probed fde-rs regressions.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("list", help="List available board cases.")

    run_parser = subparsers.add_parser("run", help="Build and probe board cases.")
    run_parser.add_argument("cases", nargs="*", help="Case names to run. Defaults to all cases.")
    run_parser.add_argument(
        "--resource-root",
        default=str(default_resource_root(repo_root())),
        help="Path to the full hardware resource bundle.",
    )
    run_parser.add_argument(
        "--out-root",
        default=str(repo_root() / "build" / "board-e2e"),
        help="Directory for generated implementation and probe artifacts.",
    )
    run_parser.add_argument(
        "--wave-probe",
        help="Explicit wave_probe command or binary path.",
    )
    return parser


def main() -> int:
    root = repo_root()
    parser = build_parser()
    args = parser.parse_args()
    if args.command == "list":
        return cmd_list(root)
    if args.command == "run":
        return cmd_run(args, root)
    raise AssertionError(f"unsupported command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
