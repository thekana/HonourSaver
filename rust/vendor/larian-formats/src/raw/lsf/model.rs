use super::parse;
use crate::error::{Error, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt::Display, str::FromStr};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "save")]
pub(super) struct Save<'a> {
    pub(super) version: LsfVersionInfo,
    #[serde(default, rename = "region", borrow)]
    pub(super) regions: Vec<Region<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LsfVersionInfo {
    #[serde(rename = "@major")]
    pub(super) major: u8,
    #[serde(rename = "@minor")]
    pub(super) minor: u8,
    #[serde(rename = "@revision")]
    pub(super) revision: u16,
    #[serde(rename = "@build")]
    pub(super) build: u32,
    #[serde(rename = "@lsf_version", default = "default_lsf_version", skip_serializing)]
    pub(super) lsf_version: u32,
    #[serde(rename = "@lslib_meta", default, skip_serializing_if = "Option::is_none")]
    pub(super) lslib_meta: Option<String>,
}

const fn default_lsf_version() -> u32 {
    7
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Region<'a> {
    #[serde(rename = "@id", borrow)]
    pub(super) id: Cow<'a, str>,
    #[serde(borrow)]
    pub(super) node: Node<'a>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Node<'a> {
    #[serde(rename = "@id", borrow)]
    pub(super) id: Cow<'a, str>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "@key", borrow)]
    pub(super) key: Option<Cow<'a, str>>,
    // `$value` collects all child content (elements and text) into a single sequence, which
    // handles text nodes that appear between `<attribute>` elements in hand-edited files.
    #[serde(default, rename = "$value")]
    pub(super) content: Vec<NodeContent<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum NodeContent<'a> {
    #[serde(rename = "attribute", borrow)]
    Attribute(Attribute<'a>),
    #[serde(rename = "children", borrow)]
    Children(Children<'a>),
    // Text nodes between elements (e.g. inline comments); captured and re-emitted when writing.
    #[serde(rename = "$text", borrow)]
    Text(Cow<'a, str>),
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Children<'a> {
    #[serde(default, rename = "node", borrow)]
    pub(super) nodes: Vec<Node<'a>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Attribute<'a> {
    #[serde(rename = "@id", borrow)]
    pub(super) id: Cow<'a, str>,
    #[serde(rename = "@type", borrow)]
    pub(super) type_name: Cow<'a, str>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "@value", borrow)]
    pub(super) value: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "@handle", borrow)]
    pub(super) handle: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "@version")]
    pub(super) version: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "arguments", borrow)]
    pub(super) translated_fs_string_arguments: Option<TranslatedFsStringArguments<'a>>,
}

impl<'a> Attribute<'a> {
    pub(super) fn get_value(&self) -> Result<&str> {
        self.value
            .as_deref()
            .ok_or_else(|| Error::LsfXmlMissingField {
                attribute_id: self.id.to_string(),
                field: "@value",
            })
    }

    pub(super) fn parse<T>(&self, map: impl Fn(T) -> parse::Attribute) -> Result<parse::Attribute>
    where
        T: FromStr,
        T::Err: Display,
    {
        let v = self.get_value()?;

        v.parse().map(map).map_err(|e| Error::LsfXmlParse {
            attribute_id: self.id.to_string(),
            value: v.to_string(),
            expected_type: self.type_name.to_string(),
            inner: e.to_string(),
        })
    }

    pub(super) fn parse_bool(&self) -> Result<parse::Attribute> {
        let value = self.get_value()?;

        let b = value
            .to_lowercase()
            .parse()
            .map_err(|_| Error::LsfXmlParse {
                attribute_id: self.id.clone().into_owned(),
                expected_type: self.type_name.to_string(),
                value: value.into(),
                inner: "invalid boolean".into(),
            })?;

        Ok(parse::Attribute::Bool(b))
    }

    pub(super) fn parse_guid(&self) -> Result<parse::Attribute> {
        let v = self.get_value()?;
        let uuid = Uuid::parse_str(v).map_err(|e| Error::LsfXmlParse {
            attribute_id: self.id.to_string(),
            value: v.to_string(),
            inner: e.to_string(),
            expected_type: self.type_name.to_string(),
        })?;

        Ok(parse::Attribute::Uuid(uuid.to_bytes_le()))
    }

    pub(super) fn parse_vec<const N: usize, T>(&self) -> Result<[T; N]>
    where
        T: FromStr + Default + Copy,
        T::Err: Display,
    {
        let mut parts = self.get_value()?.split(' ');
        parse_split_exact(&mut parts, self.id.as_ref(), self.type_name.as_ref())
    }

    pub(super) fn parse_matrix<const X: usize, const Y: usize>(&self) -> Result<[[f32; X]; Y]> {
        let mut out = [[0.0; X]; Y];
        let mut parts = self.get_value()?.split(' ');

        for row in &mut out {
            *row = parse_split_exact(&mut parts, self.id.as_ref(), self.type_name.as_ref())?;
        }

        let remaining_count = parts.count();

        if remaining_count > 0 {
            return Err(Error::LsfXmlWrongComponentCount {
                attribute_id: self.id.to_string(),
                expected: X * Y,
                found: X * Y + remaining_count,
            });
        }

        Ok(out)
    }

