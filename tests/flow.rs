use fde::{
    ImplementationOptions, load_arch, load_map_input, resource::ResourceBundle, run_implementation,
};
use serde_json::Value;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(path: &str) -> PathBuf {
    repo_root().join(path)
}

fn external_resource_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("FDE_TEST_RESOURCE_ROOT") {
        let path = PathBuf::from(path);
        if path.join("fdp3p7_arch.xml").is_file() {
            return Some(path);
        }
    }
    ResourceBundle::discover_from(&repo_root())
        .ok()
        .map(|bundle| bundle.root)
}

fn temp_out(name: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let out = dir.path().join(name);
    (dir, out)
}

fn report_json(path: &PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read report")).expect("parse report")
}

fn bitgen_materialization_counts(report: &Value) -> Option<(usize, usize, usize)> {
    let stage = report
        .get("stages")?
        .as_array()?
        .iter()
        .find(|stage| stage.get("stage").and_then(Value::as_str) == Some("bitgen"))?;
    let message = stage
        .get("messages")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|message| message.starts_with("Materialized "))?;
    let numbers = message
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<usize>().ok())
        .collect::<Option<Vec<_>>>()?;
    match numbers.as_slice() {
        [config_bits, tile_images, routed_pips] => Some((*config_bits, *tile_images, *routed_pips)),
        _ => None,
    }
}

fn line_prefix_count(text: &str, prefix: &str) -> usize {
    text.lines().filter(|line| line.starts_with(prefix)).count()
}

fn fdri_chunks(lines: &[String]) -> Vec<(String, String)> {
    let mut chunks = Vec::new();
    let mut index = 0usize;
    while index + 3 < lines.len() {
        if lines[index] == "3000_2001" && lines[index + 2].starts_with("3000_4000") {
            chunks.push((
                lines[index + 1]
                    .split('\t')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                lines[index + 3]
                    .split('\t')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            ));
            let length = u32::from_str_radix(&lines[index + 3][0..4], 16).expect("fdri high")
                * 65_536
                + u32::from_str_radix(&lines[index + 3][5..9], 16).expect("fdri low")
                - 0x5000_0000;
            index += 4 + length as usize;
        } else {
            index += 1;
        }
    }
    chunks
}

#[test]
fn edif_parser_smoke_test() {
    let design = load_map_input(&fixture("tests/fixtures/simple.edf")).expect("load edif");
    assert_eq!(design.name, "blinky");
    assert_eq!(design.ports.len(), 4);
    assert_eq!(design.cells.len(), 2);
    assert_eq!(design.nets.len(), 5);
}

#[test]
fn edif_parser_resolves_renamed_instance_refs() {
    let design =
        load_map_input(&fixture("tests/fixtures/renamed-instances.edf")).expect("load renamed");
    let comb = design
        .nets
        .iter()
        .find(|net| net.name == "net_comb")
        .expect("comb net");
    let driver = comb.driver.as_ref().expect("comb driver");
    let sink = comb.sinks.first().expect("comb sink");

    assert_eq!(driver.kind, "cell");
    assert_eq!(driver.name, "u_lut");
    assert_eq!(driver.pin, "O");
    assert_eq!(sink.kind, "cell");
    assert_eq!(sink.name, "u_ff");
    assert_eq!(sink.pin, "D");
}

#[test]
fn end_to_end_impl_generates_artifacts() {
    let (_temp, out_dir) = temp_out("impl-run");
    let report = run_implementation(&ImplementationOptions {
        input: fixture("tests/fixtures/simple.edf"),
        out_dir: out_dir.clone(),
        resource_root: Some(fixture("tests/fixtures/hw_lib")),
        constraints: Some(fixture("tests/fixtures/constraints.xml")),
        ..ImplementationOptions::default()
    })
    .expect("implementation run");

    for key in [
        "map",
        "pack",
        "place",
        "route",
        "sta",
        "sta_report",
        "bitstream",
        "bitstream_sidecar",
        "report",
    ] {
        let path = PathBuf::from(report.artifacts.get(key).expect("artifact path"));
        assert!(path.exists(), "missing artifact {key}: {}", path.display());
    }

    assert!(report.timing.is_some());
    assert!(report.bitstream_sha256.is_some());
}

