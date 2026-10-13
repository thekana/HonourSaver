use super::{
    AttributeInfo, Intermediate, LsfData, MetadataFormat, Names, NodeInfo, data_type::DataType,
    model,
};
use crate::{
    error::{Error, Result},
    raw::lsf::float::{format_f32_lslib, format_f64_lslib},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use itertools::Itertools;
use std::{
    borrow::Cow,
    io::{BufRead, Cursor, Read, Write},
    vec,
};

#[derive(Debug)]
pub(super) struct Parser {
    input: Cursor<Vec<u8>>,
    nodes: vec::IntoIter<NodeInfo>,
    attributes: Vec<AttributeInfo>,
    names: Names,
    format: MetadataFormat,
    output: LsfData,
    node_count: u32,
}

impl Parser {
    pub(super) fn new(intermediate: Intermediate) -> Self {
        Self {
            input: Cursor::new(intermediate.values),
            nodes: intermediate.nodes.into_iter(),
            attributes: intermediate.attributes,
            names: intermediate.names,
            format: intermediate.metadata.format,
            output: LsfData {
                format_version: intermediate.header.format_version,
                game_version: intermediate.header.game_version,
                regions: Vec::new(),
                root_indexes: Vec::new(),
                children_indexes: Vec::new(),
                use_adjacency_format: intermediate.metadata.format ==
                    MetadataFormat::KeysAndAdjacency,
            },
            node_count: 0,
        }
    }

    pub(super) fn parse_next(&mut self) -> Result<bool> {
        let Some(node) = self.nodes.next() else {
            return Ok(false);
        };

        let next_node_index = if self.format == MetadataFormat::KeysAndAdjacency {
            None
        } else {
            Some(self.node_count)
        };

        let parser = AttributeParser {
            next_attr_index: node.first_attr,
            input: &mut self.input,
            attributes: &self.attributes,
            next_node_index,
        };

        self.output.insert_attributes(&self.names, &node, parser)?;
        self.node_count += 1;

        Ok(true)
    }

    pub(super) fn finish(self) -> LsfData {
        self.output
    }
}

#[derive(Debug)]
pub(super) struct AttributeParser<'a> {
    attributes: &'a [AttributeInfo],
    input: &'a mut Cursor<Vec<u8>>,
    next_attr_index: Option<u32>,
    next_node_index: Option<u32>,
}

#[derive(Debug)]
pub(super) struct ParsedAttribute<'a> {
    pub(super) info: &'a AttributeInfo,
    pub(super) value: Attribute,
}

