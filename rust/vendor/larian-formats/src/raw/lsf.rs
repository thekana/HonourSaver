mod data_type;
mod float;
mod header;
#[macro_use]
mod helpers;
mod metadata;
mod model;
mod names;
mod node;
mod parse;

use self::{
    header::Header,
    helpers::Items,
    metadata::{CompressionSizes, Metadata, MetadataFormat},
    model::NodeContent,
    names::Names,
    node::{AttributeInfo, NodeInfo, index_to_u32, update_nodes, write_keys},
    parse::{Attribute, AttributeParser, Parser},
};
use super::CompressionType;
use crate::{
    error::{Error, Result},
    packed_version::UnpackedVersion,
};
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use serde::Serialize as _;
use std::{
    borrow::Cow,
    collections::HashMap,
    io::{Cursor, Read, Write},
};

#[derive(Debug)]
struct Intermediate {
    header: Header,
    metadata: Metadata,
    names: Names,
    nodes: Vec<NodeInfo>,
    attributes: Vec<AttributeInfo>,
    values: Vec<u8>,
    keys: Option<Vec<u8>>,
}

type GraphData = (Names, Vec<NodeInfo>, Vec<AttributeInfo>, Vec<u8>);

fn build_parent_of(children_indexes: &[Vec<usize>], len: usize) -> Result<Vec<Option<u32>>> {
    let mut parent_of: Vec<Option<u32>> = vec![None; len];
    for (parent_idx, children) in children_indexes.iter().enumerate() {
        let parent_u32 = index_to_u32(parent_idx, "LSF parent node index")?;
        for &child_idx in children {
            parent_of[child_idx] = Some(parent_u32);
        }
    }
    Ok(parent_of)
}

// Build next_sibling_of: for each region, what is the next sibling's region index.
// Only used in V3 (KeysAndAdjacency) format; V2 (None) nodes don't include this field.
fn build_next_sibling_of(children_indexes: &[Vec<usize>], len: usize) -> Result<Vec<Option<u32>>> {
    let mut next_sibling_of: Vec<Option<u32>> = vec![None; len];
    for children in children_indexes {
        for window in children.windows(2) {
            let next_u32 = index_to_u32(window[1], "LSF sibling node index")?;
            next_sibling_of[window[0]] = Some(next_u32);
        }
    }
    Ok(next_sibling_of)
}

fn build_graph_data(
    lsf: &LsfData,
    format: MetadataFormat,
    parent_of: &[Option<u32>],
    next_sibling_of: &[Option<u32>],
) -> Result<GraphData> {
    let mut name_builder = NameBuilder::default();
    let mut nodes_out: Vec<NodeInfo> = Vec::with_capacity(lsf.regions.len());
    let mut attributes_out: Vec<AttributeInfo> = Vec::new();
    let mut values: Vec<u8> = Vec::new();

    for (region_idx, named_region) in lsf.regions.iter().enumerate() {
        let (name_outer, name_inner) = name_builder.insert(&named_region.name)?;
        let region = &named_region.value;

        // Insert the key attribute name into the names table so write_keys can look it up
        if let Some(key_attr_name) = &region.key_attribute {
            name_builder.insert(key_attr_name)?;
        }

        // Record the index of this node's first attribute in the attributes array
        let first_attr = if region.attributes.is_empty() {
            None
        } else {
            Some(index_to_u32(attributes_out.len(), "LSF first attribute index")?)
        };

        let attr_start_idx = attributes_out.len();
        let node_u32 = index_to_u32(region_idx, "LSF node index for attribute")?;
        for (i, named_attr) in region.attributes.iter().enumerate() {
            let (attr_outer, attr_inner) = name_builder.insert(&named_attr.name)?;

            let offset = try_as_compression_size!(
                u64_from_usize!(values.len()) => u32,
                "LSF attribute value offset"
            )?;

            let start = values.len();

            named_attr.value.write_lsf_value(&mut values)?;

            let length = u32::try_from(values.len() - start).map_err(|_| {
                Error::CompressedFileSizeTooHigh {
                    context: "LSF attribute value length".into(),
                    value: u64_from_usize!(values.len() - start),
                    bit_width: u32::BITS,
                }
            })?;

            let (next_attr_index, offset_field, node_index) =
                if format == MetadataFormat::KeysAndAdjacency {
                    let next = if i + 1 < region.attributes.len() {
                        Some(index_to_u32(attr_start_idx + i + 1, "LSF attribute next index")?)
                    } else {
                        None
                    };
                    (next, Some(offset), None)
                } else {
                    (None, None, Some(node_u32))
                };

            attributes_out.push(AttributeInfo {
                entry_inner_index: attr_inner,
                entry_outer_index: attr_outer,
                length,
                data_type: named_attr.value.data_type(),
                next_attr_index,
                offset: offset_field,
                node_index,
            });
        }

        nodes_out.push(NodeInfo {
            entry_inner_index: name_inner,
            entry_outer_index: name_outer,
            parent: parent_of[region_idx],
            next_sibling: next_sibling_of[region_idx],
            first_attr,
            key_attribute: region.key_attribute.clone(),
        });
    }

    Ok((name_builder.build(), nodes_out, attributes_out, values))
}

