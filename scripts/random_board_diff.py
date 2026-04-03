#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import random
import re
import shutil
import xml.etree.ElementTree as ET
from dataclasses import asdict, dataclass
from pathlib import Path

from fde_board import (
    BoardCase,
    CommandError,
    default_resource_root,
    find_probe_command,
    probe_bitstream,
    repo_root,
    run_command,
    run_rust_impl,
)

BOARD_CLK_PATTERN = 0x0004
BOARD_RST_PATTERN = 0x0008
PROBE_REPEAT_WORDS = 64
DEFAULT_PROBE_SEGMENTS: tuple[str, ...] = (
    "0x0008*64",
    "0x000c*64",
    "0x0008*64",
    "0x0000*64",
    "0x0004*64",
    "0x0000*64",
    "0x0004*64",
    "0x0000*64",
    "0x0004*64",
    "0x0000*64",
    "0x0004*64",
    "0x0000*64",
    "0x0004*64",
    "0x0000*64",
    "0x0004*64",
)
TAIL_IDLE_PATTERN = 0x0000
IGNORED_SEGMENT_INDEXES = frozenset({0})
SIM_OUTPUT_RE = re.compile(r"sim_output\[(\d+)\]=0x([0-9a-fx]+)")
CONSTRAINTS = {
    "clk": "P77",
    "rst": "P152",
    "y0": "P7",
    "y1": "P6",
    "y2": "P5",
    "y3": "P4",
}


@dataclass(frozen=True, slots=True)
class RandomCircuitSpec:
    width: int
    seed_value: int
    shift_left: int
    shift_right: int
    shift_mix: int
    feedback_mask_a: int
    feedback_mask_b: int
    mask_a: int
    mask_b: int
    mask_c: int
    mask_d: int
    const_a: int
    const_b: int
    output_masks: tuple[int, int, int, int]
    output_inverts: tuple[bool, bool, bool, bool]

    @property
    def full_mask(self) -> int:
        return (1 << self.width) - 1


@dataclass(frozen=True, slots=True)
class RandomCaseArtifact:
    name: str
    module_name: str
    seed: int
    case_dir: Path
    verilog_path: Path
    constraints_path: Path
    edif_path: Path
    spec_path: Path
    synth_log_path: Path


@dataclass(frozen=True, slots=True)
class CaseResult:
    name: str
    seed: int
    passed: bool
    expected_outputs: list[str | None]
    actual_outputs: list[str]
    case_dir: Path
    message: str


class RandomCircuitGenerationError(RuntimeError):
    pass


def rotate_left(value: int, width: int, shift: int) -> int:
    mask = (1 << width) - 1
    return ((value << shift) | (value >> (width - shift))) & mask


def rotate_right(value: int, width: int, shift: int) -> int:
    mask = (1 << width) - 1
    return ((value >> shift) | (value << (width - shift))) & mask


def parity(value: int) -> int:
    return value.bit_count() & 1