impl AttributeParser<'_> {
    pub(super) fn parse_next(&mut self) -> Result<Option<ParsedAttribute<'_>>> {
        let Some(index) = self.next_attr_index else {
            return Ok(None);
        };

        if usize_from_u32!(index) >= self.attributes.len() {
            return Ok(None);
        }

        let info = &self.attributes[usize_from_u32!(index)];

        let data_type = info.data_type;

        if let Some(offset) = info.offset {
            // Adjacency format: follow the linked list
            self.input.set_position(offset.into());
            self.next_attr_index = info.next_attr_index;
        } else {
            // Sequential (v2) format: stop when we reach a different node's attributes
            if let Some(expected) = self.next_node_index &&
                info.node_index != Some(expected)
            {
                self.next_attr_index = None;
                return Ok(None);
            }
            self.next_attr_index = Some(index + 1);
        }

        let attr = match data_type {
            DataType::None => Attribute::None,
            DataType::String => self.parse_string(info).map(Attribute::String)?,
            DataType::Path => self.parse_string(info).map(Attribute::Path)?,
            DataType::FixedString => self.parse_string(info).map(Attribute::FixedString)?,
            DataType::LsString => self.parse_string(info).map(Attribute::LsString)?,
            DataType::WString => self.parse_string(info).map(Attribute::WString)?,
            DataType::LsWString => self.parse_string(info).map(Attribute::LsWString)?,
            DataType::TranslatedString => self.parse_translated_string(info)?,
            DataType::TranslatedFsString => self.parse_translated_fs_string(info)?,
            DataType::ScratchBuffer => self.parse_scratch_buffer(info)?,
            DataType::Byte => self.parse_byte(info)?,
            DataType::Short => {
                let value = read_num!(i16, self.input, info.fmt_context("short"))?;
                Attribute::I16(value)
            }
            DataType::UShort => {
                let value = read_num!(u16, self.input, info.fmt_context("ushort"))?;
                Attribute::U16(value)
            }
            DataType::Int => {
                let value = read_num!(i32, self.input, info.fmt_context("int"))?;
                Attribute::I32(value)
            }
            DataType::UInt => {
                let value = read_num!(u32, self.input, info.fmt_context("uint"))?;
                Attribute::U32(value)
            }
            DataType::Float => {
                let value = read_num!(f32, self.input, info.fmt_context("float"))?;
                Attribute::F32(value)
            }
            DataType::Double => {
                let value = read_num!(f64, self.input, info.fmt_context("double"))?;
                Attribute::F64(value)
            }
            DataType::IVec2 => self.parse_ivec2(info)?,
            DataType::IVec3 => self.parse_ivec3(info)?,
            DataType::IVec4 => self.parse_ivec4(info)?,
            DataType::Vec2 => self.parse_vec2(info)?,
            DataType::Vec3 => self.parse_vec3(info)?,
            DataType::Vec4 => self.parse_vec4(info)?,
            DataType::Mat2 => Attribute::Mat2x2(self.make_matrix(info)?),
            DataType::Mat3 => Attribute::Mat3x3(self.make_matrix(info)?),
            DataType::Mat3x4 => Attribute::Mat3x4(self.make_matrix(info)?),
            DataType::Mat4x3 => Attribute::Mat4x3(self.make_matrix(info)?),
            DataType::Mat4 => Attribute::Mat4x4(self.make_matrix(info)?),
            DataType::Bool => self.parse_bool(info)?,
            DataType::ULongLong => {
                let value = read_num!(u64, self.input, info.fmt_context("ulonglong"))?;
                Attribute::U64(value)
            }
            DataType::Long => {
                let value = read_num!(i64, self.input, info.fmt_context("long"))?;
                Attribute::OldI64(value)
            }
            DataType::Int8 => {
                let value = read_num!(i8, self.input, info.fmt_context("int8"))?;
                Attribute::I8(value)
            }
            DataType::Uuid => self.parse_uuid(info)?,
            DataType::Int64 => {
                let value = read_num!(i64, self.input, info.fmt_context("int64"))?;
                Attribute::I64(value)
            }
        };

        Ok(Some(ParsedAttribute { info, value: attr }))
    }

    fn parse_uuid(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        let mut bytes = [0_u8; 16];
        self.input
            .read_exact(&mut bytes)
            .map_err(|e| Error::read_io(attr.fmt_context("uuid"), e))?;
        Ok(Attribute::Uuid(swap_guid_node_bytes(bytes)))
    }

    fn parse_bool(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        let mut byte = [0_u8; 1];
        self.input
            .read_exact(&mut byte)
            .map_err(|e| Error::read_io(attr.fmt_context("bool"), e))?;
        Ok(Attribute::Bool(byte[0] != 0))
    }

    fn parse_vec4(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        Ok(Attribute::Vec4(
            read_num!(f32, self.input, attr.fmt_context("vec4[0]"))?,
            read_num!(f32, self.input, attr.fmt_context("vec4[1]"))?,
            read_num!(f32, self.input, attr.fmt_context("vec4[2]"))?,
            read_num!(f32, self.input, attr.fmt_context("vec4[3]"))?,
        ))
    }

    fn parse_vec3(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        Ok(Attribute::Vec3(
            read_num!(f32, self.input, attr.fmt_context("vec3[0]"))?,
            read_num!(f32, self.input, attr.fmt_context("vec3[1]"))?,
            read_num!(f32, self.input, attr.fmt_context("vec3[2]"))?,
        ))
    }

    fn parse_vec2(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        Ok(Attribute::Vec2(
            read_num!(f32, self.input, attr.fmt_context("vec2[0]"))?,
            read_num!(f32, self.input, attr.fmt_context("vec2[1]"))?,
        ))
    }

    fn parse_ivec4(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        Ok(Attribute::IVec4(
            read_num!(i32, self.input, attr.fmt_context("ivec4[0]"))?,
            read_num!(i32, self.input, attr.fmt_context("ivec4[1]"))?,
            read_num!(i32, self.input, attr.fmt_context("ivec4[2]"))?,
            read_num!(i32, self.input, attr.fmt_context("ivec4[3]"))?,
        ))
    }

    fn parse_ivec3(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        Ok(Attribute::IVec3(
            read_num!(i32, self.input, attr.fmt_context("ivec3[0]"))?,
            read_num!(i32, self.input, attr.fmt_context("ivec3[1]"))?,
            read_num!(i32, self.input, attr.fmt_context("ivec3[2]"))?,
        ))
    }

    fn parse_ivec2(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        Ok(Attribute::IVec2(
            read_num!(i32, self.input, attr.fmt_context("ivec2[0]"))?,
            read_num!(i32, self.input, attr.fmt_context("ivec2[1]"))?,
        ))
    }

    fn parse_byte(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        let mut byte = [0_u8; 1];
        self.input
            .read_exact(&mut byte)
            .map_err(|e| Error::read_io(attr.fmt_context("byte"), e))?;
        Ok(Attribute::U8(byte[0]))
    }

    fn parse_scratch_buffer(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        let mut bytes = vec![0; usize_from_u32!(attr.length)];
        self.input
            .read_exact(&mut bytes)
            .map_err(|e| Error::read_io(attr.fmt_context("scratch buffer"), e))?;
        Ok(Attribute::ScratchBuffer(bytes))
    }

    fn parse_translated_string(&mut self, attr: &AttributeInfo) -> Result<Attribute, Error> {
        let version = read_num!(u16, self.input, attr.fmt_context("translated string version"))?;
        let length = read_num!(u32, self.input, attr.fmt_context("translated string length"))?;
        let handle = self.parse_string_with_len(attr, length)?;
        Ok(Attribute::TranslatedString(TranslatedString { version, handle }))
    }

    fn parse_string(&mut self, attr: &AttributeInfo) -> Result<String> {
        let mut bytes = Vec::new();

        self.input
            .read_until(0, &mut bytes)
            .map_err(|e| Error::read_io(attr.fmt_context(&format!("{:?}", attr.data_type)), e))?;

        if matches!(bytes.last(), Some(0)) {
            bytes.pop();
        }

        Ok(String::from_utf8_lossy(&bytes).into())
    }

    fn parse_string_with_len(&mut self, attr: &AttributeInfo, length: u32) -> Result<String> {
        let mut bytes = vec![0_u8; usize_from_u32!(length).saturating_sub(1)];

        self.input
            .read_exact(&mut bytes)
            .map_err(|e| Error::read_io(attr.fmt_context(&format!("{:?}", attr.data_type)), e))?;

        bytes.truncate(
            bytes
                .iter()
                .enumerate()
                .rfind(|(_, b)| **b != 0)
                .map_or(0, |(i, _)| i + 1),
        );

        self.input.read_exact(&mut [0_u8]).unwrap();

        Ok(String::from_utf8_lossy(&bytes).into())
    }

    fn parse_translated_fs_string(&mut self, attr: &AttributeInfo) -> Result<Attribute> {
        let str = self.parse_translated_fs_string_inner(attr)?;
        Ok(Attribute::TranslatedFsString(str))
    }

    fn parse_translated_fs_string_inner(
        &mut self,
        attr: &AttributeInfo,
    ) -> Result<TranslatedFsString> {
        let version = read_num!(u16, self.input, attr.fmt_context("translated fs string version"))?;
        let handle_length =
            read_num!(u32, self.input, attr.fmt_context("translated fs string handle length"))?;
        let handle = self.parse_string_with_len(attr, handle_length)?;

        let arguments_count =
            read_num!(i32, self.input, attr.fmt_context("translated fs string arguments count"))?;
        let mut args = Vec::with_capacity(usize::try_from(arguments_count).unwrap_or(0));

        for _ in 0..arguments_count {
            let key_length = read_num!(
                u32,
                self.input,
                attr.fmt_context("translated fs string arg key length")
            )?;
            let key = self.parse_string_with_len(attr, key_length)?;

            let inner = self.parse_translated_fs_string_inner(attr)?;

            let value_length = read_num!(
                u32,
                self.input,
                attr.fmt_context("translated fs string arg value length")
            )?;
            let value = self.parse_string_with_len(attr, value_length)?;

            args.push(TranslatedFsStringArgument {
                key,
                inner: Box::new(inner),
                value,
            });
        }

        Ok(TranslatedFsString {
            version,
            handle,
            args,
        })
    }

    fn make_matrix<const X: usize, const Y: usize>(
        &mut self,
        attr: &AttributeInfo,
    ) -> Result<[[f32; X]; Y]> {
        let mut array = [[0.0; X]; Y];

        for (i, row) in array.iter_mut().enumerate() {
            for (j, item) in row.iter_mut().enumerate() {
                *item =
                    read_num!(f32, self.input, attr.fmt_context(&format!("mat{X}x{Y}[{i}][{j}]")))?;
            }
        }

        Ok(array)
    }
}