impl Intermediate {
    fn write_lsf(self, mut output: &mut impl Write) -> Result<()> {
        self.header.write(output)?;
        self.metadata.write(output)?;

        let compression = self.metadata.compression;

        write_section!("names", self.metadata.names, compression, output, self.names, false);
        write_items!(
            "nodes",
            self.nodes,
            self.metadata.format,
            self.metadata.nodes,
            compression,
            output
        )?;

        write_items!(
            "attributes",
            self.attributes,
            self.metadata.format,
            self.metadata.attributes,
            compression,
            output
        )?;

        write_section!(
            "values",
            self.metadata.values,
            compression,
            output,
            Items { bytes: self.values },
            true,
            "values"
        );

        if let Some(keys) = self.keys {
            write_section!(
                "keys",
                self.metadata.keys,
                compression,
                output,
                Items { bytes: keys },
                true,
                "keys"
            );
        }

        Ok(())
    }

    fn from_lsf_data(lsf: &LsfData) -> Result<Self> {
        let has_keys = lsf.regions.iter().any(|r| r.value.key_attribute.is_some());

        let format = if lsf.use_adjacency_format || has_keys {
            MetadataFormat::KeysAndAdjacency
        } else {
            MetadataFormat::None
        };

        let compression = CompressionType::None;
        let parent_of = build_parent_of(&lsf.children_indexes, lsf.regions.len())?;
        let next_sibling_of = if format == MetadataFormat::KeysAndAdjacency {
            build_next_sibling_of(&lsf.children_indexes, lsf.regions.len())?
        } else {
            vec![None; lsf.regions.len()]
        };

        let (names, nodes_out, attributes_out, values) =
            build_graph_data(lsf, format, &parent_of, &next_sibling_of)?;
        let names_len = names.byte_len();

        let keys = if format == MetadataFormat::KeysAndAdjacency {
            Some(write_keys(&nodes_out, &names)?)
        } else {
            None
        };

        // V3 (KeysAndAdjacency): u16 + u16 + i32 + i32 + i32 = 16 bytes per node/attr
        // V2 (None):             u16 + u16 + i32 + i32      = 12 bytes per node/attr
        let (node_entry_size, attr_entry_size) = match format {
            MetadataFormat::KeysAndAdjacency => (16, 16),
            MetadataFormat::None => (12, 12),
        };

        let nodes_len = nodes_out.len() * node_entry_size;
        let attrs_len = attributes_out.len() * attr_entry_size;
        let keys_len = keys.as_ref().map_or(0, std::vec::Vec::len);

        let metadata = Metadata {
            names: CompressionSizes::uncompressed(names_len, "LSF names section size")?,
            keys: CompressionSizes::uncompressed(keys_len, "LSF keys section size")?,
            nodes: CompressionSizes::uncompressed(nodes_len, "LSF nodes section size")?,
            attributes: CompressionSizes::uncompressed(attrs_len, "LSF attributes section size")?,
            values: CompressionSizes::uncompressed(values.len(), "LSF values section size")?,
            compression,
            format,
        };

        Ok(Self {
            header: Header {
                format_version: lsf.format_version,
                game_version: lsf.game_version,
            },
            metadata,
            names,
            nodes: nodes_out,
            attributes: attributes_out,
            values,
            keys,
        })
    }

