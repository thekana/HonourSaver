#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
mod common;

mod bg3;
pub mod error;
pub mod loca;
pub mod lsf;
pub mod lspk;
mod packed_version;

#[cfg(feature = "raw")]
pub mod raw;
#[cfg(not(feature = "raw"))]
mod raw;

pub use packed_version::UnpackedVersion;
