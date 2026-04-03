mod helpers;
mod lut_expr;
mod mapped_xml;
mod physical;
mod physical_import;
mod reader;
mod writer;

pub(crate) fn load_design_xml(xml: &str) -> anyhow::Result<crate::ir::Design> {
    reader::load_design_xml(xml)
}

pub(crate) fn save_fde_design_xml_with_context(
    design: &crate::ir::Design,
    context: &crate::io::DesignWriteContext<'_>,
) -> anyhow::Result<String> {
    writer::save_design_xml(
        design,
        &writer::XmlWriteContext {
            arch: context.arch,
            _cil: context.cil,
            _constraints: context.constraints,
            cil_path: context.cil_path,
        },
    )
}