#[test]
fn end_to_end_impl_handles_used_ground_constant_net() {
    let (_temp, out_dir) = temp_out("impl-const-gnd");
    let report = run_implementation(&ImplementationOptions {
        input: fixture("tests/fixtures/const-gnd.edf"),
        out_dir: out_dir.clone(),
        resource_root: Some(fixture("tests/fixtures/hw_lib")),
        constraints: Some(fixture("tests/fixtures/const-gnd-constraints.xml")),
        ..ImplementationOptions::default()
    })
    .expect("implementation run with used ground net");

    let mapped = PathBuf::from(report.artifacts.get("map").expect("map artifact"));
    let route = PathBuf::from(report.artifacts.get("route").expect("route artifact"));
    let bitstream = PathBuf::from(
        report
            .artifacts
            .get("bitstream")
            .expect("bitstream artifact"),
    );
    let sidecar = PathBuf::from(
        report
            .artifacts
            .get("bitstream_sidecar")
            .expect("bitstream sidecar"),
    );
    let mapped_text = fs::read_to_string(&mapped).expect("read mapped design");
    let bitstream_bytes = fs::read(&bitstream).expect("read bitstream");
    let sidecar_text = fs::read_to_string(&sidecar).expect("read bitstream sidecar");

    assert!(route.exists(), "missing route artifact {}", route.display());
    assert!(
        bitstream.exists(),
        "missing bitstream artifact {}",
        bitstream.display()
    );
    assert!(mapped_text.contains("cell name=\"u_gnd\" kind=\"lut\" type_name=\"LUT4\""));
    assert!(mapped_text.contains("driver kind=\"cell\" name=\"u_gnd\" pin=\"O\""));
    assert!(!bitstream_bytes.is_empty(), "bitstream should not be empty");
    assert!(sidecar_text.contains("# Routed Transmission Pips"));
}

#[test]
fn implementation_is_deterministic_for_same_seed() {
    let (_temp_a, out_a) = temp_out("impl-a");
    let (_temp_b, out_b) = temp_out("impl-b");
    let options = ImplementationOptions {
        input: fixture("tests/fixtures/simple.edf"),
        resource_root: Some(fixture("tests/fixtures/hw_lib")),
        constraints: Some(fixture("tests/fixtures/constraints.xml")),
        seed: 12345,
        ..ImplementationOptions::default()
    };
    let report_a = run_implementation(&ImplementationOptions {
        out_dir: out_a.clone(),
        ..options.clone()
    })
    .expect("impl a");
    let report_b = run_implementation(&ImplementationOptions {
        out_dir: out_b.clone(),
        ..options
    })
    .expect("impl b");

    let bit_a = fs::read(report_a.artifacts.get("bitstream").expect("bit a")).expect("read a");
    let bit_b = fs::read(report_b.artifacts.get("bitstream").expect("bit b")).expect("read b");
    let side_a = fs::read_to_string(report_a.artifacts.get("bitstream_sidecar").expect("side a"))
        .expect("read side a");
    let side_b = fs::read_to_string(report_b.artifacts.get("bitstream_sidecar").expect("side b"))
        .expect("read side b");

    assert_eq!(bit_a, bit_b);
    assert_eq!(side_a, side_b);
}

#[test]
fn can_parse_external_arch_when_available() {
    let Some(resource_root) = external_resource_root() else {
        return;
    };
    let arch = load_arch(&resource_root.join("fdp3p7_arch.xml")).expect("load external arch");
    assert!(arch.width > 0);
    assert!(arch.height > 0);
}

