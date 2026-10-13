//! Logic for writing LSPK files.

use super::UnpackedFile;
use crate::{
    error::{Error, Result},
    raw::write_packed_bytes,
};
use std::{collections::VecDeque, ffi::OsString, fs::OpenOptions, path::PathBuf};

/// Packs loose mod files into an LSPK file.
///
/// # Arguments
/// * `mod_root_path` - The path to the directory containing the loose mod files.
/// * `destination` - The path to the output LSPK file. If `None`, the output file will be created
///   next to `mod_root_path` with the file prefix of `mod_root_path` and the extension `.pak`.
///
/// # Errors
///
/// Returns an error if packing and writing the LSPK file fails.
pub fn write(mod_root_path: impl Into<PathBuf>, destination: Option<PathBuf>) -> Result<()> {
    let writer = Writer::with_mod_root_path(mod_root_path, destination)?;
    writer.write()
}

#[derive(Debug)]
pub(crate) struct Writer {
    mod_root_path: PathBuf,
    files: Vec<UnpackedFile>,
    destination: Option<PathBuf>,
}

impl Writer {
    fn write(mut self) -> Result<()> {
        let destination_path = self.destination_path()?;

        let output_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&destination_path)
            .map_err(|e| Error::read_io(destination_path.display(), e))?;

        write_packed_bytes(self.files.iter().map(UnpackedFile::as_mod_file), output_file)?;
        Ok(())
    }

    fn destination_path(&mut self) -> Result<PathBuf> {
        if let Some(destination) = self.destination.take() {
            return Ok(destination);
        }

        let Some(mod_output_file_path) = self.mod_root_path.file_name() else {
            return Err(Error::UnsupportedModPath(std::mem::take(&mut self.mod_root_path)));
        };

        let mut output_path = OsString::from(mod_output_file_path);
        output_path.push(".pak");

        Ok(output_path.into())
    }

    fn with_mod_root_path(
        mod_root_path: impl Into<PathBuf>,
        destination: Option<PathBuf>,
    ) -> Result<Self> {
        let mod_root_path = mod_root_path.into();
        let mut files = Vec::new();

        let mut dirs_to_check = VecDeque::new();
        dirs_to_check.push_back(PathBuf::from(""));

        while let Some(dir) = dirs_to_check.pop_front() {
            let full_path = mod_root_path.join(&dir);

            for entry in
                std::fs::read_dir(&full_path).map_err(|e| Error::read_io(full_path.display(), e))?
            {
                let entry = entry.map_err(|e| {
                    Error::write_io(format!("iterating {}", full_path.display()), e)
                })?;

                let file_type = entry.file_type().map_err(|e| {
                    Error::write_io(
                        format!("getting file type of {}", entry.file_name().display()),
                        e,
                    )
                })?;

                if file_type.is_symlink() {
                    continue;
                }

                let relative_path = dir.join(entry.file_name());

                if file_type.is_dir() {
                    dirs_to_check.push_back(relative_path);
                    continue;
                }

                let file_path = mod_root_path.join(&relative_path);

                let decompressed_bytes = std::fs::read(&file_path)
                    .map_err(|e| Error::read_io(file_path.display(), e))?;

                files.push(UnpackedFile {
                    path: relative_path,
                    contents: decompressed_bytes,
                });
            }
        }

        Ok(Self {
            mod_root_path,
            files,
            destination,
        })
    }
}