def format_hex(value: int, width: int) -> str:
    digits = max(1, (width + 3) // 4)
    return f"{width}'h{value:0{digits}x}"


def parse_segment_pattern(segment: str) -> int:
    pattern_text, _, _ = segment.partition("*")
    return int(pattern_text, 16)


def wave_patterns(probe_segments: tuple[str, ...]) -> tuple[int, ...]:
    return tuple(parse_segment_pattern(segment) for segment in probe_segments) + (
        TAIL_IDLE_PATTERN,
    )


def next_state(spec: RandomCircuitSpec, state: int) -> int:
    feedback_a = parity(state & spec.feedback_mask_a)
    feedback_b = parity(state & spec.feedback_mask_b)
    rotl = rotate_left(state, spec.width, spec.shift_left)
    rotr = rotate_right(state, spec.width, spec.shift_right)
    shifted = (state << spec.shift_mix) & spec.full_mask
    mix_a = rotl ^ (state & spec.mask_a) ^ spec.const_a
    mix_b = (rotr | spec.mask_b) ^ shifted ^ (spec.mask_c if feedback_a else 0)
    mix_c = (state & spec.mask_d) if feedback_b else ((~state) & spec.mask_d & spec.full_mask)
    return (mix_a ^ mix_b ^ mix_c ^ spec.const_b) & spec.full_mask


def output_nibble(spec: RandomCircuitSpec, state: int) -> int:
    value = 0
    for index, (mask, invert) in enumerate(zip(spec.output_masks, spec.output_inverts)):
        bit = parity(state & mask) ^ int(invert)
        value |= bit << index
    return value


def simulate_outputs(spec: RandomCircuitSpec, probe_segments: tuple[str, ...]) -> tuple[list[str], list[str | None], list[int]]:
    state = 0
    prev_clk = False
    outputs: list[str] = []
    checked_outputs: list[str | None] = []
    states: list[int] = []

    for index, pattern in enumerate(wave_patterns(probe_segments)):
        clk = bool(pattern & BOARD_CLK_PATTERN)
        rst = bool(pattern & BOARD_RST_PATTERN)
        if not prev_clk and clk:
            state = spec.seed_value if rst else next_state(spec, state)
        nibble = output_nibble(spec, state)
        outputs.append(f"0x{nibble:x}")
        checked_outputs.append(None if index in IGNORED_SEGMENT_INDEXES else f"0x{nibble:x}")
        states.append(state)
        prev_clk = clk

    return outputs, checked_outputs, states


def ensure_nontrivial_behavior(spec: RandomCircuitSpec, states: list[int], outputs: list[str | None]) -> None:
    checked_values = [value for value in outputs if value is not None]
    distinct_outputs = len(set(checked_values))
    if distinct_outputs < 4:
        raise RandomCircuitGenerationError(
            f"expected at least 4 distinct outputs, saw {distinct_outputs}"
        )

    state_transitions = sum(1 for lhs, rhs in zip(states, states[1:]) if lhs != rhs)
    if state_transitions < 4:
        raise RandomCircuitGenerationError(
            f"expected at least 4 state transitions, saw {state_transitions}"
        )

    flip_mask = 0
    for lhs, rhs in zip(states, states[1:]):
        flip_mask |= lhs ^ rhs
    if flip_mask.bit_count() < max(4, spec.width // 2):
        raise RandomCircuitGenerationError(
            "not enough state-bit activity in the probe window"
        )


def covered_output_masks(width: int, rng: random.Random) -> tuple[int, int, int, int]:
    masks = [0, 0, 0, 0]
    for bit in range(width):
        chosen = rng.randrange(4)
        masks[chosen] |= 1 << bit
        for index in range(4):
            if index != chosen and rng.random() < 0.35:
                masks[index] |= 1 << bit
    return tuple(mask or (1 << (index % width)) for index, mask in enumerate(masks))


def nonzero_mask(width: int, rng: random.Random) -> int:
    mask = 0
    while mask == 0:
        mask = rng.getrandbits(width)
    return mask


def generate_spec(width: int, rng: random.Random) -> RandomCircuitSpec:
    return RandomCircuitSpec(
        width=width,
        seed_value=nonzero_mask(width, rng),
        shift_left=rng.randrange(1, width),
        shift_right=rng.randrange(1, width),
        shift_mix=rng.randrange(1, width),
        feedback_mask_a=nonzero_mask(width, rng),
        feedback_mask_b=nonzero_mask(width, rng),
        mask_a=nonzero_mask(width, rng),
        mask_b=rng.getrandbits(width),
        mask_c=nonzero_mask(width, rng),
        mask_d=nonzero_mask(width, rng),
        const_a=rng.getrandbits(width),
        const_b=rng.getrandbits(width),
        output_masks=covered_output_masks(width, rng),
        output_inverts=tuple(bool(rng.getrandbits(1)) for _ in range(4)),
    )


def generate_active_spec(
    rng: random.Random,
    width_range: range,
    probe_segments: tuple[str, ...],
    max_attempts: int,
) -> tuple[RandomCircuitSpec, list[str], list[str | None]]:
    last_error: str | None = None
    for _ in range(max_attempts):
        spec = generate_spec(rng.choice(tuple(width_range)), rng)
        outputs, checked_outputs, states = simulate_outputs(spec, probe_segments)
        try:
            ensure_nontrivial_behavior(spec, states, checked_outputs)
        except RandomCircuitGenerationError as error:
            last_error = str(error)
            continue
        return spec, outputs, checked_outputs

    raise SystemExit(
        "could not generate a sufficiently active random circuit"
        + (f": {last_error}" if last_error else "")
    )


def verilog_for_case(module_name: str, spec: RandomCircuitSpec) -> str:
    width = spec.width
    return f"""module {module_name}(\n    input clk,\n    input rst,\n    output y0,\n    output y1,\n    output y2,\n    output y3\n);\n  localparam integer WIDTH = {width};\n  localparam [WIDTH-1:0] FULL_MASK = {format_hex(spec.full_mask, width)};\n  reg [WIDTH-1:0] state;\n  wire feedback_a = ^(state & {format_hex(spec.feedback_mask_a, width)});\n  wire feedback_b = ^(state & {format_hex(spec.feedback_mask_b, width)});\n  wire [WIDTH-1:0] rotl = {{state[WIDTH-{spec.shift_left}-1:0], state[WIDTH-1:WIDTH-{spec.shift_left}]}};\n  wire [WIDTH-1:0] rotr = {{state[{spec.shift_right}-1:0], state[WIDTH-1:{spec.shift_right}]}};\n  wire [WIDTH-1:0] shifted = (state << {spec.shift_mix}) & FULL_MASK;\n  wire [WIDTH-1:0] mix_a = rotl ^ (state & {format_hex(spec.mask_a, width)}) ^ {format_hex(spec.const_a, width)};\n  wire [WIDTH-1:0] mix_b = (rotr | {format_hex(spec.mask_b, width)}) ^ shifted ^ ({{WIDTH{{feedback_a}}}} & {format_hex(spec.mask_c, width)});\n  wire [WIDTH-1:0] mix_c = feedback_b ? (state & {format_hex(spec.mask_d, width)}) : ((~state) & {format_hex(spec.mask_d, width)});\n  wire [WIDTH-1:0] next_state = (mix_a ^ mix_b ^ mix_c ^ {format_hex(spec.const_b, width)}) & FULL_MASK;\n\n  always @(posedge clk) begin\n    if (rst) begin\n      state <= {format_hex(spec.seed_value, width)};\n    end else begin\n      state <= next_state;\n    end\n  end\n\n  assign y0 = ^(state & {format_hex(spec.output_masks[0], width)}) ^ 1'b{int(spec.output_inverts[0])};\n  assign y1 = ^(state & {format_hex(spec.output_masks[1], width)}) ^ 1'b{int(spec.output_inverts[1])};\n  assign y2 = ^(state & {format_hex(spec.output_masks[2], width)}) ^ 1'b{int(spec.output_inverts[2])};\n  assign y3 = ^(state & {format_hex(spec.output_masks[3], width)}) ^ 1'b{int(spec.output_inverts[3])};\nendmodule\n"""


def write_constraints(case_name: str, path: Path) -> None:
    root = ET.Element("design", {"name": case_name})
    for port_name, position in CONSTRAINTS.items():
        ET.SubElement(root, "port", {"name": port_name, "position": position})
    tree = ET.ElementTree(root)
    if hasattr(ET, "indent"):
        ET.indent(tree, space="  ")
    tree.write(path, encoding="UTF-8", xml_declaration=True)


def can_run_iverilog() -> bool:
    return shutil.which("iverilog") is not None and shutil.which("vvp") is not None


def verilog_testbench(module_name: str, probe_segments: tuple[str, ...]) -> str:
    patterns = [f"4'h{pattern:01x}" for pattern in wave_patterns(probe_segments)]
    assigns = "\n    ".join(
        f"patterns[{index}] = {pattern};" for index, pattern in enumerate(patterns)
    )
    return f"""`timescale 1ns/1ps
module tb;
  reg clk = 0;
  reg rst = 0;
  wire y0;
  wire y1;
  wire y2;
  wire y3;
  integer i;
  reg [3:0] patterns [0:{len(patterns) - 1}];

  {module_name} dut(
    .clk(clk),
    .rst(rst),
    .y0(y0),
    .y1(y1),
    .y2(y2),
    .y3(y3)
  );

  initial begin
    {assigns}
    #1;
    for (i = 0; i < {len(patterns)}; i = i + 1) begin
      rst = patterns[i][3];
      clk = patterns[i][2];
      #1;
      $display("sim_output[%0d]=0x%0h", i, {{y3, y2, y1, y0}});
    end
    $finish;
  end
endmodule
"""


def parse_sim_outputs(text: str, expected_len: int) -> list[str | None]:
    values: list[str | None] = [None] * expected_len
    for index_text, raw_value in SIM_OUTPUT_RE.findall(text):
        index = int(index_text)
        if index >= expected_len:
            continue
        if "x" in raw_value.lower():
            values[index] = None
        else:
            values[index] = f"0x{raw_value.lower()}"
    return values


def verify_python_model_against_iverilog(
    root: Path,
    module_name: str,
    verilog_path: Path,
    probe_segments: tuple[str, ...],
    expected_outputs: list[str | None],
    case_dir: Path,
) -> None:
    if not can_run_iverilog():
        return

    tb_path = case_dir / "golden_tb.v"
    tb_path.write_text(verilog_testbench(module_name, probe_segments))
    compile_result = run_command(
        ["iverilog", "-g2012", "-o", str(case_dir / "golden_tb.out"), str(verilog_path), str(tb_path)],
        root,
        case_dir / "iverilog.compile.log",
    )
    if compile_result.returncode != 0:
        raise CommandError("iverilog compile", compile_result, case_dir / "iverilog.compile.log")

    run_result = run_command(
        ["vvp", str(case_dir / "golden_tb.out")],
        root,
        case_dir / "iverilog.run.log",
    )
    if run_result.returncode != 0:
        raise CommandError("iverilog run", run_result, case_dir / "iverilog.run.log")

    sim_outputs = parse_sim_outputs(run_result.output, len(expected_outputs))
    mismatch = compare_outputs(expected_outputs, [value or "x" for value in sim_outputs])
    if mismatch:
        raise RandomCircuitGenerationError(
            f"python model does not match iverilog simulation: {mismatch}"
        )


def write_case_files(
    root: Path,
    out_root: Path,
    index: int,
    master_seed: int,
    probe_segments: tuple[str, ...],
    width_range: range,
    generation_attempts: int,
) -> tuple[RandomCaseArtifact, BoardCase, list[str | None]]:
    case_seed = master_seed + index * 0x9E3779B97F4A7C15
    rng = random.Random(case_seed)
    case_name = f"random-diff-{index:03d}-{case_seed & 0xffff_ffff:08x}"
    module_name = case_name.replace("-", "_")
    case_dir = out_root / case_name
    if case_dir.exists():
        shutil.rmtree(case_dir)
    case_dir.mkdir(parents=True, exist_ok=True)

    spec, _, checked_outputs = generate_active_spec(
        rng,
        width_range,
        probe_segments,
        generation_attempts,
    )

    verilog_path = case_dir / f"{case_name}.v"
    verilog_path.write_text(verilog_for_case(module_name, spec))

    constraints_path = case_dir / "constraints.xml"
    write_constraints(case_name, constraints_path)

    edif_path = case_dir / f"{case_name}.edf"
    synth_log_path = case_dir / "synth.log"
    synth_result = run_command(
        [
            "python3",
            str(root / "scripts" / "synth_yosys_fde.py"),
            "--top",
            module_name,
            "--out-edf",
            str(edif_path),
            "--log",
            str(synth_log_path),
            str(verilog_path),
        ],
        root,
        case_dir / "synth.stdout.log",
    )
    if synth_result.returncode != 0:
        raise CommandError(f"{case_name}: synth", synth_result, case_dir / "synth.stdout.log")

    verify_python_model_against_iverilog(root, module_name, verilog_path, probe_segments, checked_outputs, case_dir)

    spec_path = case_dir / "spec.json"
    spec_payload = {
        "name": case_name,
        "seed": case_seed,
        "module_name": module_name,
        "probe_segments": list(probe_segments),
        "expected_outputs": checked_outputs,
        "spec": asdict(spec),
    }
    spec_path.write_text(json.dumps(spec_payload, indent=2, sort_keys=True))

    board_case = BoardCase(
        name=case_name,
        edf=edif_path,
        constraints=constraints_path,
        expected_outputs=tuple(value or "SKIP" for value in checked_outputs),
        probe_segments=probe_segments,
    )
    artifact = RandomCaseArtifact(
        name=case_name,
        module_name=module_name,
        seed=case_seed,
        case_dir=case_dir,
        verilog_path=verilog_path,
        constraints_path=constraints_path,
        edif_path=edif_path,
        spec_path=spec_path,
        synth_log_path=synth_log_path,
    )
    return artifact, board_case, checked_outputs


def compare_outputs(expected: list[str | None], actual: list[str]) -> str | None:
    if len(actual) != len(expected):
        return f"expected {len(expected)} segments but saw {len(actual)}"

    mismatches = [
        f"segment[{index}] expected {exp} got {act}"
        for index, (exp, act) in enumerate(zip(expected, actual))
        if exp is not None and exp != act
    ]
    if mismatches:
        return "; ".join(mismatches[:4])
    return None


def default_out_root(root: Path) -> Path:
    return root / "build" / "random-board-diff"


def run_case(
    root: Path,
    artifact: RandomCaseArtifact,
    board_case: BoardCase,
    expected_outputs: list[str | None],
    resource_root: Path,
    probe_command: list[str],
) -> CaseResult:
    impl_dir = artifact.case_dir / "impl"
    bitstream = run_rust_impl(root, board_case, resource_root, impl_dir)
    actual_outputs = probe_bitstream(
        root,
        bitstream,
        probe_command,
        board_case.probe_segments,
        impl_dir / "wave_probe.log",
    )
    mismatch = compare_outputs(expected_outputs, actual_outputs)
    if mismatch:
        return CaseResult(
            name=artifact.name,
            seed=artifact.seed,
            passed=False,
            expected_outputs=expected_outputs,
            actual_outputs=actual_outputs,
            case_dir=artifact.case_dir,
            message=mismatch,
        )
    return CaseResult(
        name=artifact.name,
        seed=artifact.seed,
        passed=True,
        expected_outputs=expected_outputs,
        actual_outputs=actual_outputs,
        case_dir=artifact.case_dir,
        message="matched golden model",
    )


def build_parser() -> argparse.ArgumentParser:
    root = repo_root()
    parser = argparse.ArgumentParser(
        description="Generate random sequential circuits, compile them with fde-rs, and diff board outputs against a software golden model."
    )
    parser.add_argument(
        "--count",
        type=int,
        default=5,
        help="Number of random circuits to generate and test.",
    )
    parser.add_argument(
        "--start-index",
        type=int,
        default=0,
        help="Starting case index, useful for reproducing a previously found counterexample.",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=20260322,
        help="Master RNG seed.",
    )
    parser.add_argument(
        "--min-width",
        type=int,
        default=8,
        help="Minimum sequential state width.",
    )
    parser.add_argument(
        "--max-width",
        type=int,
        default=16,
        help="Maximum sequential state width.",
    )
    parser.add_argument(
        "--generation-attempts",
        type=int,
        default=128,
        help="Maximum rejected random specs before giving up on a case.",
    )
    parser.add_argument(
        "--resource-root",
        default=str(default_resource_root(root)),
        help="Path to the full hardware resource bundle.",
    )
    parser.add_argument(
        "--out-root",
        default=str(default_out_root(root)),
        help="Directory for generated random circuits and implementation artifacts.",
    )
    parser.add_argument(
        "--wave-probe",
        help="Explicit wave_probe command or binary path.",
    )
    parser.add_argument(
        "--keep-going",
        action="store_true",
        help="Continue testing remaining random circuits after a failure.",
    )
    return parser


def main() -> int:
    root = repo_root()
    args = build_parser().parse_args()
    if args.count <= 0:
        raise SystemExit("--count must be positive")
    if args.start_index < 0:
        raise SystemExit("--start-index must be non-negative")
    if args.min_width < 4:
        raise SystemExit("--min-width must be at least 4")
    if args.max_width < args.min_width:
        raise SystemExit("--max-width must be greater than or equal to --min-width")

    resource_root = Path(args.resource_root).resolve()
    out_root = Path(args.out_root).resolve()
    out_root.mkdir(parents=True, exist_ok=True)
    probe_command = find_probe_command(root, args.wave_probe)
    probe_segments = DEFAULT_PROBE_SEGMENTS
    width_range = range(args.min_width, args.max_width + 1)

    print(f"seed={args.seed}")
    print(f"probe={' '.join(probe_command)}")
    print(f"out_root={out_root}")

    failures: list[CaseResult] = []
    results: list[CaseResult] = []
    for index in range(args.start_index, args.start_index + args.count):
        try:
            artifact, board_case, expected_outputs = write_case_files(
                root,
                out_root,
                index,
                args.seed,
                probe_segments,
                width_range,
                args.generation_attempts,
            )
            result = run_case(
                root,
                artifact,
                board_case,
                expected_outputs,
                resource_root,
                probe_command,
            )
        except CommandError as error:
            result = CaseResult(
                name=f"random-diff-{index:03d}",
                seed=args.seed,
                passed=False,
                expected_outputs=[],
                actual_outputs=[],
                case_dir=out_root / f"random-diff-{index:03d}",
                message=str(error),
            )

        results.append(result)
        status = "PASS" if result.passed else "FAIL"
        print(f"{status} {result.name}: {result.message}")
        print(f"  case_dir={result.case_dir}")
        if not result.passed:
            failures.append(result)
            if not args.keep_going:
                break

    summary_path = out_root / "summary.json"
    summary_payload = {
        "seed": args.seed,
        "probe_segments": list(probe_segments),
        "results": [
            {
                "name": result.name,
                "seed": result.seed,
                "passed": result.passed,
                "message": result.message,
                "case_dir": str(result.case_dir),
                "expected_outputs": result.expected_outputs,
                "actual_outputs": result.actual_outputs,
            }
            for result in results
        ],
    }
    summary_path.write_text(json.dumps(summary_payload, indent=2, sort_keys=True))
    print(f"summary={summary_path}")

    if failures:
        print(f"{len(failures)} random case(s) failed")
        return 1

    print(f"all {len(results)} random case(s) matched the golden model")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
