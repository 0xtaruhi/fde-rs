use std::sync::Arc;

mod arch;
mod bundle;
mod constants;
mod delay;

pub use arch::{Arch, Pad, PadSiteKind, TileInstance, TileKind, TileSideCapacity, load_arch};
pub use bundle::ResourceBundle;
pub use constants::{
    ARCH_FILE, CIL_FILE, DC_CELL_FILE, DELAY_FILE, PACK_CELL_FILE, PACK_CONFIG_FILE,
    PACK_DCP_LIB_FILE, STA_LIB_FILE,
};
pub use delay::{DelayModel, load_delay_model};

pub type SharedArch = Arc<Arch>;
pub type SharedDelayModel = Arc<DelayModel>;
