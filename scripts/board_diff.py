#!/usr/bin/env python3

from __future__ import annotations

import argparse
import shutil
from collections import defaultdict
from pathlib import Path

from fde_board import (
    BoardCase,
    CaseSelectionError,
    CommandError,
    ProbeMismatchError,
    default_resource_root,
    find_probe_command,
    load_cases,
    probe_bitstream,
    repo_root,
    run_rust_impl,
    select_cases,
)

class BaselineError(RuntimeError):
    pass


def default_baseline_root(root: Path) -> Path:
    return root.parent / "FDE-Source" / "build" / "hw-io-probe"


def default_out_root(root: Path) -> Path:
    return root / "build" / "board-diff"


def find_baseline_bitstreams(baseline_root: Path, case: BoardCase) -> list[Path]:
    case_root = baseline_root / case.name
    if not case_root.is_dir():
        return []
    return sorted(case_root.glob("**/06-output.bit"))


def select_reference_candidate(
    root: Path,
    case: BoardCase,
    probe_command: list[str],
    baseline_candidates: list[Path],
    case_out_dir: Path,
) -> tuple[Path, list[str]]:
    grouped_matches: dict[tuple[str, ...], list[Path]] = defaultdict(list)
    mismatches: list[str] = []

    for bitstream in baseline_candidates:
        relative_name = bitstream.parent.name
        log_path = case_out_dir / "baseline-probes" / relative_name / "wave_probe.log"
        try:
            outputs = probe_bitstream(
                root,
                bitstream,
                probe_command,
                case.probe_segments,
                log_path,
            )
        except CommandError as error:
            mismatches.append(f"{bitstream}: probe failed ({error})")
            continue

        grouped_matches[tuple(outputs)].append(bitstream)

    if not grouped_matches:
        summary = (
            "; ".join(mismatches)
            if mismatches
            else "no successfully probed candidate bitstreams"
        )
        raise BaselineError(
            f"{case.name}: could not establish a usable baseline ({summary})"
        )

    if len(grouped_matches) > 1:
        summaries = [
            f"{','.join(outputs)} <- {', '.join(str(path) for path in sorted(paths))}"
            for outputs, paths in sorted(grouped_matches.items())
        ]
        if mismatches:
            summaries.extend(mismatches)
        raise BaselineError(
            f"{case.name}: baseline bitstreams disagree on probed outputs ({'; '.join(summaries)})"
        )

    outputs, bitstreams = next(iter(grouped_matches.items()))
    return sorted(bitstreams)[0], list(outputs)


def cmd_list(args: argparse.Namespace, root: Path) -> int:
    all_cases = load_cases(root)
    baseline_root = Path(args.baseline_root).resolve()
    baseline_candidates = {
        case.name: find_baseline_bitstreams(baseline_root, case)
        for case in all_cases.values()
    }
    for case in all_cases.values():
        candidate_count = len(baseline_candidates[case.name])
        print(f"{case.name}\tbaseline_candidates={candidate_count}")
    return 0


