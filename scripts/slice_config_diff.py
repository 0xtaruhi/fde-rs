#!/usr/bin/env python3
"""Compare slice configs between a packed XML and a bitstream sidecar."""

from __future__ import annotations

import argparse
import itertools
import re
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path

SLICE_KEYS = (
    "F",
    "G",
    "FFX",
    "FFY",
    "FXMUX",
    "GYMUX",
    "DXMUX",
    "DYMUX",
    "CKINV",
    "INITX",
    "INITY",
    "SYNC_ATTR",
)

DEFAULTS = {
    "F": "#OFF",
    "G": "#OFF",
    "FFX": "#OFF",
    "FFY": "#OFF",
    "FXMUX": "#OFF",
    "GYMUX": "#OFF",
    "DXMUX": "#OFF",
    "DYMUX": "#OFF",
    "CKINV": "#OFF",
    "INITX": "#OFF",
    "INITY": "#OFF",
    "SYNC_ATTR": "#OFF",
}


@dataclass(frozen=True)
class SliceRecord:
    name: str
    cfg: dict[str, str]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare per-slice configuration signatures between a packed XML and a sidecar."
    )
    parser.add_argument("--packed", required=True, type=Path)
    parser.add_argument("--sidecar", required=True, type=Path)
    parser.add_argument("--site-name", default="S0")
    parser.add_argument("--show-limit", type=int, default=10)
    return parser.parse_args()


def lut_expr_to_hex(raw: str) -> str:
    if not raw.startswith("#LUT:D="):
        return raw
    expression = raw[len("#LUT:D=") :]
    python_expression = (
        expression.replace("*", " and ")
        .replace("+", " or ")
        .replace("~", " not ")
    )
    bits: list[int] = []
    for a4, a3, a2, a1 in itertools.product([0, 1], repeat=4):
        env = {
            "A1": bool(a1),
            "A2": bool(a2),
            "A3": bool(a3),
            "A4": bool(a4),
        }
        bits.append(1 if eval(python_expression, {}, env) else 0)  # noqa: S307
    value = sum(bit << index for index, bit in enumerate(bits))
    return f"0x{value:04X}"


def parse_packed_slices(path: Path) -> list[SliceRecord]:
    root = ET.parse(path).getroot()
    records: list[SliceRecord] = []
    for instance in root.findall("./library/module/contents/instance"):
        if instance.attrib.get("moduleRef") != "slice":
            continue
        config = {
            item.attrib["name"]: lut_expr_to_hex(item.attrib["value"])
            for item in instance.findall("config")
        }
        records.append(SliceRecord(instance.attrib["name"], config))
    return records


def parse_sidecar_slices(path: Path, site_name: str) -> list[SliceRecord]:
    tile_pattern = re.compile(r"^TILE (\S+) type=(\S+) @")
    records: list[SliceRecord] = []
    current_tile: str | None = None
    current_type: str | None = None
    current_cfg: dict[str, str] = {}
    with path.open() as handle:
        for raw_line in handle:
            line = raw_line.rstrip("\n")
            if line.startswith("TILE "):
                if current_type == "CENTER" and current_cfg:
                    records.append(SliceRecord(current_tile or "<unknown>", current_cfg))
                match = tile_pattern.match(line)
                if match is None:
                    raise ValueError(f"unrecognized TILE line: {line}")
                current_tile, current_type = match.groups()
                current_cfg = {}
                continue
            if not line.startswith("CFG ") or current_type != "CENTER":
                continue
            _, site, config_entry = line.split(" ", 2)
            if site != site_name:
                continue
            key, value = config_entry.split("=", 1)
            current_cfg[key] = value
    if current_type == "CENTER" and current_cfg:
        records.append(SliceRecord(current_tile or "<unknown>", current_cfg))
    return records


def lut_multiset(records: list[SliceRecord]) -> Counter[str]:
    counts: Counter[str] = Counter()
    for record in records:
        for key in ("F", "G"):
            counts[record.cfg.get(key, "#OFF")] += 1
    return counts


def normalize_signature(cfg: dict[str, str]) -> tuple[tuple[str, str], ...]:
    normalized = {}
    for key in SLICE_KEYS:
        if key in cfg:
            normalized[key] = cfg[key]
        elif key in DEFAULTS:
            normalized[key] = DEFAULTS[key]
    return tuple((key, normalized[key]) for key in SLICE_KEYS)


def signature_text(signature: tuple[tuple[str, str], ...]) -> str:
    return " ".join(f"{key}={value}" for key, value in signature)


def print_lut_summary(packed_records: list[SliceRecord], sidecar_records: list[SliceRecord]) -> None:
    packed_luts = lut_multiset(packed_records)
    sidecar_luts = lut_multiset(sidecar_records)
    print("== LUT init multiset ==")
    if packed_luts == sidecar_luts:
        print("MATCH")
        return
    print("MISMATCH")
    print("packed-only:", dict(packed_luts - sidecar_luts))
    print("sidecar-only:", dict(sidecar_luts - packed_luts))


def print_signature_summary(
    packed_records: list[SliceRecord], sidecar_records: list[SliceRecord], show_limit: int
) -> None:
    packed_by_signature: dict[tuple[tuple[str, str], ...], list[str]] = defaultdict(list)
    sidecar_by_signature: dict[tuple[tuple[str, str], ...], list[str]] = defaultdict(list)
    for record in packed_records:
        packed_by_signature[normalize_signature(record.cfg)].append(record.name)
    for record in sidecar_records:
        sidecar_by_signature[normalize_signature(record.cfg)].append(record.name)

    packed_counts = Counter(
        {signature: len(names) for signature, names in packed_by_signature.items()}
    )
    sidecar_counts = Counter(
        {signature: len(names) for signature, names in sidecar_by_signature.items()}
    )
    packed_only = packed_counts - sidecar_counts
    sidecar_only = sidecar_counts - packed_counts

    print()
    print("== Normalized per-slice signature ==")
    print(
        "Rules: fill missing inactive controls as #OFF, compare F/G/FF/DXMUX/DYMUX/"
        "CKINV/INIT/SYNC_ATTR."
    )
    print(f"packed slices: {len(packed_records)}")
    print(f"sidecar slices: {len(sidecar_records)}")
    print(f"packed-only total: {sum(packed_only.values())}")
    print(f"sidecar-only total: {sum(sidecar_only.values())}")

    if not packed_only and not sidecar_only:
        print("MATCH")
        return

    if packed_only:
        print()
        print("packed-only signatures:")
        for signature, count in packed_only.most_common(show_limit):
            examples = ",".join(packed_by_signature[signature][:count])
            print(f"  count={count} instances={examples}")
            print(f"    {signature_text(signature)}")

    if sidecar_only:
        print()
        print("sidecar-only signatures:")
        for signature, count in sidecar_only.most_common(show_limit):
            examples = ",".join(sidecar_by_signature[signature][:count])
            print(f"  count={count} tiles={examples}")
            print(f"    {signature_text(signature)}")


def main() -> int:
    args = parse_args()
    packed_records = parse_packed_slices(args.packed)
    sidecar_records = parse_sidecar_slices(args.sidecar, args.site_name)
    if not packed_records:
        print("error: no slice instances found in packed XML", file=sys.stderr)
        return 1
    if not sidecar_records:
        print("error: no slice configs found in sidecar", file=sys.stderr)
        return 1
    print_lut_summary(packed_records, sidecar_records)
    print_signature_summary(packed_records, sidecar_records, args.show_limit)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