    fn read_lsf(input: &mut impl Read) -> Result<Self> {
        let header = Header::read(input)?;
        let metadata = Metadata::read(input)?;

        let name_bytes = read_section!("names", metadata.names, metadata.compression, input, false);
        let names = Names::from_bytes(&name_bytes)?;

        let nodes = read_items!(NodeInfo, "nodes", metadata, metadata.nodes, input);
        let attributes =
            read_items!(AttributeInfo, "attributes", metadata, metadata.attributes, input);

        let values = read_section!("values", metadata.values, metadata.compression, input, true);
        let keys = if metadata.format == MetadataFormat::KeysAndAdjacency {
            Some(read_section!("keys", metadata.keys, metadata.compression, input, true))
        } else {
            None
        };

        Ok(Self {
            header,
            metadata,
            names,
            nodes,
            attributes,
            values,
            keys,
        })
    }
}

/// Represents the data in an LSF file.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct LsfData {
    format_version: u32,
    game_version: UnpackedVersion,
    regions: Vec<Named<Region>>,
    root_indexes: Vec<usize>,
    children_indexes: Vec<Vec<usize>>,
    use_adjacency_format: bool,
}

/// Represents a named value of type `T`.
#[derive(Debug)]
pub struct Named<T> {
    name: String,
    value: T,
    trailing_text: Option<String>,
}

#[cfg(test)]
impl<T: PartialEq> PartialEq for Named<T> {
    fn eq(&self, other: &Self) -> bool {
        // trailing_text is intentionally excluded: it is not preserved through the binary LSF
        // format, so round-trips through LSF would fail if it were included.
        self.name == other.name && self.value == other.value
    }
}

#[cfg(test)]
impl<T: Eq> Eq for Named<T> {}

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Region {
    key_attribute: Option<String>,
    attributes: Vec<Named<Attribute>>,
}

impl Region {
    const fn new(key_attribute: Option<String>) -> Self {
        Self {
            key_attribute,
            attributes: Vec::new(),
        }
    }
}

#[derive(Default)]
struct NameBuilder {
    map: HashMap<String, (u16, u16)>,
    strings: Vec<String>,
}

impl NameBuilder {
    fn insert(&mut self, s: &str) -> Result<(u16, u16)> {
        if let Some(&pair) = self.map.get(s) {
            return Ok(pair);
        }

        let inner = try_as_metadata_size!(
            u64_from_usize!(self.strings.len()) => u16,
           NumberOfEntriesTooHigh,
           "LSF names table",
        )?;

        self.strings.push(s.to_string());
        self.map.insert(s.to_string(), (0, inner));
        Ok((0, inner))
    }

    fn build(self) -> Names {
        // lslib uses 512 buckets; pad to match expected structure
        const NUM_BUCKETS: usize = 0x200;
        let mut buckets = vec![Vec::new(); NUM_BUCKETS];
        if !self.strings.is_empty() {
            buckets[0] = self.strings;
        }
        Names { inner: buckets }
    }
}

impl LsfData {
    fn get_children(&self, index: usize) -> impl Iterator<Item = (usize, &Named<Region>)> {
        self.children_indexes
            .get(index)
            .into_iter()
            .flat_map(|children_indexes| {
                children_indexes
                    .iter()
                    .copied()
                    .map(|i| (i, &self.regions[i]))
            })
    }

    /// Parses a BG3 .lsf file from the given input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be parsed.
    pub fn read_lsf(input: &mut impl Read) -> Result<Self> {
        let mut intermediate = Intermediate::read_lsf(input)?;

        if let Some(keys) = &intermediate.keys {
            update_nodes(keys, &intermediate.names, &mut intermediate.nodes)?;
        }

        let mut parser = Parser::new(intermediate);

        while parser.parse_next()? {}

        Ok(parser.finish())
    }

    /// Writes data to the given output stream as an .lsf file.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written.
    pub fn write_lsf(&self, out: &mut impl Write) -> Result<()> {
        Intermediate::from_lsf_data(self)?.write_lsf(out)
    }

    /// Parses a BG3 .lsx file from the given input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be parsed.
    pub fn read_lsx(input: &mut impl Read) -> Result<Self> {
        let mut xml = String::new();
        input
            .read_to_string(&mut xml)
            .map_err(|e| Error::read_io("LSF XML input", e))?;
        let save: model::Save<'_> = quick_xml::de::from_str(&xml)?;
        Self::from_model_save(&save)
    }

