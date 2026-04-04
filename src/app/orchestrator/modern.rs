use anyhow::{Context, Result};
use std::{collections::BTreeMap, fs, sync::Arc, time::Instant};

use crate::{
    bitgen::{self, BitgenOptions},
    cil::load_cil,
    constraints::load_constraints,
    io::{DesignWriteContext, save_design, save_design_with_context},
    map::{self, MapOptions},
    pack::{self, PackOptions},
    place::{self, PlaceOptions},
    report::ImplementationReport,
    resource::{load_arch, load_delay_model},
    route::{self, RouteOptions},
    sta::{self, StaOptions},
};

use super::{
    options::ImplementationOptions,
    report::{FlowArtifacts, ReportContext, build_report, write_log, write_report, write_summary},
    resources::resolve_resources,
};

pub(crate) fn run(options: &ImplementationOptions) -> Result<ImplementationReport> {
    let flow_started = Instant::now();
    fs::create_dir_all(&options.out_dir)
        .with_context(|| format!("failed to create {}", options.out_dir.display()))?;

    let resources = resolve_resources(options)?;
    let inputs = report_inputs(options);
    let resource_paths = report_resources(options, &resources);

    let constraints = match options.constraints.as_deref() {
        Some(path) => Arc::<[_]>::from(load_constraints(path)?),
        None => Arc::from([]),
    };
    let arch = Arc::new(load_arch(&resources.arch)?);
    let delay_model = load_delay_model(resources.delay.as_deref())?.map(Arc::new);
    let loaded_cil = match resources.cil.as_ref() {
        Some(cil_path) => Some(load_cil(cil_path)?),
        None => None,
    };
    let artifacts = FlowArtifacts::modern(&options.out_dir, options.emit_sidecar);

    let input_design = map::load_input(&options.input)?;
    let map_started = Instant::now();
    let mut map_result = map::run(
        input_design,
        &MapOptions {
            lut_size: options.lut_size,
            cell_library: resources.dc_cell.clone(),
            emit_structural_verilog: false,
        },
    )?;
    map_result.report.set_elapsed(map_started.elapsed());
    save_design(&map_result.value.design, &artifacts.map)?;
    map_result.report.artifact("design", &artifacts.map);

    let pack_started = Instant::now();
    let mut pack_result = pack::run(
        map_result.value.design,
        &PackOptions {
            family: options.family.clone(),
            capacity: options.pack_capacity,
            cell_library: resources.pack_cell.clone(),
            dcp_library: resources.pack_lib.clone(),
            config: resources.pack_config.clone(),
        },
    )?;
    pack_result.report.set_elapsed(pack_started.elapsed());
    save_design(&pack_result.value, &artifacts.pack)?;
    pack_result.report.artifact("design", &artifacts.pack);

    let place_started = Instant::now();
    let mut place_result = place::run(
        pack_result.value,
        &PlaceOptions {
            arch: Arc::clone(&arch),
            delay: delay_model.clone(),
            constraints: Arc::clone(&constraints),
            mode: options.place_mode,
            seed: options.seed,
        },
    )?;
    place_result.report.set_elapsed(place_started.elapsed());
    save_design_with_context(
        &place_result.value,
        &artifacts.place,
        &DesignWriteContext {
            arch: Some(arch.as_ref()),
            constraints: constraints.as_ref(),
            ..DesignWriteContext::default()
        },
    )?;
    place_result.report.artifact("design", &artifacts.place);

    let route_device_design = loaded_cil
        .as_ref()
        .map(|cil| {
            route::lower_design(
                place_result.value.clone(),
                arch.as_ref(),
                Some(cil),
                constraints.as_ref(),
            )
        })
        .transpose()?;
    let route_started = Instant::now();
    let mut route_result = route::run_with_artifacts(
        place_result.value,
        &RouteOptions {
            arch: Arc::clone(&arch),
            arch_path: resources.arch.clone(),
            constraints: Arc::clone(&constraints),
            cil: loaded_cil.clone(),
            device_design: route_device_design,
        },
    )?;
    route_result.report.set_elapsed(route_started.elapsed());
    route_result.report.artifact("design", &artifacts.route);
    if let Some(device_path) = artifacts.device.as_ref() {
        route_result.report.artifact("device_design", device_path);
    }
    let route::RouteStageArtifacts {
        design: routed_design,
        device_design,
        route_image,
    } = route_result.value;
    save_design_with_context(
        &routed_design,
        &artifacts.route,
        &DesignWriteContext {
            arch: Some(arch.as_ref()),
            cil: loaded_cil.as_ref(),
            constraints: constraints.as_ref(),
            cil_path: resources.cil.as_deref(),
        },
    )?;
    if let Some(device_path) = artifacts.device.as_ref() {
        fs::write(device_path, serde_json::to_string_pretty(&device_design)?)
            .with_context(|| format!("failed to write {}", device_path.display()))?;
    }

    let sta_started = Instant::now();
    let mut sta_result = sta::run(
        routed_design,
        &StaOptions {
            arch: Some(Arc::clone(&arch)),
            delay: delay_model.clone(),
        },
    )?;
    sta_result.report.set_elapsed(sta_started.elapsed());
    if let Some(sta_lib) = resources.sta_lib.as_ref() {
        sta_result
            .report
            .push(format!("Referenced timing library {}", sta_lib.display()));
    }
    save_design_with_context(
        &sta_result.value.design,
        &artifacts.sta,
        &DesignWriteContext {
            arch: Some(arch.as_ref()),
            ..DesignWriteContext::default()
        },
    )?;
    sta_result.report.artifact("design", &artifacts.sta);
    fs::write(&artifacts.sta_report, &sta_result.value.report_text)
        .with_context(|| format!("failed to write {}", artifacts.sta_report.display()))?;
    sta_result
        .report
        .artifact("timing_report", &artifacts.sta_report);

    let bitgen_started = Instant::now();
    let mut bitgen_result = bitgen::run(
        sta_result.value.design.clone(),
        &BitgenOptions {
            arch_name: Some(arch.name.clone()),
            arch_path: Some(resources.arch.clone()),
            cil_path: resources.cil.clone(),
            cil: loaded_cil,
            device_design: Some(device_design),
            route_image: Some(route_image),
        },
    )?;
    bitgen_result.report.set_elapsed(bitgen_started.elapsed());
    fs::write(&artifacts.bitstream, &bitgen_result.value.bytes)
        .with_context(|| format!("failed to write {}", artifacts.bitstream.display()))?;
    bitgen_result
        .report
        .metric("bitstream_sha256", bitgen_result.value.sha256.clone());
    bitgen_result
        .report
        .artifact("bitstream", &artifacts.bitstream);
    if let Some(sidecar_path) = artifacts.bitstream_sidecar.as_ref() {
        fs::write(sidecar_path, &bitgen_result.value.sidecar_text)
            .with_context(|| format!("failed to write {}", sidecar_path.display()))?;
        bitgen_result.report.artifact("sidecar", sidecar_path);
    }

    let stages = vec![
        map_result.report,
        pack_result.report,
        place_result.report,
        route_result.report,
        sta_result.report,
        bitgen_result.report,
    ];

    let report = build_report(
        ReportContext {
            flow: "impl".to_string(),
            design: sta_result.value.design.name.clone(),
            out_dir: options.out_dir.clone(),
            seed: options.seed,
            elapsed_ms: flow_started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            inputs,
            resources: resource_paths,
        },
        &artifacts,
        stages,
        sta_result.value.design.timing.clone(),
        Some(bitgen_result.value.sha256.clone()),
    );
    write_report(&artifacts.report, &report)?;
    write_summary(&artifacts.summary, &report)?;
    write_log(&artifacts.log, &report)?;
    Ok(report)
}

