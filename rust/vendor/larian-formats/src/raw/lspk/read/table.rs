use crate::{
    common::pathbuf_from_bytes,
    error::{Error, Result},
    lspk::UnpackedFile,
    raw::{
        TABLE_ENTRY_PATH_LENGTH,
        lspk::{EntryDetails, EntryDetailsVersion},
    },
};
use std::{
    borrow::Cow,
    io::{Read, Seek},
    path::Path,
    sync::LazyLock,
};

// Source: https://github.com/LaughingLeader/BG3ModManager/blob/fc3535868935c16b7260be0d5749c0d84b2a9b1a/src/GUI/Resources/IgnoredMods.json#L21
static IGNORED_PATH: LazyLock<&Path> = LazyLock::new(|| Path::new("Game/GUI/Assets"));

// Source: https://github.com/LaughingLeader/BG3ModManager/blob/fc3535868935c16b7260be0d5749c0d84b2a9b1a/src/GUI/Resources/IgnoredMods.json#L23-L185
pub(crate) const BUILTIN_MOD_FOLDERS: &[&str] = &[
    "Gustav",
    "GustavDev",
    "GustavX",
    "Shared",
    "SharedDev",
    "Engine",
    "Game",
    "DiceSet_01",
    "DiceSet_02",
    "DiceSet_03",
    "DiceSet_04",
    "DiceSet_05",
    "DiceSet_06",
    "DiceSet_07",
    "Honour",
    "HonourX",
    "ModBrowser",
    "MainUI",
    "CrossplayUI",
    "PhotoMode",
];

#[derive(Debug)]
pub(super) struct Table {
    version: EntryDetailsVersion,
    num_entries: usize,
    bytes: Vec<u8>,
}

impl Table {
    pub(super) fn read_from(input: &mut impl Read, version: EntryDetailsVersion) -> Result<Self> {
        let num_entries =
            usize_from_u32!(read_num!(u32, input, ".pak number of files contained files")?);
        let num_table_bytes =
            usize_from_u32!(read_num!(u32, input, ".pak number of bytes in table")?);

        let mut compressed_table = vec![0_u8; num_table_bytes];
        input
            .read_exact(&mut compressed_table)
            .map_err(|e| Error::read_io(".pak compressed table", e))?;

        let bytes = lz4_flex::block::decompress(
            &compressed_table,
            num_entries * version.decompressed_table_entry_length(),
        )
        .map_err(Error::TableEntryDecompressionFailed)?;

        Ok(Self {
            version,
            num_entries,
            bytes,
        })
    }

