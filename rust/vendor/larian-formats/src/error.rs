//! Error types for `larian-formats`.

use std::{fmt::Display, num::ParseIntError, path::PathBuf};
use thiserror::Error;

/// Convenience type alias for `Result` with the default error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors that can occur when processing BG3 files.
#[derive(Debug, Error)]
pub enum Error {
    /// The parent index specified in the .lsf file was not found when parsing the file.
    #[error("parent index {index} in .lsf file not found when parsing {context}")]
    InvalidParentIndex { context: String, index: u32 },

    /// The compression type specified in the file is not supported.
    #[error("unknown compression type specified with {value} found when {context}")]
    UnknownCompressionType { value: u32, context: String },

    // The offset to the compressed file didn't correctly point to a location in the file.
    #[error(
        "compressed file offset {compressed_file_offset_found} doesn't specify a valid location \
         in the file"
    )]
    InvalidCompressedFileOffset { compressed_file_offset_found: u64 },

    /// The footer that should contain metadata did not properly indicate its length.
    #[error(
         "{} bytes found after footer offset {footer_offset_found}, but the footer indicated a length of {footer_length_found}",
         num_bytes - footer_offset_found
     )]
    InvalidFooterLength {
        footer_length_found: usize,
        footer_offset_found: usize,
        num_bytes: usize,
    },

    /// The offset to the footer didn't correctly point to a location in the file.
    #[error("offset {offset_found} doesn't specify a valid location in the file")]
    InvalidOffset { offset_found: u64 },

    /// Unable to decompress a file.
    #[error(
        "tried to LZ4 decompress {context} with compressed_length 0x{compressed_length:x} and \
         decompressed length 0x{decompressed_length:x} but got {error}"
    )]
    Lz4DecompressionFailed {
        context: String,
        error: lz4_flex::block::DecompressError,
        compressed_length: usize,
        decompressed_length: usize,
    },

    /// The number of entries in the file is too high to fit into a the given integer width.
    #[error(
        "during {context} got {value} entries which is too many to fit into a {bit_width}-bit \
         integer"
    )]
    NumberOfEntriesTooHigh {
        context: String,
        value: u64,
        bit_width: u32,
    },

    /// The compressed size of the file is too large to fit into a the given integer width.
    #[error(
        "during {context} got {value} bytes which is too large to fit into a {bit_width}-bit \
         integer"
    )]
    CompressedFileSizeTooHigh {
        context: String,
        value: u64,
        bit_width: u32,
    },

    /// The decompressed size of the file is too large to fit into a the given integer width.
    #[error(
        "during {context} got {value} bytes which is too large to fit into a {bit_width}-bit \
         integer"
    )]
    DecompressedFileSizeTooHigh {
        context: String,
        value: u64,
        bit_width: u32,
    },

    /// The compressed size of the metadata is too large to fit into a the given integer width.
    #[error(
        "during {context} got metadata length of {value} bytes which is too large to fit into a \
         {bit_width}-bit integer"
    )]
    CompressedMetadataSizeTooHigh {
        context: String,
        value: u64,
        bit_width: u32,
    },

    /// The length of
    #[error("string length {value} for {context} does not fit into {bit_width} bits")]
    StringLengthTooHigh {
        context: String,
        value: u64,
        bit_width: u32,
    },

    /// The index of the entry is too large to fit into the given integer width.
    #[error(
        "during {context} got index {value} which is too large to fit into a signed \
         {bit_width}-bit integer"
    )]
    IndexTooHigh {
        context: String,
        value: u32,
        bit_width: u32,
    },

    /// The "magic number" used to identify the type of file didn't match the expected value.
    #[error("found {bytes_found:?} instead of {bytes_found:?}")]
    InvalidIdentifier {
        expected_bytes: [u8; 4],
        bytes_found: [u8; 4],
    },

    /// The number of attributes in the mod info section doesn't match the expected count of 6.
    #[error("mod info had {found} attributes instead of 6")]
    InvalidModInfoAttributeCount { found: usize },

    /// A meta.lsx file was found within an LSPK .pak archive that didn't match the expected
    /// XML format.
    #[error("meta.lsx in the LSPK file didn't match expected schema due to {context}")]
    InvalidMetadataFile { context: String },

    /// The mod info section is missing the `"Folder"` attribute.
    #[error("mod info missing folder attribute")]
    MissingModInfoFolder,

    /// The mod info section is missing the `"MD5"` attribute.
    #[error("mod info missing md5 attribute")]
    MissingModInfoMd5,

    /// The mod info section is missing the `"Name"` attribute.
    #[error("mod info missing name attribute")]
    MissingModInfoName,

    /// The mod info section is missing the `"PublishHandle"` attribute.
    #[error("mod info missing publish_handle attribute")]
    MissingModInfoPublishHandle,

    /// The mod info section is missing the `"UUID"` attribute.
    #[error("mod info missing uuid attribute")]
    MissingModInfoUuid,

    /// The mod info section is missing the `"Version"` attribute.
    #[error("mod info missing version attribute")]
    MissingModInfoVersion,

    /// An I/O error occured.
    #[error("when {context} got {inner}")]
    Io {
        context: String,
        inner: std::io::Error,
    },

    /// The attribute with the given name was not found.
    #[error("no value found for attribute {name}")]
    MissingAttributeValue { name: &'static str },

    /// No meta.lsx file was found in an LSPK .pak file.
    /// A zipfile didn't contain any file with the .pak extension.
    #[error("no file with the correct extension was found in the zip")]
    MissingLspkFileInZip,

    /// No meta.lsx file was found in the LSPK file.
    #[error("no meta.lsx was found in the LSPK file")]
    MissingMetadataFile,

    /// The LSX file is missing the required `"ModuleInfo"` node.
    #[error(
        "no node found with XPath \
         /save/region[@id='Config']/node[@id='root']/children/node[@id='ModuleInfo']"
    )]
    MissingModuleInfo,

    /// The an integer value could not be parsed from the LSX file at the given location.
    #[error("could not parse {location} as integer because {inner}")]
    ParseIntError {
        inner: ParseIntError,
        location: &'static str,
    },

    /// An error occurred when trying to parse the path of a file within an LSPK .pak archive.
    #[error("tried to decompress table but got {0}")]
    TableEntryDecompressionFailed(lz4_flex::block::DecompressError),

    /// The file being read was too short to contain all of the expected metadata of the given
    /// format.
    #[error("not enough bytes to be a valid file of the given format")]
    TooShort,

    /// The version number read when parsing the file was lower than the minimum supported version.
    #[error("this tool does not support version number {version_number_found}")]
    UnsupportedVersion { version_number_found: u32 },

    /// The path isn't supported for packing from.
    #[error("can't pack mods at path `{0}`; try moving them to a named directory before packing")]
    UnsupportedModPath(PathBuf),

    /// An error occured when trying to read the data from a zipfile.
    #[cfg(feature = "zip")]
    #[error("{0}")]
    UnableToReadZipFile(#[from] rc_zip_sync::rc_zip::error::Error),

    /// Unable to decompress a file.
    #[error("tried to decompress {} but got {error}", path.display())]
    ZlibDecompressionFailed {
        path: PathBuf,
        error: std::io::Error,
    },

    #[error("tried to decompress {} but got {error}", path.display())]
    ZstdDecompressionFailed { path: PathBuf, error: String },

    /// The file contains zstd-compressed data, but this tool does not currently support it.
    #[error("zstd-compressed data in .pak not currently supported")]
    ZstdNotSupported,

    /// The LSX data could not be parsed.
    #[error("failed to deserialize XML: {0}")]
    XmlDeserialize(#[from] quick_xml::DeError),

    /// The LSX data could not be serialized.
    #[error("failed to serialize XML: {0}")]
    XmlSerialize(#[from] quick_xml::SeError),

    /// The LSX data could not be written to a file.
    #[error("failed to write XML: {0}")]
    XmlWrite(std::io::Error),

    /// LSF attribute is missing a required field.
    #[error("LSF XML attribute '{attribute_id}': missing required field '{field}'")]
    LsfXmlMissingField {
        attribute_id: String,
        field: &'static str,
    },

    /// LSF value has an unknown type.
    #[error("LSF XML attribute '{attribute_id}': unknown type '{type_name}'")]
    LsfXmlUnknownType {
        attribute_id: String,
        type_name: String,
    },

    /// LSF value has the wrong number of components.
    #[error(
        "LSF XML attribute '{attribute_id}': expected {expected} space-separated values but got \
         {found}"
    )]
    LsfXmlWrongComponentCount {
        attribute_id: String,
        expected: usize,
        found: usize,
    },

    /// LSF value failed to parse.
    #[error(
        "failed to parse LSF XML `{attribute_id}` attribute `{value}` as {expected_type} due to \
         {inner}"
    )]
    LsfXmlParse {
        attribute_id: String,
        expected_type: String,
        value: String,
        inner: String,
    },

    /// LSF value failed to decode base64.
    #[error("LSF XML attribute '{attribute_id}': failed to decode base64: {inner}")]
    LsfXmlDecodeBase64 {
        attribute_id: String,
        inner: base64::DecodeError,
    },
}

impl Error {
    pub fn custom_io(context: impl Display, inner: std::io::Error) -> Self {
        Self::Io {
            context: format!("{context}"),
            inner,
        }
    }

    pub fn read_io(context: impl Display, inner: std::io::Error) -> Self {
        Self::Io {
            context: format!("reading {context}"),
            inner,
        }
    }

    pub fn write_io(context: impl Display, inner: std::io::Error) -> Self {
        Self::Io {
            context: format!("writing {context}"),
            inner,
        }
    }
}
