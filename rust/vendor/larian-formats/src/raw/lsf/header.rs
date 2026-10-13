use crate::{
    error::{Error, Result},
    packed_version::UnpackedVersion,
    raw::read_bytes,
};
use std::io::{Read, Write};

static ID_BYTES: &[u8; 4] = b"LSOF";
pub(super) const MIN_SUPPORTED_LSF_VERSION: u32 = 0x6;

#[derive(Debug)]
pub(super) struct Header {
    pub(super) format_version: u32,
    pub(super) game_version: UnpackedVersion,
}

impl Header {
    pub(super) fn write(&self, output: &mut impl Write) -> Result<()> {
        output
            .write_all(ID_BYTES)
            .map_err(|e| Error::write_io(".lsf header", e))?;

        output
            .write_all(&self.format_version.to_le_bytes())
            .map_err(|e| Error::write_io(".lsf format version", e))?;

        output
            .write_all(&self.game_version.to_packed().to_le_bytes())
            .map_err(|e| Error::write_io(".lsf game version", e))?;

        Ok(())
    }

    pub(super) fn read(input: &mut impl Read) -> Result<Self> {
        let id_bytes = read_bytes(input).map_err(|e| Error::read_io(".lsf header", e))?;

        if id_bytes != *ID_BYTES {
            return Err(Error::InvalidIdentifier {
                expected_bytes: *ID_BYTES,
                bytes_found: id_bytes,
            });
        }

        let format_version = read_num!(u32, input, ".lsf format version")?;

        if format_version < MIN_SUPPORTED_LSF_VERSION {
            return Err(Error::UnsupportedVersion {
                version_number_found: format_version,
            });
        }

        let packed_game_version = read_num!(u64, input, ".lsf game version")?;

        Ok(Self {
            format_version,
            game_version: UnpackedVersion::from_packed(packed_game_version),
        })
    }
}