    fn from_model_save(save: &model::Save<'_>) -> Result<Self> {
        let mut lsf = Self {
            format_version: save.version.lsf_version,
            game_version: UnpackedVersion {
                major: save.version.major,
                minor: save.version.minor,
                rev: save.version.revision,
                build: save.version.build,
            },
            regions: Vec::new(),
            root_indexes: Vec::new(),
            children_indexes: Vec::new(),
            use_adjacency_format: save
                .version
                .lslib_meta
                .as_deref()
                .is_some_and(|m| m.contains("lsf_keys_adjacency")),
        };
        for region in &save.regions {
            lsf.insert_model_node(&region.node, None)?;
        }
        Ok(lsf)
    }

    fn insert_model_node(&mut self, node: &model::Node<'_>, parent: Option<u32>) -> Result<()> {
        let current_index = self.regions.len();

        let mut region = Region::new(node.key.as_ref().map(|k| k.as_ref().to_string()));
        let mut model_children = None;
        for item in &node.content {
            match item {
                model::NodeContent::Attribute(attr) => {
                    region.attributes.push(Named {
                        name: attr.id.as_ref().to_string(),
                        value: model_attribute_to_parse_attribute(attr)?,
                        trailing_text: None,
                    });
                }
                model::NodeContent::Text(text) => {
                    if let Some(last) = region.attributes.last_mut() {
                        last.trailing_text = Some(text.as_ref().to_string());
                    }
                }
                model::NodeContent::Children(children) => {
                    model_children = Some(children);
                }
            }
        }

        self.regions.push(Named {
            name: node.id.as_ref().to_string(),
            value: region,
            trailing_text: None,
        });
        self.children_indexes.push(Vec::new());

        if let Some(p) = parent {
            self.children_indexes[p as usize].push(current_index);
        } else {
            self.root_indexes.push(current_index);
        }

        if let Some(children) = model_children {
            let parent_u32 = try_as_metadata_size!(
                u64_from_usize!(current_index) => u32,
                NumberOfEntriesTooHigh,
                "LSX XML node index",
            )?;

            for child in &children.nodes {
                self.insert_model_node(child, Some(parent_u32))?;
            }
        }

        Ok(())
    }

    /// Writes data to the given output stream as an .lsx file.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written.
    pub fn write_lsx(&self, out: &mut impl Write) -> Result<()> {
        writeln!(out, r#"<?xml version="1.0" encoding="utf-8"?>"#)
            .map_err(|e| Error::write_io("XML header", e))?;
        let mut xml = String::new();
        let mut ser = quick_xml::se::Serializer::new(&mut xml);
        ser.indent(' ', 4);
        self.to_model_save().serialize(ser)?;
        writeln!(out, "{xml}").map_err(|e| Error::write_io("XML body", e))
    }

    fn to_model_save(&self) -> model::Save<'_> {
        model::Save {
            version: model::LsfVersionInfo {
                major: self.game_version.major,
                minor: self.game_version.minor,
                revision: self.game_version.rev,
                build: self.game_version.build,
                lsf_version: self.format_version,
                lslib_meta: Some(
                    if self.use_adjacency_format {
                        "v1,bswap_guids,lsf_keys_adjacency"
                    } else {
                        "v1,bswap_guids"
                    }
                    .to_string(),
                ),
            },
            regions: self
                .root_indexes
                .iter()
                .map(|&i| {
                    let Named { name, value, .. } = &self.regions[i];
                    model::Region {
                        id: Cow::Borrowed(name),
                        node: self.region_to_model_node(name, value, i),
                    }
                })
                .collect(),
        }
    }

