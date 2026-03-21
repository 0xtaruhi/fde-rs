use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BitstreamImage {
    pub design_name: String,
    #[serde(default)]
    pub bytes: Vec<u8>,
    pub sidecar_text: String,
    pub sha256: String,
}