#[test]
fn rust_impl_emits_device_and_tile_config_when_external_resources_are_available() {
    let Some(resource_root) = external_resource_root() else {
        return;
    };

    let (_temp, out_dir) = temp_out("impl-rust-tiles");
    let report = run_implementation(&ImplementationOptions {
        input: fixture("tests/fixtures/blinky-yosys.edf"),
        out_dir: out_dir.clone(),
        resource_root: Some(resource_root),
        constraints: Some(fixture("tests/fixtures/fdp3p7-constraints.xml")),
        ..ImplementationOptions::default()
    })
    .expect("rust implementation run");

    let device = PathBuf::from(report.artifacts.get("device").expect("device artifact"));
    let sidecar = PathBuf::from(
        report
            .artifacts
            .get("bitstream_sidecar")
            .expect("bitstream sidecar"),
    );
    let sidecar_text = fs::read_to_string(&sidecar).expect("read sidecar");

    assert!(
        device.exists(),
        "missing device artifact {}",
        device.display()
    );
    assert!(sidecar_text.contains("# Tile Config Image"));
    assert!(sidecar_text.contains("TILE "));
    assert!(sidecar_text.contains("CFG "));
    assert!(sidecar_text.contains("# Routed Transmission Pips"));
    assert!(sidecar_text.contains("PIP "));
    assert!(sidecar_text.contains("BIT GSB_"));
    assert!(!sidecar_text.contains("Unresolved config "));
    assert!(!sidecar_text.contains("Missing site SRAM mapping "));
    assert!(!sidecar_text.contains("routing PIPs are not emitted yet"));
}

#[test]
fn rust_impl_emits_text_bitstream_when_external_resources_are_available() {
    let Some(resource_root) = external_resource_root() else {
        return;
    };

    let (_temp, out_dir) = temp_out("impl-rust-text-bitstream");
    let report = run_implementation(&ImplementationOptions {
        input: fixture("tests/fixtures/blinky-yosys.edf"),
        out_dir: out_dir.clone(),
        resource_root: Some(resource_root),
        constraints: Some(fixture("tests/fixtures/fdp3p7-constraints.xml")),
        ..ImplementationOptions::default()
    })
    .expect("rust implementation run");

    let bitstream = PathBuf::from(
        report
            .artifacts
            .get("bitstream")
            .expect("bitstream artifact"),
    );
    let text = fs::read_to_string(&bitstream).expect("read bitstream");
    let lines = text.lines().map(ToString::to_string).collect::<Vec<_>>();

    assert!(
        lines
            .first()
            .is_some_and(|line| line.contains("// chip_type: fdp3000k"))
    );
    assert!(text.contains("AA99_5566"));
    assert!(text.contains("3000_4000\t//400 words"));
    assert!(text.contains("5000_0190"));
    assert!(!text.contains("FDEBIT24"));
    assert_eq!(lines.len(), 52_672);
    assert_eq!(fdri_chunks(&lines).len(), 69);
}

#[test]
fn complex_external_resource_impl_emits_text_bitstream() {
    let Some(resource_root) = external_resource_root() else {
        return;
    };
    let input = repo_root().join("build/regression-complex/complex8-yosys.edf");
    let constraints = repo_root().join("build/regression-complex/constraints.xml");
    if !resource_root.join("fdp3p7_cil.xml").exists() || !input.exists() || !constraints.exists() {
        return;
    }

    let (_temp, out_dir) = temp_out("impl-rust-complex-text-bitstream");
    let report = run_implementation(&ImplementationOptions {
        input,
        out_dir: out_dir.clone(),
        resource_root: Some(resource_root),
        constraints: Some(constraints),
        ..ImplementationOptions::default()
    })
    .expect("complex rust implementation");

    let bitstream = PathBuf::from(
        report
            .artifacts
            .get("bitstream")
            .expect("bitstream artifact"),
    );
    let text = fs::read_to_string(&bitstream).expect("read bitstream");
    let lines = text.lines().map(ToString::to_string).collect::<Vec<_>>();

    assert!(text.contains("AA99_5566"));
    assert_eq!(lines.len(), 52_672);
    let chunks = fdri_chunks(&lines);
    assert_eq!(chunks.len(), 69);
    assert_eq!(
        chunks.first().map(|chunk| chunk.1.as_str()),
        Some("5000_0190")
    );
    assert_eq!(
        chunks.last().map(|chunk| chunk.1.as_str()),
        Some("5000_0080")
    );

    let report_path = PathBuf::from(report.artifacts.get("report").expect("report artifact"));
    let report_json = report_json(&report_path);
    let Some((config_bits, tile_images, routed_pips)) = bitgen_materialization_counts(&report_json)
    else {
        panic!("missing bitgen materialization counts");
    };
    assert!(config_bits > 0, "expected non-zero config bits");
    assert!(tile_images > 0, "expected non-zero tile images");
    assert!(routed_pips > 0, "expected non-zero routed pips");
}

