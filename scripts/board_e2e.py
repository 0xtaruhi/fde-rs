#!/usr/bin/env python3

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

from fde_board import (
    BoardCase,
    CaseSelectionError,
    CommandError,
    ProbeMismatchError,
    default_resource_root,
    ensure_expected_outputs,
    find_probe_command,
    load_cases,
    probe_bitstream,
    repo_root,
    run_rust_impl,
    select_cases,
)


def default_out_root(root: Path) -> Path:
    return root / "build" / "board-e2e"


def run_case(
    root: Path,
    case: BoardCase,
    resource_root: Path,
    out_root: Path,
    probe_command: list[str],
) -> tuple[bool, str]:
    out_dir = out_root / case.name
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    try:
        bitstream = run_rust_impl(root, case, resource_root, out_dir)
        actual_outputs = probe_bitstream(
            root,
            bitstream,
            probe_command,
            case.probe_segments,
            out_dir / "wave_probe.log",
        )
        ensure_expected_outputs(case, actual_outputs)
    except (CommandError, ProbeMismatchError) as error:
        return False, str(error)

    return True, f"{case.name}: {','.join(actual_outputs)}"


def cmd_list(root: Path) -> int:
    for case in load_cases(root).values():
        print(case.name)
    return 0


def cmd_run(args: argparse.Namespace, root: Path) -> int:
    try:
        selected_cases = select_cases(load_cases(root), args.cases)
    except CaseSelectionError as error:
        raise SystemExit(str(error)) from error

    resource_root = Path(args.resource_root).resolve()
    out_root = Path(args.out_root).resolve()
    out_root.mkdir(parents=True, exist_ok=True)
    probe_command = find_probe_command(root, args.wave_probe)

    failures: list[str] = []
    print(f"probe={' '.join(probe_command)}")
    for case in selected_cases:
        ok, message = run_case(root, case, resource_root, out_root, probe_command)
        print(("PASS " if ok else "FAIL ") + message)
        if not ok:
            failures.append(case.name)

    if failures:
        print(f"{len(failures)} case(s) failed: {', '.join(failures)}")
        return 1

    print(f"all {len(selected_cases)} case(s) passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    root = repo_root()
    parser = argparse.ArgumentParser(description="Run board-probed fde-rs regressions.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("list", help="List available board cases.")

    run_parser = subparsers.add_parser("run", help="Build and probe board cases.")
    run_parser.add_argument(
        "cases",
        nargs="*",
        help="Case names to run. Defaults to all cases.",
    )
    run_parser.add_argument(
        "--resource-root",
        default=str(default_resource_root(root)),
        help="Path to the full hardware resource bundle.",
    )
    run_parser.add_argument(
        "--out-root",
        default=str(default_out_root(root)),
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
