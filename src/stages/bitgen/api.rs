use super::{circuit::BitgenCircuit, generator::generate_bitstream};
use crate::{
    cil::Cil,
    ir::{BitstreamImage, Design},
    report::StageOutput,
    route::DeviceRouteImage,
};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct BitgenOptions {
    pub arch_name: Option<String>,
    pub arch_path: Option<PathBuf>,
    pub cil_path: Option<PathBuf>,
    pub cil: Option<Cil>,
    pub device_design: Option<super::DeviceDesign>,
    pub route_image: Option<DeviceRouteImage>,
}

pub fn run(design: Design, options: &BitgenOptions) -> Result<StageOutput<BitstreamImage>> {
    let mut design = design;
    design.infer_slice_bindings_from_route_pips();
    let circuit = BitgenCircuit::from_design(&design);
    generate_bitstream(&circuit, options)
}
