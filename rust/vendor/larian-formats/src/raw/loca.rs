use crate::error::{Error, Result};
use serde::Deserialize;
use std::io::{BufReader, Read, Seek, Write};

/// Bytes for `LOCA`
static SIGNATURE: [u8; 4] = [0x4c, 0x4f, 0x43, 0x41];

#[derive(Debug)]
pub(super) struct Info {
    key: [u8; 64],
    version: u16,
    length: u32,
}

impl Info {
    pub(super) fn read(input: &mut impl Read) -> Result<Self> {
        let mut key = [0; 64];
        input
            .read_exact(&mut key)
            .map_err(|e| Error::read_io("reading .loca entry key", e))?;

        let version = read_num!(u16, input, "reading .loca entry version")?;
        let length = read_num!(u32, input, "reading .local entry length")?;

        Ok(Self {
            key,
            version,
            length,
        })
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone, PartialEq, Eq))]
struct TextEntry {
    key: String,
    version: u16,
    contents: String,
}
impl TextEntry {
    fn write_xml(&self, output: &mut impl Write) -> std::io::Result<()> {
        output.write_all(b"\t<content contentuid=\"")?;
        output.write_all(self.key.as_bytes())?;
        output.write_all(b"\" version=\"")?;
        output.write_fmt(format_args!("{}", self.version))?;
        output.write_all(b"\">")?;
        output.write_all(quick_xml::escape::partial_escape(&self.contents).as_bytes())?;
        output.write_all(b"</content>\n")?;

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "contentList")]
struct LocaXml {
    #[serde(default, rename = "content")]
    content: Vec<LocaXmlEntry>,
}

#[derive(Debug, Deserialize)]
struct LocaXmlEntry {
    #[serde(rename = "@contentuid")]
    contentuid: String,
    #[serde(rename = "@version")]
    version: u16,
    #[serde(default, rename = "$text")]
    text: String,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone, PartialEq, Eq))]
pub struct LocalizedText {
    entries: Vec<TextEntry>,
}

impl LocalizedText {
    /// Writes the contents in the .loca file format.
    ///
    /// # Errors
    ///
    /// Returns an error if the contents could not be written or were invalid.
    pub fn write_loca(self, output: &mut (impl Write + Seek)) -> Result<()> {
        let start = output
            .stream_position()
            .map_err(|e| Error::write_io("getting .loca stream position", e))?;

        output
            .write_all(&SIGNATURE)
            .map_err(|e| Error::write_io("writing .loca signature bytes", e))?;

        let num_entries =
            u32::try_from(self.entries.len()).map_err(|_| Error::NumberOfEntriesTooHigh {
                context: ".loca file".into(),
                value: u64_from_usize!(self.entries.len()),
                bit_width: 32,
            })?;

        output
            .write_all(&num_entries.to_le_bytes())
            .map_err(|e| Error::write_io("writing .loca num entries", e))?;

        // Skip past 4 bytes for offset u32
        output
            .seek_relative(4)
            .map_err(|e| Error::write_io("seeking past .loca offset", e))?;

        for (i, entry) in self.entries.iter().enumerate() {
            let key = entry.key.as_bytes();

            output
                .write_all(key)
                .map_err(|e| Error::write_io(format!("writing .loca entry {i} key"), e))?;

            let num_extra_zeroes = 64_usize.saturating_sub(key.len());
            std::io::copy(&mut std::io::repeat(0).take(u64_from_usize!(num_extra_zeroes)), output)
                .map_err(|e| Error::write_io(format!("writing .loca entry {i} key padding"), e))?;

            output
                .write(&entry.version.to_le_bytes())
                .map_err(|e| Error::write_io(format!("writing .loca entry {i} version"), e))?;

            let num_bytes = entry.contents.len() + 1;

            let num_bytes =
                u32::try_from(num_bytes).map_err(|_| Error::NumberOfEntriesTooHigh {
                    context: format!(".loca entry {i}"),
                    value: num_bytes as u64,
                    bit_width: 64,
                })?;

            output
                .write(&num_bytes.to_le_bytes())
                .map_err(|e| Error::write_io(format!("writing .loca entry {i} version"), e))?;
        }

        let offset = output
            .stream_position()
            .map_err(|e| Error::write_io("recording .loca offset", e))?;

        let offset = u32::try_from(offset).map_err(|_| Error::InvalidOffset {
            offset_found: offset,
        })?;

        for (i, entry) in self.entries.into_iter().enumerate() {
            output
                .write_all(entry.contents.as_bytes())
                .map_err(|e| Error::write_io(format!("writing .loca entry {i} contents"), e))?;

            output
                .write(&[0])
                .map_err(|e| Error::write_io("writing .loca entry {i} terminator byte", e))?;
        }

        output
            .seek(std::io::SeekFrom::Start(start + 8))
            .map_err(|e| Error::write_io("seeing .loca offset", e))?;

        output
            .write_all(&offset.to_le_bytes())
            .map_err(|e| Error::write_io("writing .loca offset", e))?;

        Ok(())
    }