    pub(super) fn read_entries<'a>(
        &'a self,
        mut input: impl Read + Seek + 'a,
    ) -> impl Iterator<Item = Result<UnpackedFile>> + 'a {
        self.entries().map(move |entry| {
            let entry = entry?;
            extract_entry(&mut input, entry)
        })
    }

    pub(super) fn paths(&self) -> impl Iterator<Item = Cow<'_, [u8]>> {
        (0..self.num_entries).map(|i| {
            let entry_offset = i * self.version.decompressed_table_entry_length();
            self.path_at(entry_offset)
        })
    }

    #[cfg(feature = "raw")]
    pub(super) fn read_entry_with_path(
        &self,
        relative_path: impl AsRef<Path>,
        input: &mut (impl Read + Seek),
    ) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self
            .entries()
            .find(|details| {
                details.as_ref().map_or(true, |EntryDetails { path, .. }| {
                    relative_path.as_ref() == Path::new(String::from_utf8_lossy(path).as_ref())
                })
            })
            .transpose()?
        else {
            return Ok(None);
        };

        extract_entry(input, entry).map(|UnpackedFile { contents, .. }| Some(contents))
    }

    #[cfg(feature = "raw")]
    pub(super) fn meta_lsx_entries(&self) -> impl Iterator<Item = Result<EntryDetails<'_>>> {
        (0..self.num_entries).filter_map(|i| {
            let entry_offset = i * self.version.decompressed_table_entry_length();
            let path = self.path_at(entry_offset);

            is_meta_lsx_path(&path).then(|| self.entry_details(entry_offset, path))
        })
    }

    fn entries(&self) -> impl Iterator<Item = Result<EntryDetails<'_>>> {
        (0..self.num_entries).map(|i| {
            let entry_offset = i * self.version.decompressed_table_entry_length();
            let path = self.path_at(entry_offset);

            self.entry_details(entry_offset, path)
        })
    }

    fn path_at(&self, entry_offset: usize) -> Cow<'_, [u8]> {
        let end_of_path = self
            .bytes
            .iter()
            .skip(entry_offset)
            .take(TABLE_ENTRY_PATH_LENGTH)
            .enumerate()
            .find_map(|(i, byte)| (*byte == 0).then_some(i))
            .unwrap_or(TABLE_ENTRY_PATH_LENGTH);

        Cow::Borrowed(&self.bytes[entry_offset..entry_offset + end_of_path])
    }

    fn entry_details<'a>(
        &'a self,
        entry_offset: usize,
        path: Cow<'a, [u8]>,
    ) -> Result<EntryDetails<'a>> {
        let entry = self
            .version
            .read_entry_details(&self.bytes, entry_offset, path)?;

        Ok(entry)
    }

    // Reimplemented from parts of https://github.com/LaughingLeader/BG3ModManager/blob/fc3535868935c16b7260be0d5749c0d84b2a9b1a/src/Core/Util/DivinityModDataLoader.cs#L466
    pub(crate) fn is_override(&self) -> bool {
        let mut found_meta = false;
        let mut found_builtin = false;
        let mut found_non_ignored_builtin = false;

        for path in self.paths() {
            if is_meta_lsx_path(path.as_ref()) {
                found_meta = true;
            } else if is_builtin_path(path.as_ref()) {
                found_builtin = true;
                if !is_ignored_path(pathbuf_from_bytes(path.as_ref())) {
                    found_non_ignored_builtin = true;
                }
            } else if is_non_builtin_path(path.as_ref()) {
                return false;
            }
        }

        found_non_ignored_builtin || (!found_meta && found_builtin)
    }
}

fn extract_entry(input: &mut (impl Read + Seek), entry: EntryDetails<'_>) -> Result<UnpackedFile> {
    let compression_type = entry.compression_type;

    entry
        .decompress_file(input, compression_type)
        .map(Into::into)
}

fn is_meta_lsx_path(path: &[u8]) -> bool {
    path.starts_with(b"Mods/") &&
        path.ends_with(b"/meta.lsx") &&
        !path["Mods/".len()..path.len() - "/meta.lsx".len()].contains(&b'/')
}

fn is_builtin_path(path: &[u8]) -> bool {
    mod_name_component(path)
        .is_some_and(|folder| BUILTIN_MOD_FOLDERS.iter().any(|f| folder == f.as_bytes()))
}

fn is_non_builtin_path(path: &[u8]) -> bool {
    mod_name_component(path)
        .is_some_and(|folder| !BUILTIN_MOD_FOLDERS.iter().any(|f| folder == f.as_bytes()))
}

fn mod_name_component(path: &[u8]) -> Option<&[u8]> {
    let (root, rest) = split_package_path_component(path)?;
    if root != b"Mods" && root != b"Public" {
        return None;
    }

    let (folder, remaining) = split_package_path_component(rest)?;

    (!remaining.is_empty()).then_some(folder)
}

fn is_ignored_path(path: impl AsRef<Path>) -> bool {
    let mut iter = path.as_ref().iter();

    loop {
        if iter.as_path().starts_with(*IGNORED_PATH) {
            return true;
        }

        if iter.next().is_none() {
            return false;
        }
    }
}

fn split_package_path_component(path: &[u8]) -> Option<(&[u8], &[u8])> {
    path.iter()
        .position(|byte| *byte == b'/')
        .map(|slash_position| (&path[..slash_position], &path[slash_position + 1..]))
}