// BG3's LSF format stores GUID node-bytes 8–15 with each consecutive pair of bytes swapped
// relative to the standard Windows Guid layout. Apply this swap on both read and write.
fn swap_guid_node_bytes(mut bytes: [u8; 16]) -> [u8; 16] {
    let len = bytes.len();

    for i in (9..).step_by(2).take_while(|&i| i < len) {
        bytes.swap(i, i - 1);
    }

    bytes
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Attribute {
    None,
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    F32(f32),
    F64(f64),
    IVec2(i32, i32),
    IVec3(i32, i32, i32),
    IVec4(i32, i32, i32, i32),
    Vec2(f32, f32),
    Vec3(f32, f32, f32),
    Vec4(f32, f32, f32, f32),
    Mat2x2([[f32; 2]; 2]),
    Mat3x3([[f32; 3]; 3]),
    Mat3x4([[f32; 3]; 4]),
    Mat4x3([[f32; 4]; 3]),
    Mat4x4([[f32; 4]; 4]),
    Bool(bool),
    String(String),
    Path(String),
    FixedString(String),
    LsString(String),
    U64(u64),
    ScratchBuffer(Vec<u8>),
    OldI64(i64),
    I8(i8),
    TranslatedString(TranslatedString),
    WString(String),
    LsWString(String),
    Uuid([u8; 16]),
    I64(i64),
    TranslatedFsString(TranslatedFsString),
}

fn write_lsf_matrix<const X: usize, const Y: usize>(
    w: &mut impl Write,
    m: &[[f32; X]; Y],
) -> Result<()> {
    for row in m {
        for val in row {
            write_num!(*val, w, "matrix element")?;
        }
    }
    Ok(())
}

impl Attribute {
    pub(super) const fn data_type(&self) -> DataType {
        match self {
            Self::None => DataType::None,
            Self::U8(_) => DataType::Byte,
            Self::I16(_) => DataType::Short,
            Self::U16(_) => DataType::UShort,
            Self::I32(_) => DataType::Int,
            Self::U32(_) => DataType::UInt,
            Self::F32(_) => DataType::Float,
            Self::F64(_) => DataType::Double,
            Self::IVec2(..) => DataType::IVec2,
            Self::IVec3(..) => DataType::IVec3,
            Self::IVec4(..) => DataType::IVec4,
            Self::Vec2(..) => DataType::Vec2,
            Self::Vec3(..) => DataType::Vec3,
            Self::Vec4(..) => DataType::Vec4,
            Self::Mat2x2(_) => DataType::Mat2,
            Self::Mat3x3(_) => DataType::Mat3,
            Self::Mat3x4(_) => DataType::Mat3x4,
            Self::Mat4x3(_) => DataType::Mat4x3,
            Self::Mat4x4(_) => DataType::Mat4,
            Self::Bool(_) => DataType::Bool,
            Self::String(_) => DataType::String,
            Self::Path(_) => DataType::Path,
            Self::FixedString(_) => DataType::FixedString,
            Self::LsString(_) => DataType::LsString,
            Self::U64(_) => DataType::ULongLong,
            Self::ScratchBuffer(_) => DataType::ScratchBuffer,
            Self::OldI64(_) => DataType::Long,
            Self::I8(_) => DataType::Int8,
            Self::TranslatedString(_) => DataType::TranslatedString,
            Self::WString(_) => DataType::WString,
            Self::LsWString(_) => DataType::LsWString,
            Self::Uuid(_) => DataType::Uuid,
            Self::I64(_) => DataType::Int64,
            Self::TranslatedFsString(_) => DataType::TranslatedFsString,
        }
    }

    pub(super) fn write_lsf_value(&self, write: &mut impl Write) -> Result<()> {
        match self {
            Self::None => {}
            Self::U8(v) => write_num!(*v, write, "uint8 attribute")?,
            Self::I16(v) => write_num!(*v, write, "int16 attribute")?,
            Self::U16(v) => write_num!(*v, write, "uint16 attribute")?,
            Self::I32(v) => write_num!(*v, write, "int32 attribute")?,
            Self::U32(v) => write_num!(*v, write, "uint32 attribute")?,
            Self::F32(v) => write_num!(*v, write, "float attribute")?,
            Self::F64(v) => write_num!(*v, write, "double attribute")?,
            Self::Bool(v) => write_num!(u8::from(*v), write, "bool attribute")?,
            Self::I8(v) => write_num!(*v, write, "int8 attribute")?,
            Self::OldI64(v) | Self::I64(v) => write_num!(*v, write, "int64 attribute")?,
            Self::U64(v) => write_num!(*v, write, "uint64 attribute")?,
            Self::Uuid(bytes) => write
                .write_all(&swap_guid_node_bytes(*bytes))
                .map_err(|e| Error::write_io("uuid attribute bytes", e))?,
            Self::String(s) |
            Self::Path(s) |
            Self::FixedString(s) |
            Self::LsString(s) |
            Self::WString(s) |
            Self::LsWString(s) => {
                write
                    .write_all(s.as_bytes())
                    .map_err(|e| Error::write_io("string attribute bytes", e))?;
                write
                    .write_all(&[0])
                    .map_err(|e| Error::write_io("string attribute null terminator", e))?;
            }
            Self::ScratchBuffer(bytes) => write
                .write_all(bytes)
                .map_err(|e| Error::write_io("ScratchBuffer attribute bytes", e))?,
            Self::TranslatedString(ts) => {
                write_num!(ts.version, write, "TranslatedString version")?;
                let len = try_as_metadata_size!(
                    u64_from_usize!(ts.handle.len() + 1) => u32,
                    StringLengthTooHigh,
                    "TranslatedString handle length",
                )?;
                write_num!(len, write, "TranslatedString handle length")?;
                write
                    .write_all(ts.handle.as_bytes())
                    .map_err(|e| Error::write_io("TranslatedString handle bytes", e))?;
                write
                    .write_all(&[0])
                    .map_err(|e| Error::write_io("TranslatedString handle null terminator", e))?;
            }
            Self::TranslatedFsString(ts) => ts.write_lsf_value(write)?,
            Self::IVec2(a, b) => {
                write_num!(*a, write, "ivec2[0]")?;
                write_num!(*b, write, "ivec2[1]")?;
            }
            Self::IVec3(a, b, c) => {
                write_num!(*a, write, "ivec3[0]")?;
                write_num!(*b, write, "ivec3[1]")?;
                write_num!(*c, write, "ivec3[2]")?;
            }
            Self::IVec4(a, b, c, d) => {
                write_num!(*a, write, "ivec4[0]")?;
                write_num!(*b, write, "ivec4[1]")?;
                write_num!(*c, write, "ivec4[2]")?;
                write_num!(*d, write, "ivec4[3]")?;
            }
            Self::Vec2(a, b) => {
                write_num!(*a, write, "fvec2[0]")?;
                write_num!(*b, write, "fvec2[1]")?;
            }
            Self::Vec3(a, b, c) => {
                write_num!(*a, write, "fvec3[0]")?;
                write_num!(*b, write, "fvec3[1]")?;
                write_num!(*c, write, "fvec3[2]")?;
            }
            Self::Vec4(a, b, c, d) => {
                write_num!(*a, write, "fvec4[0]")?;
                write_num!(*b, write, "fvec4[1]")?;
                write_num!(*c, write, "fvec4[2]")?;
                write_num!(*d, write, "fvec4[3]")?;
            }
            Self::Mat2x2(m) => write_lsf_matrix(write, m)?,
            Self::Mat3x3(m) => write_lsf_matrix(write, m)?,
            Self::Mat3x4(m) => write_lsf_matrix(write, m)?,
            Self::Mat4x3(m) => write_lsf_matrix(write, m)?,
            Self::Mat4x4(m) => write_lsf_matrix(write, m)?,
        }
        Ok(())
    }

    #[expect(clippy::too_many_lines, reason = "enum has more cases than line number lint")]
    pub(super) fn to_model_attribute<'a>(&'a self, id: &'a str) -> model::Attribute<'a> {
        match self {
            Self::None => model::Attribute::with_value(id, "None", "None".into()),
            Self::I8(v) => model::Attribute::with_value(id, "int8", v.to_string().into()),
            Self::I16(v) => model::Attribute::with_value(id, "int16", v.to_string().into()),
            Self::I32(v) => model::Attribute::with_value(id, "int32", v.to_string().into()),
            Self::I64(v) => model::Attribute::with_value(id, "int64", v.to_string().into()),
            Self::OldI64(v) => model::Attribute::with_value(id, "old_int64", v.to_string().into()),
            Self::U8(v) => model::Attribute::with_value(id, "uint8", v.to_string().into()),
            Self::U16(v) => model::Attribute::with_value(id, "uint16", v.to_string().into()),
            Self::U32(v) => model::Attribute::with_value(id, "uint32", v.to_string().into()),
            Self::U64(v) => model::Attribute::with_value(id, "uint64", v.to_string().into()),
            Self::F32(v) => model::Attribute::with_value(id, "float", format_f32_lslib(*v).into()),
            Self::F64(v) => model::Attribute::with_value(id, "double", format_f64_lslib(*v).into()),
            Self::Bool(true) => model::Attribute::with_value(id, "bool", "True".into()),
            Self::Bool(false) => model::Attribute::with_value(id, "bool", "False".into()),
            Self::String(s) => model::Attribute::with_value(id, "string", Cow::Borrowed(s)),
            Self::Path(s) => model::Attribute::with_value(id, "path", Cow::Borrowed(s)),
            Self::FixedString(s) => {
                model::Attribute::with_value(id, "FixedString", Cow::Borrowed(s))
            }
            Self::LsString(s) => model::Attribute::with_value(id, "LSString", Cow::Borrowed(s)),
            Self::LsWString(s) => model::Attribute::with_value(id, "LSWString", Cow::Borrowed(s)),
            Self::WString(s) => model::Attribute::with_value(id, "WString", Cow::Borrowed(s)),
            Self::Uuid(bytes) => model::Attribute::with_value(
                id,
                "guid",
                uuid::Uuid::from_bytes_le(*bytes).to_string().into(),
            ),
            Self::IVec2(a, b) => {
                model::Attribute::with_value(id, "ivec2", format!("{a} {b}").into())
            }
            Self::IVec3(a, b, c) => {
                model::Attribute::with_value(id, "ivec3", format!("{a} {b} {c}").into())
            }
            Self::IVec4(a, b, c, d) => {
                model::Attribute::with_value(id, "ivec4", format!("{a} {b} {c} {d}").into())
            }
            Self::Vec2(a, b) => model::Attribute::with_value(
                id,
                "fvec2",
                format!("{} {}", format_f32_lslib(*a), format_f32_lslib(*b)).into(),
            ),
            Self::Vec3(a, b, c) => model::Attribute::with_value(
                id,
                "fvec3",
                format!(
                    "{} {} {}",
                    format_f32_lslib(*a),
                    format_f32_lslib(*b),
                    format_f32_lslib(*c),
                )
                .into(),
            ),
            Self::Vec4(a, b, c, d) => model::Attribute::with_value(
                id,
                "fvec4",
                format!(
                    "{} {} {} {}",
                    format_f32_lslib(*a),
                    format_f32_lslib(*b),
                    format_f32_lslib(*c),
                    format_f32_lslib(*d),
                )
                .into(),
            ),
            Self::Mat2x2(m) => {
                model::Attribute::with_value(id, "mat2x2", matrix_value_string(m).into())
            }
            Self::Mat3x3(m) => {
                model::Attribute::with_value(id, "mat3x3", matrix_value_string(m).into())
            }
            Self::Mat3x4(m) => {
                model::Attribute::with_value(id, "mat3x4", matrix_value_string(m).into())
            }
            Self::Mat4x3(m) => {
                model::Attribute::with_value(id, "mat4x3", matrix_value_string(m).into())
            }
            Self::Mat4x4(m) => {
                model::Attribute::with_value(id, "mat4x4", matrix_value_string(m).into())
            }
            Self::ScratchBuffer(buf) => {
                model::Attribute::with_value(id, "ScratchBuffer", BASE64.encode(buf).into())
            }
            Self::TranslatedString(ts) => model::Attribute {
                id: id.into(),
                type_name: "TranslatedString".into(),
                value: None,
                handle: Some(Cow::Borrowed(&ts.handle)),
                version: Some(ts.version),
                translated_fs_string_arguments: None,
            },
            Self::TranslatedFsString(ts) => model::Attribute {
                id: id.into(),
                type_name: "TranslatedFSString".into(),
                value: None,
                handle: Some(Cow::Borrowed(&ts.handle)),
                version: Some(ts.version),
                translated_fs_string_arguments: model::TranslatedFsStringArguments::from_parse_args(
                    &ts.args,
                ),
            },
        }
    }
}

fn matrix_value_string<const X: usize, const Y: usize>(m: &[[f32; X]; Y]) -> String {
    m.iter()
        .flat_map(|row| row.iter())
        .copied()
        .map(format_f32_lslib)
        .join(" ")
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct TranslatedString {
    pub(super) version: u16,
    pub(super) handle: String,
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct TranslatedFsString {
    pub(super) version: u16,
    pub(super) handle: String,
    pub(super) args: Vec<TranslatedFsStringArgument>,
}

impl TranslatedFsString {
    fn write_lsf_value(&self, w: &mut impl Write) -> Result<()> {
        write_num!(self.version, w, "TranslatedFsString version")?;
        let handle_len = try_as_metadata_size!(
            u64_from_usize!(self.handle.len() + 1) => u32,
            StringLengthTooHigh,
            "TranslatedFsString handle length",
        )?;
        write_num!(handle_len, w, "TranslatedFsString handle length")?;
        w.write_all(self.handle.as_bytes())
            .map_err(|e| Error::write_io("TranslatedFsString handle bytes", e))?;
        w.write_all(&[0])
            .map_err(|e| Error::write_io("TranslatedFsString handle null terminator", e))?;
        self.write_lsf_arguments(w)
    }

    fn write_lsf_arguments(&self, w: &mut impl Write) -> Result<()> {
        let arg_count = try_as_metadata_size!(
            u64_from_usize!(self.args.len()) => i32,
            NumberOfEntriesTooHigh,
            "TranslatedFsString argument count",
        )?;

        write_num!(arg_count, w, "TranslatedFsString argument count")?;
        for arg in &self.args {
            arg.write_lsf(w)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct TranslatedFsStringArgument {
    pub(super) key: String,
    pub(super) inner: Box<TranslatedFsString>,
    pub(super) value: String,
}

impl TranslatedFsStringArgument {
    fn write_lsf(&self, w: &mut impl Write) -> Result<()> {
        let key_len = try_as_metadata_size!(
            u64_from_usize!(self.key.len() + 1) => u32,
            StringLengthTooHigh,
            "TranslatedFsString argument key length",
        )?;

        write_num!(key_len, w, "TranslatedFsString argument key length")?;
        w.write_all(self.key.as_bytes())
            .map_err(|e| Error::write_io("TranslatedFsString argument key bytes", e))?;
        w.write_all(&[0])
            .map_err(|e| Error::write_io("TranslatedFsString argument key null terminator", e))?;
        self.inner.write_lsf_value(w)?;
        let value_len = try_as_metadata_size!(
            u64_from_usize!(self.value.len() + 1) => u32,
            NumberOfEntriesTooHigh,
            "TranslatedFsString argument value length",
        )?;

        write_num!(value_len, w, "TranslatedFsString argument value length")?;
        w.write_all(self.value.as_bytes())
            .map_err(|e| Error::write_io("TranslatedFsString argument value bytes", e))?;
        w.write_all(&[0])
            .map_err(|e| Error::write_io("TranslatedFsString argument value null terminator", e))?;
        Ok(())
    }
}
