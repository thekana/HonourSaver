use crate::{
    error::{Error, Result},
    raw::lspk::EntryDetailsVersion,
};
use std::io::{Seek, Write};

use super::{EntryDetails, LSPK_ID_BYTES, ModFile, TABLE_ENTRY_PATH_LENGTH, VERSION_BYTES};
use crate::raw::CompressionType;

fn write_files<'a>(
    files: impl IntoIterator<Item = ModFile<'a>>,
    mut output: impl Seek + Write,
) -> Result<Vec<EntryDetails<'a>>> {
    files
        .into_iter()
        .map(|file| {
            let num_bytes_decompressed_usize = file.contents.len();
            let compressed_bytes = lz4_flex::block::compress(&file.contents);

            let num_bytes_compressed_usize = compressed_bytes.len();

            let num_bytes_decompressed = try_as_decompression_size!(
                u64_from_usize!(num_bytes_decompressed_usize) => u32,
                format!(".pak file {}", String::from_utf8_lossy(&file.path))
            )?;

            let num_bytes_compressed = try_as_compression_size!(
                u64_from_usize!(num_bytes_compressed_usize) => u32,
                format!(".pak file {}", String::from_utf8_lossy(&file.path))
            )?;

            let offset = output.stream_position().map_err(|e| {
                Error::custom_io(
                    format!("recording offset to {} in .pak", String::from_utf8_lossy(&file.path)),
                    e,
                )
            })?;
            output.write_all(&compressed_bytes).map_err(|e| {
                Error::write_io(
                    format!("compressed {} into .pak", String::from_utf8_lossy(&file.path)),
                    e,
                )
            })?;

            Ok(EntryDetails {
                path: file.path,
                num_bytes_compressed: num_bytes_compressed.into(),
                num_bytes_decompressed: num_bytes_decompressed.into(),
                offset,
                compression_type: CompressionType::Lz4,
            })
        })
        .collect()
}

fn generate_compressed_table(entry_details: Vec<EntryDetails>) -> Result<Vec<u8>> {
    let table_size =
        EntryDetailsVersion::V18.decompressed_table_entry_length() * entry_details.len();

    let mut table = Vec::with_capacity(table_size);

    for entry in entry_details {
        let table_offset = table.len();

        // Write path bytes
        table.extend(entry.path.as_ref());

        // Pad to expected path length
        table.resize(table_offset + TABLE_ENTRY_PATH_LENGTH, 0);

        // Write offset bytes in expected format
        table.extend(&(entry.offset & u64::from(u32::MAX)).to_le_bytes()[..4]);
        table.extend(&(entry.offset >> 32).to_le_bytes()[6..]);

        // Write compression flags
        table.push(0x0);
        table.push(0x12);

        let num_bytes_compressed = try_as_compression_size!(
            entry.num_bytes_compressed => u32,
            format!("writing entry details for {}", String::from_utf8_lossy(&entry.path))
        )?;

        let num_bytes_decompressed = try_as_decompression_size!(
            entry.num_bytes_decompressed => u32,
            format!("writing entry details for {}", String::from_utf8_lossy(&entry.path))
        )?;

        // Write compressed length
        table.extend(num_bytes_compressed.to_le_bytes());

        // Write uncompressed length
        table.extend(num_bytes_decompressed.to_le_bytes());
    }

    Ok(lz4_flex::block::compress(&table))
}

/// Writes the bytes of each file into the output stream compressed using LZ4.
///
/// # Errors
///
/// Returns an error if writing to the output stream fails, if the output stream is not seekable, if
/// the compression fails, or if the number of entries can't fit into a u32.
pub fn write_packed_bytes<'a>(
    files: impl IntoIterator<Item = ModFile<'a>>,
    mut output: impl Seek + Write,
) -> Result<()> {
    output
        .write_all(LSPK_ID_BYTES)
        .map_err(|e| Error::write_io("ID bytes into .pak".to_string(), e))?;
    output
        .write_all(&VERSION_BYTES)
        .map_err(|e| Error::write_io("format version bytes into .pak".to_string(), e))?;
    output
        .seek(std::io::SeekFrom::Start(38))
        .map_err(|e| Error::custom_io("seeking to end of .pak header".to_string(), e))?;

    output
        .write_all(&[0x1, 0x0])
        .map_err(|e| Error::write_io(" number of .pak parts".to_string(), e))?;

    let entry_details = write_files(files, &mut output)?;
    let footer_offset = output
        .stream_position()
        .map_err(|e| Error::custom_io("recording .pak footer offset".to_string(), e))?;

    let num_entries_usize = entry_details.len();

    let num_entries_u32 = try_as_metadata_size!(
        u64_from_usize!(num_entries_usize) => u32,
        NumberOfEntriesTooHigh,
        ".pak file"
    )?;

    output
        .write_all(&num_entries_u32.to_le_bytes())
        .map_err(|e| Error::write_io("number of compressed files stored in .pak".to_string(), e))?;

    let compressed_table = generate_compressed_table(entry_details)?;

    let num_compressed_table_bytes = try_as_compression_size!(
        u64_from_usize!(compressed_table.len()) => u32,
        ".pak compressed table"
    )?;

    output
        .write_all(&num_compressed_table_bytes.to_le_bytes())
        .map_err(|e| Error::write_io("number of compressed .pak table bytes".to_string(), e))?;
    output
        .write_all(&compressed_table)
        .map_err(|e| Error::write_io("compressed .pak table".to_string(), e))?;

    output.seek(std::io::SeekFrom::Start(8)).map_err(|e| {
        Error::custom_io("seeking back to header to record .pak footer metadata".to_string(), e)
    })?;
    output
        .write_all(&footer_offset.to_le_bytes())
        .map_err(|e| Error::write_io(".pak footer offset".to_string(), e))?;
    output
        .write_all(&(num_compressed_table_bytes + 8).to_le_bytes())
        .map_err(|e| Error::write_io("footer length .pak header metadata".to_string(), e))?;

    Ok(())
}
