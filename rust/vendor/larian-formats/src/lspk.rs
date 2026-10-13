//! Logic for reading and writing BG3 .pak files.
//!
//! Because `.pak` is a relatively common extension used by multiple games for incompatible formats,
//! `larian-formats` uses the metadata format's "LSPK" marker as a shorthand for referring to BG3
//! .pak files.

pub use crate::{
    bg3::pak::{ModuleInfo, ModuleShortDesc},
    raw::list_mod_pack_files,
};
pub use write::write;

use crate::{
    common::pathbuf_from_bytes,
    error::{Error, Result},
    raw::{ModFile, unpack_files},
};
use std::{
    borrow::Cow,
    io::Cursor,
    path::{Path, PathBuf},
};

mod read;
mod write;
/// Represents a single unpacked file from an LSPK file.
#[derive(Debug)]
pub struct UnpackedFile {
    /// The relative path of the loose mod file.
    pub path: PathBuf,

    /// The decompressed contents of the loose mod file.
    pub contents: Vec<u8>,
}

impl<'a> From<ModFile<'a>> for UnpackedFile {
    fn from(mod_file: ModFile<'a>) -> Self {
        Self {
            path: pathbuf_from_bytes(&mod_file.path),
            contents: mod_file.contents.into_owned(),
        }
    }
}

impl UnpackedFile {
    /// Borrows the data in the unpacked file.
    pub fn as_mod_file(&self) -> ModFile<'_> {
        ModFile {
            path: Cow::Borrowed(self.path.as_os_str().as_encoded_bytes()),
            contents: Cow::Borrowed(&self.contents),
        }
    }
}

/// A parsed LSPK file.
#[derive(Debug)]
pub struct Lspk {
    /// The raw bytes of the LSPK file.
    pub packed_bytes: Vec<u8>,

    /// The unpacked files contained in the LSPK file.
    pub files: Vec<UnpackedFile>,
}

/// Determines whether the LSPK package at the given path should be treated as an override mod.
///
/// Packages with missing or invalid metadata are treated as overrides.
pub fn is_override(path: impl AsRef<Path>) -> bool {
    crate::raw::is_override(path).unwrap_or(true)
}

impl Lspk {
    /// Parses an LSPK file from the given bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes do not contain a valid LSPK file.
    pub fn from_bytes<'a>(bytes: impl Into<Cow<'a, [u8]>>) -> Result<Self> {
        let bytes = bytes.into();

        unpack_files(&mut Cursor::new(bytes.as_ref())).map(|files| Self {
            packed_bytes: bytes.into_owned(),
            files,
        })
    }

    fn from_pak(path: impl AsRef<Path>) -> Result<Self> {
        let packed_bytes =
            std::fs::read(&path).map_err(|e| Error::read_io(path.as_ref().display(), e))?;

        unpack_files(&mut Cursor::new(&packed_bytes)).map(|files| Self {
            packed_bytes,
            files,
        })
    }

    /// Parses an LSPK file.
    ///
    /// # Errors
    ///
    /// Returns an error if the .pak file is invalid.
    #[cfg(not(feature = "zip"))]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_pak(path)
    }

    /// Parses an LSPK file from a path to either a .pak or .zip file.
    ///
    /// If the `zip` feature is disabled, this method will still be defined but will return an
    /// error when passed the path to .zip file. Passing the path directly to an LSPK file will
    /// still work.
    ///
    /// # Errors
    ///
    /// Returns an error if no .pak is found in the .zip or if the LSPK file is invalid.
    #[cfg(feature = "zip")]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let is_zipfile = path
            .as_ref()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"));

        let reader = if is_zipfile {
            Self::from_zip
        } else {
            Self::from_pak
        };

        reader(path)
    }
}

#[cfg(test)]
mod tests {
    use super::is_override;
    use crate::raw::{ModFile, write_packed_bytes};
    use std::{fs, io::Cursor, path::PathBuf};

    const FOLDER: &str = "TestMod";
    const META_PATH: &[u8] = b"Mods/TestMod/meta.lsx";

