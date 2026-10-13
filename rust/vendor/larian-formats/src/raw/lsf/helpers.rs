use crate::error::{Error, Result};
use std::io::Write;

macro_rules! read_section {
    ( $context:literal, $sizes:expr, $compression_type:expr, $input:expr, $frame_decoding:expr ) => {{
        if $sizes.num_bytes_decompressed == 0 {
            Vec::new()
        } else {
            match $compression_type {
                CompressionType::None => {
                    let mut bytes = vec![0; usize_from_u32!($sizes.num_bytes_decompressed)];

                    $input.read_exact(&mut bytes).map_err(|e| {
                        Error::read_io(concat!(".lsf ", $context, " section (uncompressed)"), e)
                    })?;

                    bytes
                }
                CompressionType::Zlib => todo!(),
                CompressionType::Lz4 if $frame_decoding => {
                    let mut input = vec![0; usize_from_u32!($sizes.num_bytes_compressed)];
                    let mut output =
                        Vec::with_capacity(usize_from_u32!($sizes.num_bytes_decompressed));

                    $input.read_exact(&mut input).map_err(|e| {
                        Error::read_io(
                            concat!(".lsf reading ", $context, " section (LZ4 frame compressed)"),
                            e,
                        )
                    })?;

                    std::io::copy(&mut FrameDecoder::new(&mut &input[..]), &mut output).map_err(
                        |e| {
                            Error::read_io(
                                concat!(
                                    ".lsf decompression ",
                                    $context,
                                    " section (LZ4 frame compressed)"
                                ),
                                e,
                            )
                        },
                    )?;

                    output
                }
                CompressionType::Lz4 => {
                    let mut bytes = vec![0; usize_from_u32!($sizes.num_bytes_compressed)];

                    $input.read_exact(&mut bytes).map_err(|e| {
                        Error::read_io(concat!(".lsf ", $context, " section (LZ4 compressed)"), e)
                    })?;

                    lz4_flex::block::decompress(
                        &bytes,
                        usize_from_u32!($sizes.num_bytes_decompressed),
                    )
                    .map_err(|e| Error::Lz4DecompressionFailed {
                        context: format!(".lsf {} section", $context),
                        error: e,
                        compressed_length: usize_from_u32!($sizes.num_bytes_compressed),
                        decompressed_length: usize_from_u32!($sizes.num_bytes_decompressed),
                    })?
                }
                CompressionType::Zstd => return Err(Error::ZstdNotSupported),
            }
        }
    }};
}

macro_rules! write_section {
    ( $context:literal, $sizes:expr, $compression_type:expr, $output:expr, $value:expr, $frame_encoding:expr $(, $pass_context:expr )? ) => {{
        match $compression_type {
            CompressionType::None => $value.write(&mut $output, $( $pass_context )?)?,
            CompressionType::Zlib => todo!(),
            CompressionType::Lz4 if $frame_encoding => {
                $value.write(&mut FrameEncoder::new(&mut $output), $( $pass_context )?)?
            }
            CompressionType::Lz4 => {
                let mut bytes = vec![0; usize_from_u32!($sizes.num_bytes_decompressed)];

                $value.write(&mut bytes, $( $pass_context )?)?;

                let bytes = lz4_flex::block::compress(&bytes);

                $output.write_all(&bytes).map_err(|e| {
                    Error::write_io(format!(".lsf {} section (LZ compressed)", $context), e)
                })?;
            }
            CompressionType::Zstd => return Err(Error::ZstdNotSupported),
        }
    }};
}

macro_rules! read_items {
    ( $item:ty, $name:literal, $metadata:expr, $sizes:expr, $input:expr) => {{
        let bytes = read_section!($name, $sizes, $metadata.compression, $input, true);

        let end = u64::try_from(bytes.len())
            .expect("platforms where usize is larger than 64 bits are not supported");
        let bytes = &mut Cursor::new(bytes.as_slice());
        let mut items = Vec::new();

        while bytes.position() < end {
            let item = <$item>::read($metadata.format, bytes, items.len())?;
            items.push(item);
        }

        items
    }};
}

pub(super) struct Items {
    pub(super) bytes: Vec<u8>,
}

impl Items {
    pub(super) fn write(self, output: &mut impl Write, context: &str) -> Result<()> {
        output
            .write_all(&self.bytes)
            .map_err(|e| Error::write_io(format!(".lsf {context} section (LZ compressed)"), e))
    }
}

macro_rules! write_items {
    ( $name:literal, $items:expr, $format:expr, $sizes:expr, $compression_type:expr, $output:expr) => {{
        if $sizes.num_bytes_decompressed == 0 {
            Ok(())
        } else if $compression_type == CompressionType::None {
            $items
                .into_iter()
                .map(|item| item.write($format, $output))
                .collect()
        } else {
            let mut bytes = Vec::with_capacity(usize_from_u32!($sizes.num_bytes_decompressed));

            for item in $items {
                item.write($format, &mut bytes)?;
            }

            write_section!(
                $name,
                $sizes,
                $compression_type,
                $output,
                crate::raw::lsf::helpers::Items { bytes },
                true,
                $name
            );

            Ok(())
        }
    }};
}
