use super::{data_type::DataType, metadata::MetadataFormat, names::Names};
use crate::error::{Error, Result};
use std::io::{Cursor, Write};

#[derive(Debug)]
pub(super) struct NodeInfo {
    pub(super) entry_inner_index: u16,
    pub(super) entry_outer_index: u16,
    pub(super) parent: Option<u32>,
    pub(super) next_sibling: Option<u32>,
    pub(super) first_attr: Option<u32>,
    pub(super) key_attribute: Option<String>,
}

impl NodeInfo {
    pub(super) fn read(
        format: MetadataFormat,
        bytes: &mut Cursor<&[u8]>,
        index: usize,
    ) -> Result<Self> {
        if format == MetadataFormat::KeysAndAdjacency {
            let entry_inner_index =
                read_num!(u16, bytes, format!(".lsf node info (v3) entry inner index {}", index))?;
            let entry_outer_index =
                read_num!(u16, bytes, format!(".lsf node info (v3) entry outer index {}", index))?;
            let parent =
                read_num!(i32, bytes, format!(".lsf node info (v3) parent index {}", index))?;
            let next_sibling =
                read_num!(i32, bytes, format!(".lsf node info (v3) sibling index {}", index))?;
            let first_attr =
                read_num!(i32, bytes, format!(".lsf node info (v3) first attr index {}", index))?;

            Ok(Self {
                entry_outer_index,
                entry_inner_index,
                parent: parent.try_into().ok(),
                next_sibling: next_sibling.try_into().ok(),
                first_attr: first_attr.try_into().ok(),
                key_attribute: None,
            })
        } else {
            let entry_inner_index =
                read_num!(u16, bytes, format!(".lsf node info (v2) entry inner index {}", index))?;
            let entry_outer_index =
                read_num!(u16, bytes, format!(".lsf node info (v2) entry outer index {}", index))?;
            let first_attr =
                read_num!(i32, bytes, format!(".lsf node info (v2) first attr index {}", index))?;
            let parent =
                read_num!(i32, bytes, format!(".lsf node info (v2) parent index {}", index))?;

            Ok(Self {
                entry_inner_index,
                entry_outer_index,
                parent: parent.try_into().ok(),
                next_sibling: None,
                first_attr: first_attr.try_into().ok(),
                key_attribute: None,
            })
        }
    }

    pub(super) fn write(self, format: MetadataFormat, output: &mut impl Write) -> Result<()> {
        write_num!(self.entry_inner_index, output, ".lsf node info entry inner index")?;
        write_num!(self.entry_outer_index, output, ".lsf node info entry outer index")?;

        if format == MetadataFormat::KeysAndAdjacency {
            let parent = index_to_i32(self.parent, ".lsf node info (v3) parent index")?;
            write_num!(parent, output, ".lsf node info (v3) parent index")?;
            let next_sibling =
                index_to_i32(self.next_sibling, ".lsf node info (v3) sibling index")?;
            write_num!(next_sibling, output, ".lsf node info (v3) sibling index")?;
            let first_attr = index_to_i32(self.first_attr, ".lsf node info (v3) first attr index")?;
            write_num!(first_attr, output, ".lsf node info (v3) first attr index")?;
        } else {
            let first_attr = index_to_i32(self.first_attr, ".lsf node info (v2) first attr index")?;
            write_num!(first_attr, output, ".lsf node info (v2) first attr index")?;
            let parent = index_to_i32(self.parent, ".lsf node info (v2) parent index")?;
            write_num!(parent, output, ".lsf node info (v2) parent index")?;
        }

        Ok(())
    }
}

pub(super) fn index_to_u32(value: usize, context: &str) -> Result<u32> {
    try_as_metadata_size!(
        u64_from_usize!(value) => u32,
        NumberOfEntriesTooHigh,
        context,
    )
}

fn index_to_i32(value: Option<u32>, context: &str) -> Result<i32> {
    let Some(value) = value else { return Ok(-1) };

    i32::try_from(value).map_err(|_| Error::IndexTooHigh {
        context: context.to_string(),
        value,
        bit_width: i32::BITS,
    })
}

#[derive(Debug)]
pub(super) struct AttributeInfo {
    pub(super) entry_inner_index: u16,
    pub(super) entry_outer_index: u16,
    pub(super) length: u32,
    pub(super) data_type: DataType,
    pub(super) next_attr_index: Option<u32>,
    pub(super) offset: Option<u32>,
    pub(super) node_index: Option<u32>,
}

impl AttributeInfo {
    pub(super) fn fmt_context(&self, info: &str) -> String {
        format!("attr[{}][{}] {info}", self.entry_outer_index, self.entry_inner_index)
    }

