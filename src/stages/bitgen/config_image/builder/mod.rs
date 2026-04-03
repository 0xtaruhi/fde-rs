mod image;
mod route;
mod site;
mod target;

use anyhow::Result;

use self::{image::ConfigImageBuilder, route::encode_route_pip, site::encode_programmed_site};
use crate::{bitgen::ProgrammingImage, cil::Cil, resource::Arch};

pub(crate) fn encode_config_image(
    programming: &ProgrammingImage,
    cil: &Cil,
    arch: Option<&Arch>,
) -> Result<super::ConfigImage> {
    let mut image = ConfigImageBuilder::new();

    for site in &programming.sites {
        encode_programmed_site(&mut image, site, cil, arch);
    }

    image.extend_notes(programming.notes.iter().cloned());

    for pip in &programming.routes {
        encode_route_pip(&mut image, pip, cil, arch);
    }

    Ok(image.finish())
}