    /// Reads localized text info from the .loca file format.
    ///
    /// # Errors
    ///
    /// Returns an error if the contents could not be read or were invalid.
    pub fn read_loca(input: &mut (impl Read + Seek)) -> Result<Self> {
        let mut signature_bytes = [0; 4];
        input
            .read_exact(&mut signature_bytes)
            .map_err(|e| Error::read_io("reading .loca signature", e))?;

        if signature_bytes != SIGNATURE {
            return Err(Error::InvalidIdentifier {
                expected_bytes: SIGNATURE,
                bytes_found: signature_bytes,
            });
        }

        let num_entries = read_num!(u32, input, ".loca num entries")?;

        let text_offset = read_num!(u32, input, ".loca text offset")?;

        let entry_info: Result<Vec<_>> = (0..num_entries).map(|_| Info::read(input)).collect();

        input
            .seek(std::io::SeekFrom::Start(text_offset.into()))
            .map_err(|e| Error::read_io("seeking .loca text offset", e))?;

        let entries: Result<_> = entry_info?
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let mut text_bytes = vec![0; usize_from_u32!(entry.length.saturating_sub(1))];
                input
                    .read_exact(&mut text_bytes)
                    .map_err(|e| Error::read_io(format!("reading .loca entry {i} contents"), e))?;

                input.read_exact(&mut [0]).map_err(|e| {
                    Error::read_io(format!("reading .loca entry {i} contents terminator byte"), e)
                })?;

                let key = String::from_utf8_lossy(&entry.key)
                    .chars()
                    .take_while(|c| *c != '\0')
                    .collect();

                Ok(TextEntry {
                    key,
                    version: entry.version,
                    contents: String::from_utf8_lossy(&text_bytes).into_owned(),
                })
            })
            .collect();

        entries.map(|entries| Self { entries })
    }

    /// Reads localized text info from .xml contents.
    ///
    /// # Errors
    ///
    /// Returns an error if the contents could not be read or were invalid.
    pub fn read_xml(input: &mut impl Read) -> Result<Self> {
        let xml: LocaXml = quick_xml::de::from_reader(BufReader::new(input))?;

        Ok(Self {
            entries: xml
                .content
                .into_iter()
                .map(|entry| TextEntry {
                    key: entry.contentuid,
                    version: entry.version,
                    contents: entry.text,
                })
                .collect(),
        })
    }

    /// Writes the contents in .xml format.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    pub fn write_xml(&self, output: &mut impl Write) -> Result<()> {
        output
            .write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<contentList>\n")
            .map_err(|e| Error::write_io("writing localization xml opening tags", e))?;

        for entry in &self.entries {
            entry.write_xml(output).map_err(|e| {
                Error::write_io(format!("wrting localization entry with key {}", entry.key), e)
            })?;
        }

        output
            .write_all(b"</contentList>")
            .map_err(|e| Error::write_io("writing localization xml closing tag", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LocalizedText;
    // use pretty_assertions::assert_eq;
    use std::{fs::File, io::Cursor, path::PathBuf};

    #[test]
    fn loca_roundtrip() {
        let mut file = File::open(PathBuf::from_iter([
            env!("CARGO_MANIFEST_DIR"),
            "src",
            "loca",
            "test-files",
            "mcm_english.loca",
        ]))
        .unwrap();

        let resource1 = LocalizedText::read_loca(&mut file).unwrap();

        let mut bytes = Cursor::new(Vec::new());
        resource1.clone().write_loca(&mut bytes).unwrap();

        bytes.set_position(0);

        let resource2 = LocalizedText::read_loca(&mut bytes).unwrap();

        assert_eq!(resource1, resource2);
    }

    #[test]
    fn read_both() {
        let mut file = File::open(PathBuf::from_iter([
            env!("CARGO_MANIFEST_DIR"),
            "src",
            "loca",
            "test-files",
            "mcm_english.loca",
        ]))
        .unwrap();

        let loca_resource = LocalizedText::read_loca(&mut file).unwrap();

        let mut file = File::open(PathBuf::from_iter([
            env!("CARGO_MANIFEST_DIR"),
            "src",
            "loca",
            "test-files",
            "mcm_english.xml",
        ]))
        .unwrap();

        let xml_resource = LocalizedText::read_xml(&mut file).unwrap();

        assert_eq!(loca_resource, xml_resource);
    }

    #[test]
    fn xml_write() {
        let mut file = File::open(PathBuf::from_iter([
            env!("CARGO_MANIFEST_DIR"),
            "src",
            "loca",
            "test-files",
            "mcm_english.loca",
        ]))
        .unwrap();

        let resource = LocalizedText::read_loca(&mut file).unwrap();

        let mut bytes = Cursor::new(Vec::new());
        resource.write_xml(&mut bytes).unwrap();

        let actual = String::from_utf8_lossy(bytes.get_ref());
        let expected = include_str!("../loca/test-files/mcm_english.xml");

        assert_eq!(actual, expected);
    }
}