#[test]
fn complex_external_resource_impl_is_seed_stable() {
    let Some(resource_root) = external_resource_root() else {
        return;
    };
    let input = repo_root().join("build/regression-complex/complex8-yosys.edf");
    let constraints = repo_root().join("build/regression-complex/constraints.xml");
    if !resource_root.join("fdp3p7_cil.xml").exists() || !input.exists() || !constraints.exists() {
        return;
    }

    let (_temp_a, out_a) = temp_out("impl-rust-complex-stable-a");
    let (_temp_b, out_b) = temp_out("impl-rust-complex-stable-b");
    let options = ImplementationOptions {
        input,
        resource_root: Some(resource_root),
        constraints: Some(constraints),
        seed: 0x1234_5678,
        ..ImplementationOptions::default()
    };

    let report_a = run_implementation(&ImplementationOptions {
        out_dir: out_a,
        ..options.clone()
    })
    .expect("complex run a");
    let report_b = run_implementation(&ImplementationOptions {
        out_dir: out_b,
        ..options
    })
    .expect("complex run b");

    let bit_a = fs::read(report_a.artifacts.get("bitstream").expect("bitstream a"))
        .expect("read bitstream a");
    let bit_b = fs::read(report_b.artifacts.get("bitstream").expect("bitstream b"))
        .expect("read bitstream b");
    let side_a = fs::read_to_string(
        report_a
            .artifacts
            .get("bitstream_sidecar")
            .expect("sidecar a"),
    )
    .expect("read sidecar a");
    let side_b = fs::read_to_string(
        report_b
            .artifacts
            .get("bitstream_sidecar")
            .expect("sidecar b"),
    )
    .expect("read sidecar b");
    let report_json_a = report_json(&PathBuf::from(
        report_a.artifacts.get("report").expect("report a"),
    ));
    let report_json_b = report_json(&PathBuf::from(
        report_b.artifacts.get("report").expect("report b"),
    ));

    assert_eq!(bit_a, bit_b);
    assert_eq!(side_a, side_b);
    assert_eq!(report_json_a.get("timing"), report_json_b.get("timing"));
    assert_eq!(
        bitgen_materialization_counts(&report_json_a),
        bitgen_materialization_counts(&report_json_b)
    );
}

#[test]
fn complex_external_resource_sidecar_contains_nontrivial_config_and_route_sections() {
    let Some(resource_root) = external_resource_root() else {
        return;
    };
    let input = repo_root().join("build/regression-complex/complex8-yosys.edf");
    let constraints = repo_root().join("build/regression-complex/constraints.xml");
    if !resource_root.join("fdp3p7_cil.xml").exists() || !input.exists() || !constraints.exists() {
        return;
    }

    let (_temp, out_dir) = temp_out("impl-rust-complex-sidecar");
    let report = run_implementation(&ImplementationOptions {
        input,
        out_dir,
        resource_root: Some(resource_root),
        constraints: Some(constraints),
        ..ImplementationOptions::default()
    })
    .expect("complex sidecar run");

    let sidecar = fs::read_to_string(
        report
            .artifacts
            .get("bitstream_sidecar")
            .expect("sidecar artifact"),
    )
    .expect("read sidecar");

    assert!(line_prefix_count(&sidecar, "TILE ") > 0);
    assert!(line_prefix_count(&sidecar, "CFG ") > 0);
    assert!(line_prefix_count(&sidecar, "BIT ") > 0);
    assert!(line_prefix_count(&sidecar, "PIP ") > 0);

    for unwanted in [
        "could not find a Rust route",
        "Missing ",
        "Owner-tile remap",
        "Unresolved config ",
    ] {
        assert!(
            !sidecar.contains(unwanted),
            "unexpected sidecar warning: {unwanted}"
        );
    }
}
