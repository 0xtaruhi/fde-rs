use anyhow::{Context, Result};
use std::{fs, sync::Arc};

use crate::{
    bitgen::{self, BitgenOptions},
    cil::load_cil,
    constraints::load_constraints,
    device::lower_design,
    io::save_design,
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
    report::{FlowArtifacts, build_report, write_report},
    resources::resolve_resources,
};

pub(crate) fn run(options: &ImplementationOptions) -> Result<ImplementationReport> {
    fs::create_dir_all(&options.out_dir)
        .with_context(|| format!("failed to create {}", options.out_dir.display()))?;

    let resources = resolve_resources(options)?;

    let constraints = match options.constraints.as_deref() {
        Some(path) => Arc::<[_]>::from(load_constraints(path)?),
        None => Arc::from([]),
    };
    let arch = Arc::new(load_arch(&resources.arch)?);
    let delay_model = load_delay_model(resources.delay.as_deref())?.map(Arc::new);
    let artifacts = FlowArtifacts::modern(&options.out_dir);

    let input_design = map::load_input(&options.input)?;
    let map_result = map::run(
        input_design,
        &MapOptions {
            lut_size: options.lut_size,
            cell_library: resources.dc_cell.clone(),
            emit_structural_verilog: false,
        },
    )?;
    save_design(&map_result.value.design, &artifacts.map)?;

    let pack_result = pack::run(
        map_result.value.design,
        &PackOptions {
            family: options.family.clone(),
            capacity: options.pack_capacity,
            cell_library: resources.pack_cell.clone(),
            dcp_library: resources.pack_lib.clone(),
            config: resources.pack_config.clone(),
        },
    )?;
    save_design(&pack_result.value, &artifacts.pack)?;

    let place_result = place::run(
        pack_result.value,
        &PlaceOptions {
            arch: Arc::clone(&arch),
            delay: delay_model.clone(),
            constraints: Arc::clone(&constraints),
            mode: options.place_mode,
            seed: options.seed,
        },
    )?;
    save_design(&place_result.value, &artifacts.place)?;

    let route_result = route::run(
        place_result.value,
        &RouteOptions {
            arch: Arc::clone(&arch),
            constraints: Arc::clone(&constraints),
            mode: options.route_mode,
        },
    )?;
    save_design(&route_result.value, &artifacts.route)?;

    let mut loaded_cil = None;
    let mut device_design = None;
    if let (Some(cil_path), Some(device_path)) = (resources.cil.as_ref(), artifacts.device.as_ref())
    {
        let cil = load_cil(cil_path)?;
        let device = lower_design(
            route_result.value.clone(),
            arch.as_ref(),
            Some(&cil),
            constraints.as_ref(),
        )?;
        fs::write(device_path, serde_json::to_string_pretty(&device)?)
            .with_context(|| format!("failed to write {}", device_path.display()))?;
        loaded_cil = Some(cil);
        device_design = Some(device);
    }

    let mut sta_result = sta::run(
        route_result.value,
        &StaOptions {
            arch: Some(Arc::clone(&arch)),
            delay: delay_model.clone(),
        },
    )?;
    if let Some(sta_lib) = resources.sta_lib.as_ref() {
        sta_result
            .report
            .push(format!("Referenced timing library {}", sta_lib.display()));
    }
    save_design(&sta_result.value.design, &artifacts.sta)?;
    fs::write(&artifacts.sta_report, &sta_result.value.report_text)
        .with_context(|| format!("failed to write {}", artifacts.sta_report.display()))?;

    let bitgen_result = bitgen::run(
        sta_result.value.design.clone(),
        &BitgenOptions {
            arch_name: Some(arch.name.clone()),
            arch_path: Some(resources.arch.clone()),
            cil_path: resources.cil.clone(),
            cil: loaded_cil,
            device_design,
        },
    )?;
    fs::write(&artifacts.bitstream, &bitgen_result.value.bytes)
        .with_context(|| format!("failed to write {}", artifacts.bitstream.display()))?;
    fs::write(
        &artifacts.bitstream_sidecar,
        &bitgen_result.value.sidecar_text,
    )
    .with_context(|| format!("failed to write {}", artifacts.bitstream_sidecar.display()))?;

    let report = build_report(
        sta_result.value.design.name.clone(),
        &options.out_dir,
        options.seed,
        &artifacts,
        vec![
            map_result.report,
            pack_result.report,
            place_result.report,
            route_result.report,
            sta_result.report,
            bitgen_result.report,
        ],
        sta_result.value.design.timing.clone(),
        Some(bitgen_result.value.sha256.clone()),
    );
    write_report(&artifacts.report, &report)?;
    Ok(report)
}
