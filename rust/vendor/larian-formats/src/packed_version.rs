use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// A full version number used by BG3 for both the game version and individual mods.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct UnpackedVersion {
    #[serde(rename = "@major")]
    pub major: u8,
    #[serde(rename = "@minor")]
    pub minor: u8,
    #[serde(rename = "@revision")]
    pub rev: u16,
    #[serde(rename = "@build")]
    pub build: u32,
}

impl UnpackedVersion {
    pub fn to_packed(self) -> u64 {
        ((u64::from(self.major) & 0x7f) << 55) |
            (u64::from(self.minor) << 47) |
            (u64::from(self.rev) << 31) |
            (u64::from(self.build) & 0x7fff_ffff)
    }

    pub const fn from_packed(value: u64) -> Self {
        // Source: https://github.com/Norbyte/lslib/blob/ff21c3feabaf54467170ce6f87357b1b8e3732d3/LSLib/LS/Resource.cs#L16
        Self {
            major: ((value >> 55) & 0x7f) as _,
            minor: ((value >> 47) & 0xff) as _,
            rev: ((value >> 31) & 0xffff) as _,
            build: (value & 0x7fff_ffff) as _,
        }
    }
}

impl Display for UnpackedVersion {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "{}.{}.{}.{}", self.major, self.minor, self.rev, self.build)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_casing::test_casing;

    const ROUND_TRIP_CASES: &[UnpackedVersion] = &[
        UnpackedVersion {
            major: 0,
            minor: 0,
            rev: 0,
            build: 0,
        },
        UnpackedVersion {
            major: u8::MAX >> 1,  // Max value for 7 bits
            minor: u8::MAX,       // Max value for 8 bits
            rev: u16::MAX,        // Max value for 16 bits
            build: u32::MAX >> 1, // Max value for 31 bits
        },
        UnpackedVersion {
            major: 4,
            minor: 0,
            rev: 9,
            build: 600,
        },
    ];

    #[test_casing(3, ROUND_TRIP_CASES)]
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "test_casing macro requires borrow")]
    fn test_game_version_round_trip(&version: &UnpackedVersion) {
        let packed = version.to_packed();
        let unpacked = UnpackedVersion::from_packed(packed);
        assert_eq!(version, unpacked);
    }

    const MASK_CASES: &[(UnpackedVersion, UnpackedVersion)] = &[
        (
            UnpackedVersion {
                major: 200, // Has bit 7 set, should be masked to 7 bits
                minor: 0,
                rev: 0,
                build: 0,
            },
            UnpackedVersion {
                #[expect(
                    clippy::decimal_bitwise_operands,
                    reason = "consistency with .lsx representation"
                )]
                major: 200 & 0x7f, // Should be masked to 72
                minor: 0,
                rev: 0,
                build: 0,
            },
        ),
        (
            UnpackedVersion {
                major: 0,
                minor: 0,
                rev: 0,
                build: 0xffff_ffff, // Has bit 31 set, should be masked to 31 bits
            },
            UnpackedVersion {
                major: 0,
                minor: 0,
                rev: 0,
                build: 0x7fff_ffff, // Should be masked
            },
        ),
    ];

    #[test_casing(2, MASK_CASES)]
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "test_casing macro requires borrow")]
    fn test_game_version_masks_invalid_bits(&input: &UnpackedVersion, &expected: &UnpackedVersion) {
        let packed = input.to_packed();
        let unpacked = UnpackedVersion::from_packed(packed);
        assert_eq!(expected, unpacked);
    }
}
