use crate::ir::{Design, DesignIndex};

pub(crate) fn annotate_net_criticality(design: &mut Design) {
    let index = design.index();
    let forward = forward_levels(design, &index);
    let backward = backward_levels(design, &index);
    let max_forward = forward.iter().copied().max().unwrap_or(1) as f64;
    let max_span = forward
        .iter()
        .zip(&backward)
        .map(|(forward_level, backward_level)| forward_level + backward_level)
        .max()
        .unwrap_or(1) as f64;

    for (net, (depth, remaining)) in design
        .nets
        .iter_mut()
        .zip(forward.into_iter().zip(backward.into_iter()))
    {
        let depth = depth as f64;
        let remaining = remaining as f64;
        let span = depth + remaining;
        let fanout = net.sinks.len() as f64;
        let span_score = span / max_span.max(1.0);
        let depth_score = depth / max_forward.max(1.0);
        let fanout_score = (fanout / 8.0).min(1.0);
        net.criticality = 0.65 * span_score + 0.25 * depth_score + 0.10 * fanout_score;
    }
}

fn forward_levels(design: &Design, index: &DesignIndex<'_>) -> Vec<usize> {
    let mut levels = vec![0usize; design.nets.len()];
    let mut changed = true;
    for _ in 0..design.cells.len().max(1) {
        if !changed {
            break;
        }
        changed = false;
        for cell in &design.cells {
            if cell.is_sequential() {
                continue;
            }

            let input_level = cell
                .inputs
                .iter()
                .filter_map(|pin| index.net_id(&pin.net).map(|net_id| levels[net_id.index()]))
                .max()
                .unwrap_or(0);
            for output in &cell.outputs {
                let Some(net_id) = index.net_id(&output.net) else {
                    continue;
                };
                let candidate = input_level + 1;
                if candidate > levels[net_id.index()] {
                    levels[net_id.index()] = candidate;
                    changed = true;
                }
            }
        }
    }

    levels
}

fn backward_levels(design: &Design, index: &DesignIndex<'_>) -> Vec<usize> {
    let mut levels = vec![0usize; design.nets.len()];
    for (net_index, net) in design.nets.iter().enumerate() {
        if net.sinks.iter().any(|sink| {
            index
                .port_for_endpoint(sink)
                .map(|port_id| index.port(design, port_id).direction.is_output_like())
                .unwrap_or(false)
        }) {
            levels[net_index] = 0;
        }
    }

    let mut changed = true;
    for _ in 0..design.cells.len().max(1) {
        if !changed {
            break;
        }
        changed = false;
        for cell in design.cells.iter().rev() {
            if cell.is_sequential() {
                continue;
            }

            let output_level = cell
                .outputs
                .iter()
                .filter_map(|pin| index.net_id(&pin.net).map(|net_id| levels[net_id.index()]))
                .max()
                .unwrap_or(0);
            for input in &cell.inputs {
                let Some(net_id) = index.net_id(&input.net) else {
                    continue;
                };
                let candidate = output_level + 1;
                if candidate > levels[net_id.index()] {
                    levels[net_id.index()] = candidate;
                    changed = true;
                }
            }
        }
    }

    levels
}

#[cfg(test)]
mod tests {
    use super::annotate_net_criticality;
    use crate::ir::{Cell, Design, Endpoint, Net, Port};

    #[test]
    fn annotates_longer_path_as_more_critical() {
        let mut design = Design {
            ports: vec![Port::input("in"), Port::output("out")],
            cells: vec![
                Cell::lut("u0", "LUT4")
                    .with_input("A", "in_net")
                    .with_output("O", "mid0"),
                Cell::lut("u1", "LUT4")
                    .with_input("A", "mid0")
                    .with_output("O", "mid1"),
                Cell::lut("u2", "LUT4")
                    .with_input("A", "in_net")
                    .with_output("O", "fast"),
            ],
            nets: vec![
                Net::new("in_net")
                    .with_driver(Endpoint::port("in", "IN"))
                    .with_sink(Endpoint::cell("u0", "A"))
                    .with_sink(Endpoint::cell("u2", "A")),
                Net::new("mid0")
                    .with_driver(Endpoint::cell("u0", "O"))
                    .with_sink(Endpoint::cell("u1", "A")),
                Net::new("mid1")
                    .with_driver(Endpoint::cell("u1", "O"))
                    .with_sink(Endpoint::port("out", "OUT")),
                Net::new("fast")
                    .with_driver(Endpoint::cell("u2", "O"))
                    .with_sink(Endpoint::port("out", "OUT")),
            ],
            ..Design::default()
        };

        annotate_net_criticality(&mut design);

        let mid0 = design
            .nets
            .iter()
            .find(|net| net.name == "mid0")
            .map(|net| net.criticality)
            .unwrap_or(0.0);
        let fast = design
            .nets
            .iter()
            .find(|net| net.name == "fast")
            .map(|net| net.criticality)
            .unwrap_or(0.0);

        assert!(mid0 > fast);
    }
}