def cmd_run(args: argparse.Namespace, root: Path) -> int:
    all_cases = load_cases(root)
    baseline_root = Path(args.baseline_root).resolve()
    if not baseline_root.is_dir():
        raise SystemExit(f"baseline root does not exist: {baseline_root}")
    baseline_candidates = {
        case.name: find_baseline_bitstreams(baseline_root, case)
        for case in all_cases.values()
    }

    try:
        if args.cases:
            selected_cases = select_cases(all_cases, args.cases)
        else:
            selected_cases = [
                case for case in all_cases.values() if baseline_candidates[case.name]
            ]
    except CaseSelectionError as error:
        raise SystemExit(str(error)) from error

    if not selected_cases:
        raise SystemExit(f"no board cases with baseline bitstreams found under {baseline_root}")

    missing_refs = [case.name for case in selected_cases if not baseline_candidates[case.name]]
    if missing_refs:
        raise SystemExit(
            "selected case(s) do not have baseline bitstreams: "
            + ", ".join(missing_refs)
        )

    resource_root = Path(args.resource_root).resolve()
    out_root = Path(args.out_root).resolve()
    out_root.mkdir(parents=True, exist_ok=True)
    probe_command = find_probe_command(root, args.wave_probe)

    failures: list[str] = []
    skipped: list[str] = []
    print(f"probe={' '.join(probe_command)}")
    print(f"baseline_root={baseline_root}")
    for case in selected_cases:
        case_out_dir = out_root / case.name
        if case_out_dir.exists():
            shutil.rmtree(case_out_dir)
        case_out_dir.mkdir(parents=True, exist_ok=True)

        try:
            current_bitstream = run_rust_impl(root, case, resource_root, case_out_dir / "current")
            current_outputs = probe_bitstream(
                root,
                current_bitstream,
                probe_command,
                case.probe_segments,
                case_out_dir / "current" / "wave_probe.log",
            )

            baseline_bitstream, baseline_outputs = select_reference_candidate(
                root,
                case,
                probe_command,
                baseline_candidates[case.name],
                case_out_dir,
            )
            if current_outputs != baseline_outputs:
                raise ProbeMismatchError(
                    f"{case.name}: current outputs {','.join(current_outputs)} differ from baseline outputs {','.join(baseline_outputs)}"
                )
        except BaselineError as error:
            if args.cases or args.strict:
                print(f"FAIL {error}")
                failures.append(case.name)
            else:
                print(f"SKIP {error}")
                skipped.append(case.name)
            continue
        except (CommandError, ProbeMismatchError) as error:
            print(f"FAIL {error}")
            failures.append(case.name)
            continue

        print(
            "PASS "
            f"{case.name}: matched baseline {baseline_bitstream} -> {','.join(current_outputs)}"
        )

    if skipped:
        print(f"{len(skipped)} case(s) skipped: {', '.join(skipped)}")

    if failures:
        print(f"{len(failures)} case(s) failed: {', '.join(failures)}")
        return 1

    matched = len(selected_cases) - len(skipped)
    print(f"all {matched} comparable case(s) matched the baseline")
    return 0


def build_parser() -> argparse.ArgumentParser:
    root = repo_root()
    parser = argparse.ArgumentParser(description="Compare board-probed outputs against a baseline.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    list_parser = subparsers.add_parser(
        "list",
        help="List board cases and how many baseline bitstreams were found.",
    )
    list_parser.add_argument(
        "--baseline-root",
        default=str(default_baseline_root(root)),
        help="Path to FDE-Source/build/hw-io-probe.",
    )

    run_parser = subparsers.add_parser(
        "run",
        help="Build current bitstreams and compare them against the baseline corpus.",
    )
    run_parser.add_argument(
        "cases",
        nargs="*",
        help="Case names to run. Defaults to all cases with discoverable baseline bitstreams.",
    )
    run_parser.add_argument(
        "--baseline-root",
        default=str(default_baseline_root(root)),
        help="Path to FDE-Source/build/hw-io-probe.",
    )
    run_parser.add_argument(
        "--resource-root",
        default=str(default_resource_root(root)),
        help="Path to the full hardware resource bundle.",
    )
    run_parser.add_argument(
        "--out-root",
        default=str(default_out_root(root)),
        help="Directory for Rust implementation artifacts and probe logs.",
    )
    run_parser.add_argument(
        "--wave-probe",
        help="Explicit wave_probe command or binary path.",
    )
    run_parser.add_argument(
        "--strict",
        action="store_true",
        help="Fail instead of skipping when a selected case does not have a unique baseline.",
    )
    return parser


def main() -> int:
    root = repo_root()
    parser = build_parser()
    args = parser.parse_args()
    if args.command == "list":
        return cmd_list(args, root)
    if args.command == "run":
        return cmd_run(args, root)
    raise AssertionError(f"unsupported command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