    fn valid_meta_lsx() -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<save>
    <region id="Config">
        <node id="root">
            <children>
                <node id="ModuleInfo">
                    <attribute id="Folder" type="LSString" value="{FOLDER}" />
                </node>
            </children>
        </node>
    </region>
</save>
"#
        )
        .into_bytes()
    }

    fn package_path(test_name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("xiba-lspk-is-override-{}-{test_name}.pak", std::process::id()))
    }

    fn write_package(test_name: &str, files: Vec<ModFile<'static>>) -> PathBuf {
        let mut bytes = Cursor::new(Vec::new());
        write_packed_bytes(files, &mut bytes).unwrap();

        let path = package_path(test_name);
        fs::write(&path, bytes.into_inner()).unwrap();
        path
    }

    fn assert_is_override(test_name: &str, files: Vec<ModFile<'static>>, expected: bool) {
        let path = write_package(test_name, files);

        assert_eq!(is_override(&path), expected);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn package_without_meta_lsx_and_without_builtin_files_is_not_override() {
        assert_is_override(
            "no-meta-lsx-no-builtin",
            vec![ModFile::new(
                b"Public/TestMod/file.txt".as_slice(),
                b"contents".as_slice(),
            )],
            false,
        );
    }

    #[test]
    fn package_without_meta_lsx_with_builtin_files_is_override() {
        assert_is_override(
            "no-meta-lsx-builtin",
            vec![ModFile::new(
                b"Public/Gustav/file.txt".as_slice(),
                b"contents".as_slice(),
            )],
            true,
        );
    }

    #[test]
    fn package_with_invalid_meta_lsx_is_not_override() {
        assert_is_override(
            "invalid-meta-lsx",
            vec![ModFile::new(META_PATH, b"<save".as_slice())],
            false,
        );
    }

    #[test]
    fn package_with_only_meta_lsx_is_not_override() {
        assert_is_override("only-meta-lsx", vec![ModFile::new(META_PATH, valid_meta_lsx())], false);
    }

    #[test]
    fn package_with_meta_lsx_and_another_mods_folder_file_is_not_override() {
        assert_is_override(
            "mods-folder-file",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(
                    b"Mods/TestMod/ScriptExtender/Lua/main.lua".as_slice(),
                    b"contents".as_slice(),
                ),
            ],
            false,
        );
    }

    #[test]
    fn package_with_public_module_folder_file_is_not_override() {
        assert_is_override(
            "public-module-folder-file",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(
                    b"Public/TestMod/RootTemplates/file.lsx".as_slice(),
                    b"contents".as_slice(),
                ),
            ],
            false,
        );
    }

    #[test]
    fn package_with_public_engine_file_is_override() {
        assert_is_override(
            "public-engine-file",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(
                    b"Public/Engine/Timeline/MaterialGroups/file.lsx".as_slice(),
                    b"contents".as_slice(),
                ),
            ],
            true,
        );
    }

    #[test]
    fn package_with_unrelated_files_and_valid_metadata_is_not_override() {
        assert_is_override(
            "unrelated-files-valid-metadata",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(b"Public/OtherMod/file.lsx".as_slice(), b"contents".as_slice()),
                ModFile::new(
                    b"Generated/Public/TestMod/file.lsx".as_slice(),
                    b"contents".as_slice(),
                ),
            ],
            false,
        );
    }

    #[test]
    fn package_with_builtin_files_and_metadata_is_override() {
        assert_is_override(
            "builtin-files-with-metadata",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(
                    b"Public/Gustav/RootTemplates/file.lsx".as_slice(),
                    b"contents".as_slice(),
                ),
            ],
            true,
        );
    }

    #[test]
    fn package_with_builtin_mods_folder_file_and_metadata_is_override() {
        assert_is_override(
            "builtin-mods-folder-file",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(b"Mods/Gustav/file.lsx".as_slice(), b"contents".as_slice()),
            ],
            true,
        );
    }

    #[test]
    fn package_with_ignored_builtin_path_and_metadata_is_not_override() {
        assert_is_override(
            "ignored-builtin-path-with-metadata",
            vec![
                ModFile::new(META_PATH, valid_meta_lsx()),
                ModFile::new(
                    b"Public/Game/GUI/Assets/file.txt".as_slice(),
                    b"contents".as_slice(),
                ),
            ],
            false,
        );
    }

    #[test]
    fn package_with_ignored_builtin_path_without_metadata_is_override() {
        assert_is_override(
            "ignored-builtin-path-no-metadata",
            vec![ModFile::new(
                b"Public/Game/GUI/Assets/file.txt".as_slice(),
                b"contents".as_slice(),
            )],
            true,
        );
    }

    #[test]
    fn package_with_only_builtin_meta_lsx_is_not_override() {
        assert_is_override(
            "builtin-meta-lsx",
            vec![ModFile::new(
                b"Mods/Gustav/meta.lsx".as_slice(),
                valid_meta_lsx(),
            )],
            false,
        );
    }
}
