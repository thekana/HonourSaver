macro_rules! read_ints_from_offset {
    ( $bytes:expr, $offset:expr, $( $int:ty : $ctx:expr ),+ $(,)? ) => {
        {
            let mut initial_offset = $offset;
            let offset = &mut initial_offset;

            (
                $({
                    let size = size_of::<$int>();
                    let out = read_num!($int, &mut &$bytes[*offset..*offset + size], $ctx)
                        .map_err(|_| Error::TooShort);
                    *offset += size;
                    out
                }),+
            )
        }
    };
}

mod read;
mod write;

pub(crate) use read::is_override;
#[cfg(feature = "raw")]
pub use read::{
    Info, Metadata, extract_file_from_pak, extract_file_from_reader, get_mod_info_from_pak,
    get_mod_info_from_reader,
};
pub use read::{list_mod_pack_files, unpack_files};
pub use write::write_packed_bytes;

use super::CompressionType;
use crate::error::{Error, Result};
use std::{
    borrow::Cow,
    io::{Read, Seek, SeekFrom},
};

pub(crate) static LSPK_ID_BYTES: &[u8; 4] = b"LSPK";

pub(crate) static VERSION_BYTES: [u8; 4] = 0x12_u32.to_le_bytes();

/// The length of the path field in the table entry for each file compressed into the LSPK file.
///
/// If the the path is shorter than this, it will be padded with 0 bytes up to this length.
pub static TABLE_ENTRY_PATH_LENGTH: usize = 256;

#[derive(Debug, Clone, Copy)]
enum EntryDetailsVersion {
    V15,
    V16,
    V18,
}

fn read_v18_entry_details<'a>(
    bytes: &'a [u8],
    entry_offset: usize,
    path: Cow<'a, [u8]>,
) -> Result<EntryDetails<'a>> {
    // Last two bytes are used for metadata, so parse as a u32 and a u16 and then
    // combine.
    let (offset_upper_bytes, offset_lower_bytes) = read_ints_from_offset!(
        bytes,
        entry_offset + TABLE_ENTRY_PATH_LENGTH,
        u32: format!(".pak offset upper bytes for {} file offset", String::from_utf8_lossy(&path)),
        u16: format!(".pak offset lower bytes for {} file offset", String::from_utf8_lossy(&path)),
    );

    let offset = u64::from(offset_upper_bytes?) | (u64::from(offset_lower_bytes?) << 32);

    // Bytes 262-263: ArchivePart (u8) and Flags (u8).
    // The Flags byte encodes compression method in the lower nibble and level in the upper.
    let (_archive_part, flags_byte) = read_ints_from_offset!(
        bytes,
        entry_offset + TABLE_ENTRY_PATH_LENGTH + 6,
        u8: format!(".pak archive part for {}", String::from_utf8_lossy(&path)),
        u8: format!(".pak compression flags for {}", String::from_utf8_lossy(&path)),
    );

    let compression_type =
        CompressionType::try_from(u32::from(flags_byte?) & 0x0F).map_err(|value| {
            Error::UnknownCompressionType {
                value,
                context: format!(".pak compression flags for {}", String::from_utf8_lossy(&path)),
            }
        })?;

    let (num_bytes_compressed, num_bytes_decompressed) = read_ints_from_offset!(
        bytes,
        entry_offset + TABLE_ENTRY_PATH_LENGTH + 8,
        u32: format!(".pak number of compressed bytes for {}", String::from_utf8_lossy(&path)),
        u32: format!(".pak number of decompressed bytes for {}", String::from_utf8_lossy(&path)),
    );

    let entry = EntryDetails {
        path,
        offset,
        num_bytes_compressed: num_bytes_compressed?.into(),
        num_bytes_decompressed: num_bytes_decompressed?.into(),
        compression_type,
    };

    Ok(entry)
}

fn read_v15_entry_details<'a>(
    mut bytes: &'a [u8],
    entry_offset: usize,
    path: Cow<'a, [u8]>,
) -> Result<EntryDetails<'a>> {
    let bytes = &mut bytes;

    let (offset, num_bytes_compressed, num_bytes_decompressed, _archive_part, flags_byte) = read_ints_from_offset!(
        bytes,
        entry_offset + TABLE_ENTRY_PATH_LENGTH,
        u64: format!(".pak file offset for {} ", String::from_utf8_lossy(&path)),
        u64: format!(".pak number of compressed bytes for {}", String::from_utf8_lossy(&path)),
        u64: format!(".pak number of decompressed bytes for {}", String::from_utf8_lossy(&path)),
        u32: format!(".pak archive part for for {}", String::from_utf8_lossy(&path)),
        u32: format!(".pak compression flags for for {}", String::from_utf8_lossy(&path)),
    );

    let compression_type = CompressionType::try_from(flags_byte? & 0x0F).map_err(|value| {
        Error::UnknownCompressionType {
            value,
            context: format!(".pak compression flags for {}", String::from_utf8_lossy(&path)),
        }
    })?;

    Ok(EntryDetails {
        path,
        offset: offset?,
        num_bytes_compressed: num_bytes_compressed?,
        num_bytes_decompressed: num_bytes_decompressed?,
        compression_type,
    })
}

impl EntryDetailsVersion {
    #[expect(clippy::identity_op, reason = "readability with newlines and comments")]
    fn decompressed_table_entry_length(self) -> usize {
        let details = match self {
            Self::V15 | Self::V16 => {
                0
                + 8 // Offset from the start of the file to the compressed file bytes
                + 8 // Number of compressed file bytes
                + 8 // Number of decompressed file bytes
                + 4 // Archive part
                + 4 // Flags
                + 4 // CRC
                + 4 // Unknown
            }
            Self::V18 => {
                0
                    + 6   // Offset from the start of the file to the compressed file bytes
                    + 1   // Archive
                    + 1   // Flags
                    + 4   // Number of compressed file bytes
                    + 4 // Number of decompressed file bytes
            }
        };

        details + TABLE_ENTRY_PATH_LENGTH // Relative path of the decompressed file (padded with trailing 0s)
    }

    fn read_from(input: &mut (impl Seek + Read)) -> Result<Self> {
        input
            .seek(SeekFrom::Start(4))
            .map_err(|e| Error::custom_io("seeking to .pak metadata version", e))?;

        let version = read_num!(u32, input, ".pak format version")?;

        match version {
            15 => Ok(Self::V15),
            16 => Ok(Self::V16),
            18 => Ok(Self::V18),
            _ => Err(Error::UnsupportedVersion {
                version_number_found: version,
            }),
        }
    }

    fn read_entry_details<'a>(
        self,
        bytes: &'a [u8],
        entry_offset: usize,
        path: Cow<'a, [u8]>,
    ) -> Result<EntryDetails<'a>> {
        match self {
            Self::V15 | Self::V16 => read_v15_entry_details(bytes, entry_offset, path),
            Self::V18 => read_v18_entry_details(bytes, entry_offset, path),
        }
    }
}

#[derive(Debug)]
struct EntryDetails<'a> {
    path: Cow<'a, [u8]>,
    offset: u64,
    num_bytes_compressed: u64,
    num_bytes_decompressed: u64,
    compression_type: CompressionType,
}

/// Represents a file entry in the LSPK archive.
#[derive(Debug)]
pub struct ModFile<'a> {
    pub path: Cow<'a, [u8]>,
    pub contents: Cow<'a, [u8]>,
}

impl<'a> ModFile<'a> {
    pub fn new(path: impl Into<Cow<'a, [u8]>>, contents: impl Into<Cow<'a, [u8]>>) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
        }
    }
}
