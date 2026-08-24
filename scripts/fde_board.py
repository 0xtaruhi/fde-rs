#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import re
import shlex
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

OUTPUT_RE = re.compile(r"outputs=(0x[0-9a-f]+)")


@dataclass(frozen=True, slots=True)
class BoardCase:
    name: str
    edf: Path
    constraints: Path
    expected_outputs: tuple[str, ...]
    probe_segments: tuple[str, ...]
    rtl_top: str | None = None
    rtl_sources: tuple[Path, ...] = ()
    rtl_testbench: Path | None = None


@dataclass(frozen=True, slots=True)
class CommandResult:
    returncode: int
    output: str


class CommandError(RuntimeError):
    def __init__(self, label: str, result: CommandResult, log_path: Path) -> None:
        super().__init__(
            f"{label} failed with exit code {result.returncode}; see {log_path}"
        )
        self.label = label
        self.result = result
        self.log_path = log_path


class CaseSelectionError(RuntimeError):
    pass


class ProbeMismatchError(RuntimeError):
    pass


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_resource_root(root: Path) -> Path:
    bundled = root / "resources" / "hw_lib"
    if bundled.is_dir():
        return bundled
    return root / "tests" / "fixtures" / "hw_lib"


def board_manifest_path(root: Path) -> Path:
    return root / "examples" / "board-e2e" / "manifest.json"


def load_manifest(root: Path) -> dict:
    return json.loads(board_manifest_path(root).read_text())


def load_cases(root: Path) -> dict[str, BoardCase]:
    case_root = root / "examples" / "board-e2e"
    manifest = load_manifest(root)
    simulation_profiles = manifest.get("simulation_profiles", {})
    cases: dict[str, BoardCase] = {}
    for raw_case in manifest["cases"]:
        rtl_top = raw_case.get("rtl_top")
        profile_name = raw_case.get("rtl_profile", "combinational")
        if rtl_top and profile_name not in simulation_profiles:
            raise ValueError(
                f"{raw_case['name']}: unknown RTL profile {profile_name!r}"
            )
        simulation = simulation_profiles[profile_name] if rtl_top else {}
        rtl_sources = tuple(root / path for path in simulation.get("sources", ()))
        rtl_testbench = simulation.get("testbench")
        if rtl_top and (not rtl_sources or not rtl_testbench):
            raise ValueError(
                f"{raw_case['name']}: RTL profile {profile_name!r} needs sources and a testbench"
            )
        case = BoardCase(
            name=raw_case["name"],
            edf=case_root / raw_case["edf"],
            constraints=case_root / raw_case["constraints"],
            expected_outputs=tuple(raw_case["expected_outputs"]),
            probe_segments=tuple(
                raw_case.get(
                    "probe_segments",
                    simulation.get("probe_segments", ()) if rtl_top else (),
                )
            ),
            rtl_top=rtl_top,
            rtl_sources=rtl_sources if rtl_top else (),
            rtl_testbench=root / rtl_testbench if rtl_top and rtl_testbench else None,
        )
        cases[case.name] = case
    return cases


def select_cases(
    all_cases: dict[str, BoardCase],
    selected_names: list[str] | None,
) -> list[BoardCase]:
    if not selected_names:
        return list(all_cases.values())

    unknown = [name for name in selected_names if name not in all_cases]
    if unknown:
        raise CaseSelectionError(f"unknown cases: {', '.join(unknown)}")
    return [all_cases[name] for name in selected_names]


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

    raise RuntimeError(
        "could not find the in-repo wave_probe tool; set --wave-probe or FDE_WAVE_PROBE only if you intentionally want a custom probe command"
    )


def run_command(command: list[str], cwd: Path, log_path: Path) -> CommandResult:
    proc = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(proc.stdout)
    return CommandResult(returncode=proc.returncode, output=proc.stdout)


def require_success(label: str, result: CommandResult, log_path: Path) -> None:
    if result.returncode != 0:
        raise CommandError(label, result, log_path)


def parse_outputs(text: str) -> list[str]:
    return OUTPUT_RE.findall(text)


def build_impl_command(
    case: BoardCase,
    resource_root: Path,
    out_dir: Path,
) -> list[str]:
    return [
        "cargo",
        "run",
        "--quiet",
        "--bin",
        "fde",
        "--",
        "impl",
        "--input",
        str(case.edf),
        "--constraints",
        str(case.constraints),
        "--resource-root",
        str(resource_root),
        "--out-dir",
        str(out_dir),
    ]


def build_probe_command(
    probe_command: list[str],
    bitstream: Path,
    probe_segments: tuple[str, ...],
) -> list[str]:
    return [*probe_command, str(bitstream), *probe_segments]


def run_rust_impl(
    root: Path,
    case: BoardCase,
    resource_root: Path,
    out_dir: Path,
) -> Path:
    impl_result = run_command(
        build_impl_command(case, resource_root, out_dir),
        root,
        out_dir / "impl.log",
    )
    require_success(f"{case.name}: impl", impl_result, out_dir / "impl.log")
    return out_dir / "06-output.bit"


def probe_bitstream(
    root: Path,
    bitstream: Path,
    probe_command: list[str],
    probe_segments: tuple[str, ...],
    log_path: Path,
) -> list[str]:
    result = run_command(
        build_probe_command(probe_command, bitstream, probe_segments),
        root,
        log_path,
    )
    require_success(f"probe {bitstream}", result, log_path)
    return parse_outputs(result.output)


def ensure_expected_outputs(case: BoardCase, actual_outputs: list[str]) -> None:
    expected = list(case.expected_outputs)
    if actual_outputs != expected:
        raise ProbeMismatchError(
            f"{case.name}: expected {','.join(expected)} but saw {','.join(actual_outputs)}"
        )
