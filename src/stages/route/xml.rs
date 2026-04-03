use anyhow::{Context, Result};
use roxmltree::Document;
use std::{fs, path::Path};

use super::RoutedNetPip;

pub fn load_route_pips(path: &Path) -> Result<Vec<RoutedNetPip>> {
    let xml = fs::read_to_string(path)
        .with_context(|| format!("failed to read routed design {}", path.display()))?;
    load_route_pips_xml(&xml)
        .with_context(|| format!("failed to parse routed design {}", path.display()))
}

pub fn load_route_pips_xml(xml: &str) -> Result<Vec<RoutedNetPip>> {
    let doc = Document::parse(xml).context("failed to parse routed xml")?;
    let root = doc.root_element();
    let mut pips = Vec::new();

    for net in root
        .descendants()
        .filter(|node| node.has_tag_name("net") && node.attribute("name").is_some())
    {
        let net_name = net.attribute("name").unwrap_or_default().to_string();
        for pip in net.children().filter(|node| node.has_tag_name("pip")) {
            let Some((x, y)) = pip
                .attribute("position")
                .and_then(|value| value.split_once(','))
                .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)))
            else {
                continue;
            };
            pips.push(RoutedNetPip {
                net_name: net_name.clone(),
                x,
                y,
                from_net: pip.attribute("from").unwrap_or_default().to_string(),
                to_net: pip.attribute("to").unwrap_or_default().to_string(),
            });
        }
    }

    Ok(pips)
}
