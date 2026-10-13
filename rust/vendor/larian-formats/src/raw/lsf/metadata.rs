use crate::{
    error::{Error, Result},
    raw::CompressionType,
};
use std::io::{Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetadataFormat {
    None,
    KeysAndAdjacency,
}

impl From<u32> for MetadataFormat {
    fn from(value: u32) -> Self {
        if value == 1 {
            Self::KeysAndAdjacency
        } else {
            Self::None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CompressionSizes {
    pub(super) num_bytes_compressed: u32,
    pub(super) num_bytes_decompressed: u32,
}

impl CompressionSizes {
    pub(super) fn uncompressed(value: usize, context: &str) -> Result<Self> {
        u32::try_from(value)
            .map(|n| Self {
                num_bytes_compressed: 0,
                num_bytes_decompressed: n,
            })
            .map_err(|_| Error::CompressedFileSizeTooHigh {
                context: context.to_string(),
                value: u64_from_usize!(value),
                bit_width: u32::BITS,
            })
    }
}

macro_rules! read_compression_sizes {
    ( $ctx:literal, $input:expr ) => {{
        let num_bytes_decompressed =
            read_num!(u32, $input, concat!($ctx, " num_bytes_decompressed"))?;
        let num_bytes_compressed = read_num!(u32, $input, concat!($ctx, " num_bytes_compressed"))?;

        CompressionSizes {
            num_bytes_compressed,
            num_bytes_decompressed,
        }
    }};
}

macro_rules! write_compression_sizes {
    ( $sizes:expr, $ctx:literal, $output:expr ) => {
        write_num!(
            $sizes.num_bytes_decompressed,
            $output,
            concat!($ctx, " num_bytes_decompressed")
        )?;
        write_num!($sizes.num_bytes_compressed, $output, concat!($ctx, " num_bytes_compressed"))?;
    };
}

#[derive(Debug, Clone)]
pub(crate) struct Metadata {
    pub(super) names: CompressionSizes,
    pub(super) keys: CompressionSizes,
    pub(super) nodes: CompressionSizes,
    pub(super) attributes: CompressionSizes,
    pub(super) values: CompressionSizes,
    pub(super) compression: CompressionType,
    pub(super) format: MetadataFormat,
}

impl Metadata {
    pub(super) fn write(&self, output: &mut impl Write) -> Result<()> {
        write_compression_sizes!(self.names, ".lsf strings", output);
        write_compression_sizes!(self.keys, ".lsf keys", output);
        write_compression_sizes!(self.nodes, ".lsf nodes", output);
        write_compression_sizes!(self.attributes, ".lsf attributes", output);
        write_compression_sizes!(self.values, ".lsf values", output);

        output
            .write_all(&self.compression.as_u32().to_le_bytes())
            .map_err(|e| Error::write_io(".lsf compression type", e))?;

        let format: u32 = match self.format {
            MetadataFormat::None => 0,
            MetadataFormat::KeysAndAdjacency => 1,
        };
        write_num!(format, output, ".lsf metadata format")?;

        Ok(())
    }

    pub(super) fn read(input: &mut impl Read) -> Result<Self> {
        let strings = read_compression_sizes!(".lsf strings", input);
        let keys = read_compression_sizes!(".lsf keys", input);
        let nodes = read_compression_sizes!(".lsf nodes", input);
        let attributes = read_compression_sizes!(".lsf attributes", input);
        let values = read_compression_sizes!(".lsf values", input);

        let compression = read_num!(u32, input, ".lsf compression type").and_then(|c| {
            CompressionType::try_from(c & 0b11).map_err(|value| Error::UnknownCompressionType {
                value,
                context: "reading compression type for .lsf file".into(),
            })
        })?;

        let format = read_num!(u32, input, ".lsf metadata format")?;

        Ok(Self {
            names: strings,
            keys,
            nodes,
            attributes,
            values,
            compression,
            format: format.into(),
        })
    }
}
