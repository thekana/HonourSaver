//! Low-level utilities for reading and writing LSF/LSPK files
macro_rules! read_num {
    ( $int:ty, $input:expr, $desc:expr ) => {
        crate::raw::read_bytes($input)
            .map(<$int>::from_le_bytes)
            .map_err(|e| $crate::error::Error::read_io($desc.to_string(), e))
    };
}

macro_rules! write_num {
    ( $int:expr, $output:expr, $desc:expr ) => {
        $output
            .write_all(&$int.to_le_bytes())
            .map_err(|e| $crate::error::Error::write_io($desc.to_string(), e))
    };
}

pub(crate) mod loca;
mod lsf;
mod lspk;

use crate::error::Result;
pub use lsf::LsfData;
pub use lspk::*;
use std::io::Read;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum CompressionType {
    None,
    Zlib,
    Lz4,
    Zstd,
}

impl CompressionType {
    const fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Zlib => 1,
            Self::Lz4 => 2,
            Self::Zstd => 3,
        }
    }
}

impl TryFrom<u32> for CompressionType {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, u32> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Zlib),
            2 => Ok(Self::Lz4),
            3 => Ok(Self::Zstd),
            other => Err(other),
        }
    }
}

fn read_bytes<const N: usize>(input: &mut impl Read) -> std::io::Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    input.read_exact(&mut bytes).map(|()| bytes)
}
