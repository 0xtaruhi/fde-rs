#!/usr/bin/env python3
"""Generate the deterministic dense routing benchmark used by router profiling."""

import argparse
import subprocess
import sys
from pathlib import Path

CONSTRAINTS = """<?xml version="1.0" encoding="UTF-8"?>
<design name="dense_design">
  <port name="clk" position="P77"/>
  <port name="rst" position="P152"/>
  <port name="din[0]" position="P151"/>
  <port name="din[1]" position="P150"/>
  <port name="dout[0]" position="P7"/>
  <port name="dout[1]" position="P6"/>
  <port name="dout[2]" position="P5"/>
  <port name="dout[3]" position="P4"/>
</design>
"""


def verilog(width: int) -> str:
    lines = [
        "module dense_design (",
        "    input wire clk,",
        "    input wire rst,",
        "    input wire [1:0] din,",
        "    output wire [3:0] dout",
        ");",
        f"  reg [{width - 1}:0] state;",
        f"  wire [{width - 1}:0] next_state;",
    ]
    lines += [
        (
            f"  assign next_state[{bit}] = state[{bit}] ^ "
            f"state[{(bit + 1) % width}] ^ "
            f"(state[{(bit + 17) % width}] & din[{bit % 2}]);"
        )
        for bit in range(width)
    ]
    lines += [
        "  always @(posedge clk) begin",
        "    if (rst) state <= 1;",
        "    else state <= next_state;",
        "  end",
    ]
    lines += [
        f"  assign dout[{output}] = ^{{{', '.join(f'state[{bit}]' for bit in range(output, width, 4))}}};"
        for output in range(4)
    ]
    return "\n".join(lines + ["endmodule", ""])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--width", type=int, default=192)
    parser.add_argument("--out-dir", default="build/dense-benchmark")
    parser.add_argument("--synthesize", action="store_true")
    args = parser.parse_args()
    if args.width < 32:
        parser.error("--width must be >= 32")

    root = Path(__file__).resolve().parents[1]
    out_dir = (root / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    source = out_dir / f"dense_{args.width}.v"
    constraints = out_dir / "constraints.xml"
    edif = source.with_suffix(".edf")
    source.write_text(verilog(args.width))
    constraints.write_text(CONSTRAINTS)
    if args.synthesize:
        subprocess.run(
            [
                sys.executable,
                str(root / "scripts/synth_yosys_fde.py"),
                "--top",
                "dense_design",
                "--out-edf",
                str(edif),
                str(source),
            ],
            cwd=root,
            check=True,
        )
    print(f"verilog={source}\nconstraints={constraints}")
    if args.synthesize:
        print(f"edif={edif}")
if __name__ == "__main__":
    main()