    pub(super) fn parse_scratch_buffer(&self) -> Result<parse::Attribute> {
        let bytes = BASE64_STANDARD.decode(self.get_value()?).map_err(|inner| {
            Error::LsfXmlDecodeBase64 {
                attribute_id: self.id.to_string(),
                inner,
            }
        })?;

        Ok(parse::Attribute::ScratchBuffer(bytes))
    }

    pub(super) fn parse_translated_string(&self) -> Result<parse::Attribute> {
        let handle = self
            .handle
            .as_deref()
            .ok_or_else(|| Error::LsfXmlMissingField {
                attribute_id: self.id.to_string(),
                field: "@handle",
            })?;
        let version = self.version.ok_or_else(|| Error::LsfXmlMissingField {
            attribute_id: self.id.to_string(),
            field: "@version",
        })?;
        Ok(parse::Attribute::TranslatedString(parse::TranslatedString {
            version,
            handle: handle.to_string(),
        }))
    }

    pub(super) fn parse_translated_fs_string(&self) -> Result<parse::Attribute> {
        let handle = self
            .handle
            .as_deref()
            .ok_or_else(|| Error::LsfXmlMissingField {
                attribute_id: self.id.to_string(),
                field: "@handle",
            })?;
        let version = self.version.ok_or_else(|| Error::LsfXmlMissingField {
            attribute_id: self.id.to_string(),
            field: "@version",
        })?;
        let args =
            model_translated_fs_string_args_to_parse(self.translated_fs_string_arguments.as_ref())?;

        Ok(parse::Attribute::TranslatedFsString(parse::TranslatedFsString {
            version,
            handle: handle.to_string(),
            args,
        }))
    }

    pub(super) fn with_value(id: &'a str, type_name: &'a str, value: Cow<'a, str>) -> Self {
        Self {
            id: id.into(),
            type_name: type_name.into(),
            value: Some(value),
            handle: None,
            version: None,
            translated_fs_string_arguments: None,
        }
    }
}

fn parse_split_exact<'a, T, const N: usize>(
    parts: &mut impl Iterator<Item = &'a str>,
    id: &str,
    expected_type: &str,
) -> Result<[T; N]>
where
    T: FromStr + Default + Copy,
    T::Err: Display,
{
    let Some(parts) = parts.next_array::<N>() else {
        return Err(Error::LsfXmlWrongComponentCount {
            attribute_id: id.into(),
            expected: N,
            found: parts.count(),
        });
    };

    let mut out = [T::default(); N];

    for (i, part) in parts.into_iter().enumerate() {
        out[i] = part.parse().map_err(|e: T::Err| Error::LsfXmlParse {
            attribute_id: id.into(),
            expected_type: expected_type.into(),
            value: part.into(),
            inner: e.to_string(),
        })?;
    }

    Ok(out)
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TranslatedFsStringArguments<'a> {
    #[serde(default, rename = "argument", borrow)]
    pub(super) arguments: Vec<TranslatedFsStringArgument<'a>>,
}

impl<'a> TranslatedFsStringArguments<'a> {
    pub(super) fn from_parse_args(args: &'a [parse::TranslatedFsStringArgument]) -> Option<Self> {
        (!args.is_empty()).then(|| Self {
            arguments: args
                .iter()
                .map(TranslatedFsStringArgument::from_parse)
                .collect(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TranslatedFsStringArgument<'a> {
    #[serde(rename = "@key", borrow)]
    pub(super) key: Cow<'a, str>,
    #[serde(rename = "@value", borrow)]
    pub(super) value: Cow<'a, str>,
    #[serde(borrow)]
    pub(super) string: TranslatedFsString<'a>,
}

impl<'a> TranslatedFsStringArgument<'a> {
    fn from_parse(arg: &'a parse::TranslatedFsStringArgument) -> Self {
        Self {
            key: Cow::Borrowed(&arg.key),
            value: Cow::Borrowed(&arg.value),
            string: TranslatedFsString::from_parse(&arg.inner),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TranslatedFsString<'a> {
    #[serde(rename = "@handle", borrow)]
    pub(super) handle: Cow<'a, str>,
    #[serde(rename = "@version")]
    pub(super) version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none", borrow)]
    pub(super) arguments: Option<TranslatedFsStringArguments<'a>>,
}

impl<'a> TranslatedFsString<'a> {
    fn from_parse(ts: &'a parse::TranslatedFsString) -> Self {
        Self {
            handle: Cow::Borrowed(&ts.handle),
            version: ts.version,
            arguments: TranslatedFsStringArguments::from_parse_args(&ts.args),
        }
    }
}

fn model_translated_fs_string_args_to_parse(
    args: Option<&TranslatedFsStringArguments<'_>>,
) -> Result<Vec<parse::TranslatedFsStringArgument>> {
    let Some(args) = args else {
        return Ok(Vec::new());
    };
    args.arguments
        .iter()
        .map(|arg| {
            let inner = parse::TranslatedFsString {
                version: arg.string.version,
                handle: arg.string.handle.as_ref().to_string(),
                args: model_translated_fs_string_args_to_parse(arg.string.arguments.as_ref())?,
            };
            Ok(parse::TranslatedFsStringArgument {
                key: arg.key.as_ref().to_string(),
                inner: Box::new(inner),
                value: arg.value.as_ref().to_string(),
            })
        })
        .collect()
}