    pub(super) fn read(
        format: MetadataFormat,
        bytes: &mut Cursor<&[u8]>,
        index: usize,
    ) -> Result<Self> {
        if format == MetadataFormat::KeysAndAdjacency {
            let entry_inner_index = read_num!(
                u16,
                bytes,
                format!(".lsf node attribute info (v3) entry inner index {}", index)
            )?;
            let entry_outer_index = read_num!(
                u16,
                bytes,
                format!(".lsf node attribute info (v3) entry outer index {}", index)
            )?;

            let length_and_type = read_num!(
                u32,
                bytes,
                format!(".lsf node attribute info (v3) combined type and length index {}", index)
            )?;

            // First 26 bits are length, remaining 6 are type (https://github.com/Norbyte/lslib/blob/ff21c3feabaf54467170ce6f87357b1b8e3732d3/LSLib/LS/Resources/LSF/LSFCommon.cs#L370-L374)
            let length = length_and_type >> 6;
            let data_type = DataType::from_lower_six_bits(length_and_type);

            let next_attr_index = read_num!(
                i32,
                bytes,
                format!(".lsf node attribute info (v3) next attribute index {}", index)
            )?;

            let offset = read_num!(
                u32,
                bytes,
                format!(".lsf node attribute info (v3) next attribute index {}", index)
            )?;

            Ok(Self {
                entry_outer_index,
                entry_inner_index,
                length,
                data_type,
                next_attr_index: next_attr_index.try_into().ok(),
                offset: Some(offset),
                node_index: None,
            })
        } else {
            let entry_inner_index = read_num!(
                u16,
                bytes,
                format!(".lsf node attribute info (v2) entry inner index {}", index)
            )?;
            let entry_outer_index = read_num!(
                u16,
                bytes,
                format!(".lsf node attribute info (v2) entry outer index {}", index)
            )?;

            let length_and_type = read_num!(
                u32,
                bytes,
                format!(".lsf node attribute info (v2) combined type and length index {}", index)
            )?;

            // First 26 bits are length, remaining 6 are type (https://github.com/Norbyte/lslib/blob/ff21c3feabaf54467170ce6f87357b1b8e3732d3/LSLib/LS/Resources/LSF/LSFCommon.cs#L370-L374)
            let length = length_and_type >> 6;
            let data_type = DataType::from_lower_six_bits(length_and_type);

            let node_index = read_num!(
                i32,
                bytes,
                format!(".lsf node attribute info (v2) node index {}", index)
            )?;

            Ok(Self {
                entry_inner_index,
                entry_outer_index,
                length,
                data_type,
                next_attr_index: None,
                node_index: node_index.try_into().ok(),
                offset: None,
            })
        }
    }

    pub(super) fn write(self, format: MetadataFormat, output: &mut impl Write) -> Result<()> {
        write_num!(self.entry_inner_index, output, ".lsf node attribute info entry inner index")?;
        write_num!(self.entry_outer_index, output, ".lsf node attribute info entry outer index")?;

        let length_and_type = (self.length << 6) | self.data_type.as_u32();

        if format == MetadataFormat::KeysAndAdjacency {
            write_num!(
                length_and_type,
                output,
                ".lsf node attribute info (v3) combined type and length"
            )?;
            let next_attr_index = index_to_i32(
                self.next_attr_index,
                ".lsf node attribute info (v3) next attribute index",
            )?;
            write_num!(
                next_attr_index,
                output,
                ".lsf node attribute info (v3) next attribute index"
            )?;
            write_num!(self.offset.unwrap_or(0), output, ".lsf node attribute info (v3) offset")?;
        } else {
            write_num!(
                length_and_type,
                output,
                ".lsf node attribute info (v2) combined type and length"
            )?;
            let node_index =
                index_to_i32(self.node_index, ".lsf node attribute info (v2) node index")?;
            write_num!(node_index, output, ".lsf node attribute info (v2) node index")?;
        }

        Ok(())
    }
}

pub(super) fn update_nodes(bytes: &[u8], names: &Names, nodes: &mut [NodeInfo]) -> Result<()> {
    let end = u64::try_from(bytes.len())
        .expect("platforms where usize is larger than 64 bits are not supported");
    let bytes = &mut Cursor::new(bytes);

    while bytes.position() < end {
        let entry_start = bytes.position();

        let node_index = read_num!(
            u32,
            bytes,
            format!(".lsf node index for key entry starting at offset {entry_start}")
        )?;

        let inner_index = read_num!(
            u16,
            bytes,
            format!(".lsf inner index for key entry starting at offset {entry_start}")
        )?;

        let outer_index = read_num!(
            u16,
            bytes,
            format!(".lsf outer index for key entry starting at offset {entry_start}")
        )?;

        let name = names.inner[usize::from(outer_index)][usize::from(inner_index)].clone();
        nodes[usize_from_u32!(node_index)].key_attribute = Some(name);
    }

    Ok(())
}

pub(super) fn write_keys(nodes: &[NodeInfo], names: &Names) -> Result<Vec<u8>> {
    let mut output = Vec::new();

    for (node_idx, node) in nodes.iter().enumerate() {
        let Some(key_attr_name) = &node.key_attribute else {
            continue;
        };

        let (outer, inner) = names
            .inner
            .iter()
            .enumerate()
            .find_map(|(outer_idx, bucket)| {
                let outer = u16::try_from(outer_idx).ok()?;
                bucket
                    .iter()
                    .position(|s| s == key_attr_name)
                    .and_then(|inner_idx| u16::try_from(inner_idx).ok())
                    .map(|inner| (outer, inner))
            })
            .ok_or_else(|| {
                Error::custom_io(
                    format!("key attribute '{key_attr_name}' not found in LSF names table"),
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "key not found"),
                )
            })?;

        let node_u32 = index_to_u32(node_idx, "LSF key node index")?;
        output.extend_from_slice(&node_u32.to_le_bytes());
        output.extend_from_slice(&inner.to_le_bytes());
        output.extend_from_slice(&outer.to_le_bytes());
    }

    Ok(output)
}
