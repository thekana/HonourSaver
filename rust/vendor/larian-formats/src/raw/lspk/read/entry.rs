use std::{
    borrow::Cow,
    io::{Read, Seek, SeekFrom},
};

use flate2::read::ZlibDecoder;

use crate::{
    common::pathbuf_from_bytes,
    error::{Error, Result},
    raw::{CompressionType, ModFile, lspk::EntryDetails},
};

impl<'a> EntryDetails<'a> {
    pub(super) fn decompress_file(
        self,
        mut reader: impl Read + Seek,
        compression_type: CompressionType,
    ) -> Result<ModFile<'a>> {
        reader.seek(SeekFrom::Start(self.offset)).map_err(|e| {
            Error::custom_io(
                format!("seeking {} offset in .pak", String::from_utf8_lossy(&self.path)),
                e,
            )
        })?;

        let num_bytes_compressed = try_as_compression_size!(
            self.num_bytes_compressed => usize,
            format!(
                "decompressing of {} from .pak",
                String::from_utf8_lossy(&self.path)
            )
        )?;

        let num_bytes_decompressed = try_as_decompression_size!(
            self.num_bytes_decompressed => usize,
            format!(
                "decompressing of {} from .pak",
                String::from_utf8_lossy(&self.path)
            )
        )?;

        let mut compressed_bytes = vec![0_u8; num_bytes_compressed];
        reader.read_exact(&mut compressed_bytes).map_err(|e| {
            Error::read_io(
                format!("compressed {} from .pak", String::from_utf8_lossy(&self.path)),
                e,
            )
        })?;

        if self.num_bytes_decompressed == 0 || compression_type == CompressionType::None {
            return Ok(ModFile {
                path: self.path,
                contents: Cow::Owned(compressed_bytes),
            });
        }

        let decompressed_bytes = match compression_type {
            CompressionType::Zlib => {
                let mut compressed_bytes = compressed_bytes.as_slice();
                let mut decoder = ZlibDecoder::new(&mut compressed_bytes);
                let mut decompressed_bytes = vec![0; num_bytes_decompressed];

                decoder
                    .read_exact(&mut decompressed_bytes)
                    .map_err(|error| Error::ZlibDecompressionFailed {
                        path: pathbuf_from_bytes(&self.path),
                        error,
                    })?;
                decompressed_bytes
            }
            CompressionType::Zstd => return Err(Error::ZstdNotSupported),
            CompressionType::Lz4 | CompressionType::None => {
                lz4_flex::block::decompress(&compressed_bytes, num_bytes_decompressed).map_err(
                    |error| Error::Lz4DecompressionFailed {
                        context: format!(
                            "file {} at offset {}",
                            pathbuf_from_bytes(&self.path).display(),
                            self.offset,
                        ),
                        error,
                        decompressed_length: num_bytes_decompressed,
                        compressed_length: compressed_bytes.len(),
                    },
                )?
            }
        };

        Ok(ModFile {
            path: self.path,
            contents: Cow::Owned(decompressed_bytes),
        })
    }
}
