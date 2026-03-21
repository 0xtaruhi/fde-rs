use super::{BitgenOptions, run};
use crate::ir::{BitstreamImage, Cluster, Design, Net, RouteSegment};
use anyhow::Result;

fn routed_design() -> Design {
    Design {
        name: "bitgen-mini".to_string(),
        stage: "timed".to_string(),
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(1, 1),
        ],
        nets: vec![
            Net::new("mid")
                .with_route_segment(RouteSegment::new((0, 1), (1, 1)))
                .with_route_segment(RouteSegment::new((1, 1), (2, 1))),
        ],
        ..Design::default()
    }
}

fn assert_image(image: &BitstreamImage) {
    assert!(image.bytes.starts_with(b"FDEBIT24"));
    assert_eq!(image.design_name, "bitgen-mini");
    assert_eq!(image.sha256.len(), 64);
    assert!(image.sidecar_text.contains("mode=deterministic-payload"));
    assert!(image.sidecar_text.contains("CLUSTER clb0"));
    assert!(image.sidecar_text.contains("NET mid len=2"));
}

#[test]
fn falls_back_to_deterministic_payload_without_resources() -> Result<()> {
    let result = run(routed_design(), &BitgenOptions::default())?;
    assert_image(&result.value);
    assert!(
        result
            .report
            .messages
            .iter()
            .any(|message| message.contains("deterministic bitstream payload"))
    );
    Ok(())
}
