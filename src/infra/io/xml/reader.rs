use super::{
    helpers::is_physical_site_module_ref, mapped_xml::load_fde_mapped_design_xml,
    physical_import::load_fde_physical_design_xml,
};
use anyhow::{Context, Result, bail};
use roxmltree::Document;

pub(super) fn load_design_xml(xml: &str) -> Result<crate::ir::Design> {
    let doc = Document::parse(xml).context("failed to parse design xml")?;
    let root = doc.root_element();
    if !root.has_tag_name("design") {
        bail!("root element is not <design>");
    }

    if is_physical_design_xml(root) {
        return load_fde_physical_design_xml(root);
    }
    load_fde_mapped_design_xml(root)
}

fn is_physical_design_xml(root: roxmltree::Node<'_, '_>) -> bool {
    root.descendants().any(|node| {
        node.has_tag_name("instance")
            && matches!(
                node.attribute("moduleRef"),
                Some(module_ref) if is_physical_site_module_ref(module_ref)
            )
    })
}

#[cfg(test)]
mod tests {
    use super::load_design_xml;

    #[test]
    fn mapped_xml_with_nonphysical_known_module_ref_stays_on_mapped_loader() {
        let xml = r#"
<design name="mapped_known_module_ref">
  <external name="work_lib">
    <module name="BLOCKRAM_1" type="GENERIC">
      <port name="CLK" direction="input" capacitance="0.00000"/>
      <port name="DO0" direction="output" capacitance="0.00000"/>
    </module>
  </external>
  <library name="work_lib">
    <module name="mapped_known_module_ref" type="GENERIC">
      <contents>
        <instance name="ram0" moduleRef="BLOCKRAM_1" libraryRef="work_lib"/>
      </contents>
    </module>
  </library>
  <topModule libraryRef="work_lib" name="mapped_known_module_ref"/>
</design>
"#;

        let design = load_design_xml(xml).expect("mapped xml should load");
        assert_eq!(design.stage, "mapped");
        assert_eq!(
            design.metadata.notes,
            vec!["Imported FDE mapped XML".to_string()]
        );
        assert!(design.cells.is_empty());
        assert!(design.nets.is_empty());
    }
}