fn report_inputs(options: &ImplementationOptions) -> BTreeMap<String, String> {
    let mut inputs = BTreeMap::new();
    inputs.insert("input".to_string(), options.input.display().to_string());
    if let Some(constraints) = options.constraints.as_ref() {
        inputs.insert("constraints".to_string(), constraints.display().to_string());
    }
    if let Some(resource_root) = options.resource_root.as_ref() {
        inputs.insert(
            "resource_root".to_string(),
            resource_root.display().to_string(),
        );
    }
    inputs
}

fn report_resources(
    options: &ImplementationOptions,
    resources: &super::options::ResolvedResources,
) -> BTreeMap<String, String> {
    let mut resolved = BTreeMap::new();
    resolved.insert("arch".to_string(), resources.arch.display().to_string());
    if let Some(delay) = resources.delay.as_ref() {
        resolved.insert("delay".to_string(), delay.display().to_string());
    }
    if let Some(sta_lib) = resources.sta_lib.as_ref() {
        resolved.insert("sta_lib".to_string(), sta_lib.display().to_string());
    }
    if let Some(cil) = resources.cil.as_ref() {
        resolved.insert("cil".to_string(), cil.display().to_string());
    }
    if let Some(dc_cell) = resources.dc_cell.as_ref() {
        resolved.insert("dc_cell".to_string(), dc_cell.display().to_string());
    }
    if let Some(pack_cell) = resources.pack_cell.as_ref() {
        resolved.insert("pack_cell".to_string(), pack_cell.display().to_string());
    }
    if let Some(pack_lib) = resources.pack_lib.as_ref() {
        resolved.insert("pack_lib".to_string(), pack_lib.display().to_string());
    }
    if let Some(pack_config) = resources.pack_config.as_ref() {
        resolved.insert("pack_config".to_string(), pack_config.display().to_string());
    }
    if let Some(family) = options.family.as_ref() {
        resolved.insert("family".to_string(), family.clone());
    }
    resolved
}
