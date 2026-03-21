use anyhow::{Context, Result, anyhow};
use std::{env, path::PathBuf};

use crate::resource::ResourceBundle;

use super::options::{ImplementationOptions, ResolvedResources};

pub(crate) fn resolve_resources(options: &ImplementationOptions) -> Result<ResolvedResources> {
    let cwd = env::current_dir().context("failed to read current directory")?;
    let bundle = if options.resource_root.is_some()
        || options.arch.is_none()
        || options.dc_cell.is_none()
        || options.pack_cell.is_none()
        || options.pack_lib.is_none()
        || options.pack_config.is_none()
        || options.delay.is_none()
        || options.sta_lib.is_none()
        || options.cil.is_none()
    {
        Some(match options.resource_root.as_deref() {
            Some(root) => ResourceBundle::from_root(root)?,
            None => ResourceBundle::discover_from(&cwd)?,
        })
    } else {
        None
    };

    let resolve_optional =
        |explicit: &Option<PathBuf>, getter: fn(&ResourceBundle) -> &PathBuf| -> Option<PathBuf> {
            explicit
                .clone()
                .or_else(|| bundle.as_ref().map(|bundle| getter(bundle).clone()))
        };

    let arch = options
        .arch
        .clone()
        .or_else(|| bundle.as_ref().map(|bundle| bundle.arch.clone()))
        .ok_or_else(|| anyhow!("an architecture XML is required for implementation"))?;

    Ok(ResolvedResources {
        dc_cell: resolve_optional(&options.dc_cell, |bundle| &bundle.dc_cell),
        pack_cell: resolve_optional(&options.pack_cell, |bundle| &bundle.pack_cell),
        pack_lib: resolve_optional(&options.pack_lib, |bundle| &bundle.pack_dcp_lib),
        pack_config: resolve_optional(&options.pack_config, |bundle| &bundle.pack_config),
        arch,
        delay: resolve_optional(&options.delay, |bundle| &bundle.delay),
        sta_lib: resolve_optional(&options.sta_lib, |bundle| &bundle.sta_lib),
        cil: resolve_optional(&options.cil, |bundle| &bundle.cil),
    })
}
