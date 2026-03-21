use anyhow::{Result, anyhow, bail};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use super::constants::{
    ARCH_FILE, CIL_FILE, DC_CELL_FILE, DELAY_FILE, PACK_CELL_FILE, PACK_CONFIG_FILE,
    PACK_DCP_LIB_FILE, STA_LIB_FILE,
};

#[derive(Debug, Clone)]
pub struct ResourceBundle {
    pub root: PathBuf,
    pub dc_cell: PathBuf,
    pub pack_cell: PathBuf,
    pub pack_dcp_lib: PathBuf,
    pub pack_config: PathBuf,
    pub sta_lib: PathBuf,
    pub arch: PathBuf,
    pub delay: PathBuf,
    pub cil: PathBuf,
}

impl ResourceBundle {
    pub fn from_root(root: &Path) -> Result<Self> {
        let root = root.to_path_buf();
        let bundle = Self {
            dc_cell: root.join(DC_CELL_FILE),
            pack_cell: root.join(PACK_CELL_FILE),
            pack_dcp_lib: root.join(PACK_DCP_LIB_FILE),
            pack_config: root.join(PACK_CONFIG_FILE),
            sta_lib: root.join(STA_LIB_FILE),
            arch: root.join(ARCH_FILE),
            delay: root.join(DELAY_FILE),
            cil: root.join(CIL_FILE),
            root,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn discover_from(start: &Path) -> Result<Self> {
        for candidate in candidate_roots(start) {
            if let Ok(bundle) = Self::from_root(&candidate) {
                return Ok(bundle);
            }
        }
        Err(anyhow!(
            "unable to discover an FDE hardware resource bundle; pass --resource-root explicitly"
        ))
    }

    fn validate(&self) -> Result<()> {
        for path in [
            &self.dc_cell,
            &self.pack_cell,
            &self.pack_dcp_lib,
            &self.pack_config,
            &self.sta_lib,
            &self.arch,
            &self.delay,
            &self.cil,
        ] {
            if !path.is_file() {
                bail!("missing resource file {}", path.display());
            }
        }
        Ok(())
    }
}

fn candidate_roots(start: &Path) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for base in [start.to_path_buf(), manifest_dir.clone()] {
        insert_bundle_layouts(&mut roots, &base);
        if let Some(parent) = base.parent() {
            insert_bundle_layouts(&mut roots, parent);
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        insert_bundle_layouts(&mut roots, &path);
                    }
                }
            }
        }
    }
    roots.into_iter().collect()
}

fn insert_bundle_layouts(roots: &mut BTreeSet<PathBuf>, base: &Path) {
    roots.insert(base.join("resource/fde/hw_lib"));
    roots.insert(base.join("src-tauri/resource/fde/hw_lib"));
}
