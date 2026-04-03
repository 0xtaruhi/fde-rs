use crate::ir::Design;
use std::fmt::Write;

pub fn export_structural_verilog(design: &Design) -> String {
    let mut output = String::new();
    let index = design.index();
    let port_list = design
        .ports
        .iter()
        .map(|port| port.name.clone())
        .collect::<Vec<_>>();
    let _ = writeln!(output, "module {}({});", design.name, port_list.join(", "));
    for port in &design.ports {
        let _ = writeln!(output, "  {} {};", port.direction.as_str(), port.name);
    }
    for net in &design.nets {
        if index.port_id(&net.name).is_none() {
            let _ = writeln!(output, "  wire {};", net.name);
        }
    }
    for cell in &design.cells {
        let mut pins = Vec::new();
        for input in &cell.inputs {
            pins.push(format!(".{}({})", input.port, input.net));
        }
        for output in &cell.outputs {
            pins.push(format!(".{}({})", output.port, output.net));
        }
        let _ = writeln!(
            output,
            "  {} {} ({});",
            cell.type_name,
            cell.name,
            pins.join(", ")
        );
    }
    let _ = writeln!(output, "endmodule");
    output
}
