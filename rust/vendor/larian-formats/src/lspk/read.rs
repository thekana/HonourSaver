//! Logic for parsing LSPK files.

#[cfg(feature = "zip")]
pub mod zip;

use super::Lspk;
use crate::{
    bg3::pak::Info,
    error::{Error, Result},
};
use std::io::Cursor;

impl Lspk {
    /// Parses the `meta.lsx` file from the LSPK file.
    ///
    /// # Errors
    ///
    /// Returns an error if the `meta.lsx` file is not found at the expected subpath or cannot be
    /// parsed.
    pub fn deserialize_meta_lsx(&self) -> Result<Info> {
        let meta_lsx = self
            .files
            .iter()
            .find(|d| {
                let mut path_components = d.path.components();

                // Check if there are exactly three components, starting with "Mods" and ending with
                // "meta.lsx".
                path_components
                    .next()
                    .map(|c| c.as_os_str().as_encoded_bytes()) ==
                    Some(b"Mods") &&
                    path_components.next().is_some() &&
                    path_components
                        .next()
                        .map(|c| c.as_os_str().as_encoded_bytes()) ==
                        Some(b"meta.lsx") &&
                    path_components.next().is_none()
            })
            .ok_or(Error::MissingMetadataFile)?;

        Info::parse(Cursor::new(&meta_lsx.contents))
    }
}
