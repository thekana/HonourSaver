use crate::{
    error::{Error, Result},
    lspk::Lspk,
};
use rc_zip_sync::{rc_zip::parse::EntryKind, ReadZip};
use std::{fs::File, path::Path};

impl Lspk {
    /// Inspects a zipfile for a file with the .pak extension and reads it.
    ///
    /// # Errors
    ///
    /// Returns an error if the zipfile cannot be read, no `.pak` file is found, or the LSPK file is
    /// invalid.
    pub fn from_zip(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = File::open(&path)
            .map_err(|e| Error::read_io(path.as_ref().display(), e))?
            .read_zip()?
            .entries()
            .find_map(|e| {
                e.sanitized_name()
                    .filter(|_| matches!(e.kind(), EntryKind::File))
                    .filter(|path| {
                        Path::new(path)
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("pak"))
                    })
                    .map(|_| e.bytes())
            })
            .ok_or(Error::MissingLspkFileInZip)?
            .map_err(|e| Error::UnableToReadZipFile(e.into()))?;

        Self::from_bytes(&bytes)
    }
}
