mod entry;
mod iter;
mod table;

use self::table::Table;
use super::LSPK_ID_BYTES as ID_BYTES;
#[cfg(feature = "raw")]
pub use crate::bg3::pak::Info;
use crate::raw::lspk::read::iter::ModFilePaths;
use crate::{
    error::{Error, Result},
    lspk::UnpackedFile,
    raw::{lspk::EntryDetailsVersion, read_bytes},
};
#[cfg(feature = "raw")]
use std::io::Cursor;
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Debug)]
struct Header {
    entry_details_version: EntryDetailsVersion,
    footer_offset: u64,
}

pub(crate) fn is_override(path: impl AsRef<Path>) -> Result<bool> {
    let file = File::open(&path).map_err(|e| Error::read_io(path.as_ref().display(), e))?;
    let mut reader = BufReader::new(file);

    let header = read_header(&mut reader)?;
    reader
        .seek(SeekFrom::Start(header.footer_offset))
        .map_err(|e| Error::custom_io("seeking to .pak footer offset", e))?;

    let table = Table::read_from(&mut reader, header.entry_details_version)?;

    Ok(table.is_override())
}

/// Lists the file paths contained in the mod file.
///
/// # Errors
///
/// Returns an error if the mod file header can't be read.
pub fn list_mod_pack_files(path: &Path) -> Result<ModFilePaths> {
    let file =
        File::open(path).map_err(|e| Error::read_io(format!("opening {}", path.display()), e))?;

    let mut reader = BufReader::new(file);
    let version = EntryDetailsVersion::read_from(&mut reader)?;

    let footer_offset =
        read_num!(u64, &mut reader, format!("footer offset of {}", path.display()))?;

    reader.seek(SeekFrom::Start(footer_offset)).map_err(|e| {
        Error::custom_io(format!("seeking footer metadata of {}", path.display()), e)
    })?;

    let table = Table::read_from(&mut reader, version)?;

    Ok(ModFilePaths { table })
}

/// Contains parsed info from `meta.lsx` files within a .pak mod.
#[cfg(feature = "raw")]
#[derive(Debug)]
pub struct Metadata {
    metadata: Vec<Info>,
    is_override: bool,
}

#[cfg(feature = "raw")]
impl Metadata {
    /// Whether the mod is an override.
    pub const fn is_override(&self) -> bool {
        self.is_override
    }

    /// Gets references to the  parsed info from each `meta.lsx` file in the mod.
    pub fn metadata(&self) -> &[crate::bg3::pak::Info] {
        &self.metadata
    }

    /// Gets the parsed info from each `meta.lsx` file in the mod, consuming the container.
    pub fn into_metadata(self) -> Vec<crate::bg3::pak::Info> {
        self.metadata
    }
}

/// Gets the metadata from each `meta.lsx` entry in the PAK file.
///
/// # Errors
///
/// Returns an error if the PAK file cannot be read or if the `meta.lsx` entries cannot be parsed.
#[cfg(feature = "raw")]
pub fn get_mod_info_from_pak(path: impl AsRef<Path>) -> Result<Metadata> {
    use std::{fs::File, io::BufReader};

    let file =
        File::open(&path).map_err(|e| Error::read_io(format!("{}", path.as_ref().display()), e))?;

    get_mod_info_from_reader(path.as_ref(), BufReader::new(file))
}

/// Gets the metadata from each `meta.lsx` entry in the bytes of a PAK file.
///
/// # Errors
///
/// Returns an error if the bytes cannot be read or if the `meta.lsx` entries cannot be parsed.
#[cfg(feature = "raw")]
pub fn get_mod_info_from_reader<T: Read + Seek>(
    path: &Path,
    mut reader: BufReader<T>,
) -> Result<Metadata> {
    let version = EntryDetailsVersion::read_from(&mut reader)?;

    let footer_offset =
        read_num!(u64, &mut reader, format!("footer offset of {}", path.display()))?;

    reader.seek(SeekFrom::Start(footer_offset)).map_err(|e| {
        Error::custom_io(format!("seeking footer metadata of {}", path.display()), e)
    })?;

    let table = Table::read_from(&mut reader, version)?;

    let metadata = table
        .meta_lsx_entries()
        .map(|entry| {
            let entry = entry?;
            let compression_type = entry.compression_type;
            let meta_lsx = entry.decompress_file(&mut reader, compression_type)?;
            crate::bg3::pak::Info::parse(Cursor::new(&meta_lsx.contents))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Metadata {
        is_override: table.is_override(),
        metadata,
    })
}

#[cfg(feature = "raw")]
/// Extracts a single file from a mod based on its relative path.
///
/// # Errors
///
/// Returns an error if the mod file does not exist or cannot be read, or if parsing and
/// decompressing the file from the mod fails.
pub fn extract_file_from_pak(
    internal_file: impl AsRef<Path>,
    mod_file: impl AsRef<Path>,
) -> Result<Option<Vec<u8>>> {
    use std::{fs::File, io::BufReader};

    let mod_file = mod_file.as_ref();

    let file =
        File::open(mod_file).map_err(|e| Error::read_io(format!("{}", mod_file.display()), e))?;

    extract_file_from_reader(internal_file.as_ref(), BufReader::new(file))
}

#[cfg(feature = "raw")]
/// Extracts a single file from a mod based on its relative path.
///
/// # Errors
///
/// Returns an error if the mod file does not exist or cannot be read, or if parsing and
/// decompressing the file from the mod fails.
pub fn extract_file_from_reader<T: Read + Seek>(
    internal_file: &Path,
    mut reader: BufReader<T>,
) -> Result<Option<Vec<u8>>> {
    let version = EntryDetailsVersion::read_from(&mut reader)?;

    let footer_offset =
        read_num!(u64, &mut reader, format!("footer offset of {}", internal_file.display()))?;

    reader.seek(SeekFrom::Start(footer_offset)).map_err(|e| {
        Error::custom_io(format!("seeking footer metadata of {}", internal_file.display()), e)
    })?;

    Table::read_from(&mut reader, version)?.read_entry_with_path(internal_file, &mut reader)
}

fn read_header(input: &mut (impl Seek + Read)) -> Result<Header> {
    let id_bytes = read_bytes(input).map_err(|e| Error::read_io(".pak header", e))?;

    if id_bytes != *ID_BYTES {
        return Err(Error::InvalidIdentifier {
            expected_bytes: *ID_BYTES,
            bytes_found: id_bytes,
        });
    }

    let entry_details_version = EntryDetailsVersion::read_from(input)?;

    let footer_offset = read_num!(u64, input, ".pak footer offset")?;

    Ok(Header {
        entry_details_version,
        footer_offset,
    })
}

/// Unpacks each compressed file entry from the PAK file.
///
/// # Errors
///
/// Returns an error if the PAK file cannot be read or if the files cannot be unpacked.
pub fn unpack_files<T>(input: &mut (impl Seek + Read)) -> Result<Vec<T>>
where
    T: From<UnpackedFile>,
{
    let header = read_header(input)?;
    input
        .seek(SeekFrom::Start(header.footer_offset))
        .map_err(|e| Error::custom_io("seeking to .pak footer offset", e))?;

    let table = Table::read_from(input, header.entry_details_version)?;
    table
        .read_entries(input)
        .map(|result| result.map(Into::into))
        .collect()
}