    fn region_to_model_node<'a>(
        &'a self,
        name: &'a str,
        region: &'a Region,
        index: usize,
    ) -> model::Node<'a> {
        let children: Vec<_> = self.get_children(index).collect();
        let mut content: Vec<model::NodeContent<'a>> = region
            .attributes
            .iter()
            .flat_map(
                |Named {
                     name,
                     value,
                     trailing_text,
                 }| {
                    let attr = NodeContent::Attribute(value.to_model_attribute(name));
                    let text = trailing_text
                        .as_deref()
                        .map(|t| model::NodeContent::Text(Cow::Borrowed(t)));
                    std::iter::once(attr).chain(text)
                },
            )
            .collect();

        if !children.is_empty() {
            content.push(model::NodeContent::Children(model::Children {
                nodes: children
                    .into_iter()
                    .map(|(child_index, Named { name, value, .. })| {
                        self.region_to_model_node(name, value, child_index)
                    })
                    .collect(),
            }));
        }
        model::Node {
            id: Cow::Borrowed(name),
            key: region.key_attribute.as_deref().map(Cow::Borrowed),
            content,
        }
    }

    fn insert_attributes(
        &mut self,
        names: &Names,
        node: &NodeInfo,
        mut attributes: AttributeParser,
    ) -> Result<u32> {
        self.children_indexes.push(Vec::new());

        let current_index = self.children_indexes.len() - 1;
        if let Some(i) = node.parent {
            self.children_indexes[usize_from_u32!(i)].push(current_index);
        } else {
            self.root_indexes.push(current_index);
        }

        let mut region = Region::new(node.key_attribute.clone());
        let mut count = 0;

        while let Some(attr) = attributes.parse_next()? {
            region.attributes.push(Named {
                name: names[attr.info].into(),
                value: attr.value,
                trailing_text: None,
            });
            count += 1;
        }

        self.regions.push(Named {
            name: names[node].into(),
            value: region,
            trailing_text: None,
        });

        Ok(count)
    }
}

fn model_attribute_to_parse_attribute(attr: &model::Attribute<'_>) -> Result<Attribute> {
    let type_name = attr.type_name.as_ref();

    Ok(match type_name {
        "None" => Attribute::None,
        "int8" => attr.parse(Attribute::I8)?,
        "int16" => attr.parse(Attribute::I16)?,
        "int32" => attr.parse(Attribute::I32)?,
        "int64" => attr.parse(Attribute::I64)?,
        "old_int64" => attr.parse(Attribute::OldI64)?,
        "uint8" => attr.parse(Attribute::U8)?,
        "uint16" => attr.parse(Attribute::U16)?,
        "uint32" => attr.parse(Attribute::U32)?,
        "uint64" => attr.parse(Attribute::U64)?,
        "float" => attr.parse(Attribute::F32)?,
        "double" => attr.parse(Attribute::F64)?,
        "bool" => attr.parse_bool()?,
        "string" => Attribute::String(attr.get_value()?.into()),
        "path" => Attribute::Path(attr.get_value()?.into()),
        "FixedString" => Attribute::FixedString(attr.get_value()?.into()),
        "LSString" => Attribute::LsString(attr.get_value()?.into()),
        "LSWString" => Attribute::LsWString(attr.get_value()?.into()),
        "WString" => Attribute::WString(attr.get_value()?.into()),
        "guid" => attr.parse_guid()?,
        "ivec2" => {
            let parts: [i32; 2] = attr.parse_vec()?;
            Attribute::IVec2(parts[0], parts[1])
        }
        "ivec3" => {
            let parts: [i32; 3] = attr.parse_vec()?;
            Attribute::IVec3(parts[0], parts[1], parts[2])
        }
        "ivec4" => {
            let parts: [i32; 4] = attr.parse_vec()?;
            Attribute::IVec4(parts[0], parts[1], parts[2], parts[3])
        }
        "fvec2" => {
            let parts: [f32; 2] = attr.parse_vec()?;
            Attribute::Vec2(parts[0], parts[1])
        }
        "fvec3" => {
            let parts: [f32; 3] = attr.parse_vec()?;
            Attribute::Vec3(parts[0], parts[1], parts[2])
        }
        "fvec4" => {
            let parts: [f32; 4] = attr.parse_vec()?;
            Attribute::Vec4(parts[0], parts[1], parts[2], parts[3])
        }
        "mat2x2" => Attribute::Mat2x2(attr.parse_matrix()?),
        "mat3x3" => Attribute::Mat3x3(attr.parse_matrix()?),
        "mat3x4" => Attribute::Mat3x4(attr.parse_matrix()?),
        "mat4x3" => Attribute::Mat4x3(attr.parse_matrix()?),
        "mat4x4" => Attribute::Mat4x4(attr.parse_matrix()?),
        "ScratchBuffer" => attr.parse_scratch_buffer()?,
        "TranslatedString" => attr.parse_translated_string()?,
        "TranslatedFSString" => attr.parse_translated_fs_string()?,
        other => {
            return Err(Error::LsfXmlUnknownType {
                attribute_id: attr.id.to_string(),
                type_name: other.to_string(),
            });
        }
    })
}
