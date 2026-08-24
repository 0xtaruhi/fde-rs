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
    parse_outputs,
    probe_bitstream,
    repo_root,
    require_success,
    run_command,
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
        if case.rtl_top is not None:
            simulate_case(root, case, out_dir)
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


def simulate_case(root: Path, case: BoardCase, out_dir: Path) -> list[str]:
    if case.rtl_top is None or case.rtl_testbench is None:
        raise ValueError(f"{case.name}: missing RTL simulation configuration")
    iverilog = shutil.which("iverilog")
    vvp = shutil.which("vvp")
    if not iverilog or not vvp:
        raise RuntimeError("RTL-backed board cases require iverilog and vvp")

    executable = out_dir / "simulation.out"
    compile_log = out_dir / "simulation.compile.log"
    run_log = out_dir / "simulation.log"
    compile_result = run_command(
        [
            iverilog,
            "-g2012",
            f"-DDUT={case.rtl_top}",
            "-s",
            "tb",
            "-o",
            str(executable),
            *(str(path) for path in case.rtl_sources),
            str(case.rtl_testbench),
        ],
        root,
        compile_log,
    )
    require_success(f"{case.name}: simulation compile", compile_result, compile_log)
    run_result = run_command([vvp, str(executable)], root, run_log)
    require_success(f"{case.name}: simulation", run_result, run_log)
    outputs = parse_outputs(run_result.output)
    ensure_expected_outputs(case, outputs)
    return outputs


def cmd_list(root: Path) -> int:
    for case in load_cases(root).values():
        print(case.name)
    return 0


def cmd_run(args: argparse.Namespace, root: Path) -> int:
    try:
        selected_cases = select_cases(load_cases(root), args.cases)
    except CaseSelectionError as error:
        raise SystemExit(str(error)) from error
    if args.rtl_only:
        selected_cases = [case for case in selected_cases if case.rtl_top is not None]

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


def cmd_simulate(args: argparse.Namespace, root: Path) -> int:
    try:
        selected_cases = select_cases(load_cases(root), args.cases)
    except CaseSelectionError as error:
        raise SystemExit(str(error)) from error

    out_root = Path(args.out_root).resolve()
    simulated = 0
    for case in selected_cases:
        if case.rtl_top is None:
            continue
        out_dir = out_root / case.name
        out_dir.mkdir(parents=True, exist_ok=True)
        outputs = simulate_case(root, case, out_dir)
        print(f"PASS {case.name}: {','.join(outputs)}")
        simulated += 1
    print(f"all {simulated} RTL-backed case(s) passed simulation")
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
    run_parser.add_argument(
        "--rtl-only",
        action="store_true",
        help="Run only cases backed by a reproducible RTL simulation.",
    )

    simulate_parser = subparsers.add_parser(
        "simulate", help="Simulate RTL-backed board cases without hardware."
    )
    simulate_parser.add_argument("cases", nargs="*", help="Case names. Defaults to all.")
    simulate_parser.add_argument(
        "--out-root",
        default=str(default_out_root(root)),
        help="Directory for simulation artifacts.",
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
    if args.command == "simulate":
        return cmd_simulate(args, root)
    raise AssertionError(f"unsupported command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
