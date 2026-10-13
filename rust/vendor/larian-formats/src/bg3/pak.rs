use crate::error::{Error, Result};
use quick_xml::XmlVersion;
use std::io::BufRead;
use uuid::Uuid;

macro_rules! impl_from_lsx {
    (
        $(
            $(#[$attr:meta])*
            $type_name:ident {
            $( $xml_id:literal => $field:ident : $field_ty:ty ),+ $(,)?
        } )+
    ) => {
        $(
            $(#[$attr])*
            #[derive(Debug, PartialEq, Eq, Clone)]
            pub struct $type_name {
                $( pub $field: $field_ty ),+
            }

            impl $type_name {
                pub fn from_lsx_node<R: std::io::BufRead>(
                    reader: &mut quick_xml::Reader<R>,
                    start: quick_xml::events::BytesStart<'_>,
                ) -> Option<Self> {
                    use quick_xml::events::Event;

                    let mut out = Self {
                        $( $field: Default::default(), )*
                    };

                    let mut buf = Vec::new();

                    loop {
                        match reader.read_event_into(&mut buf).ok()? {
                            Event::Empty(e) if e.name().as_ref() == b"attribute" => {
                                let mut id = None;
                                let mut val = None;

                                for a in e.attributes() {
                                    let a = a.ok()?;
                                    if a.key.as_ref() == b"id" {
                                        id = Some(a.normalized_value(XmlVersion::Explicit1_0).ok()?.to_string());
                                    } else if a.key.as_ref() == b"value" {
                                        val = Some(a.normalized_value(XmlVersion::Explicit1_0).ok()?.to_string());
                                    }
                                }

                                if let (Some(id), Some(val)) = (id.as_deref(), val.as_deref()) {
                                    #[allow(clippy::collapsible_match, reason = "macro allows variable number of arms")]
                                    match id {
                                        $(
                                            $xml_id => {
                                                out.$field = val.trim().parse::<$field_ty>().unwrap_or_default();
                                            }
                                        )*
                                        _ => {}
                                    }
                                }
                            }

                            Event::End(e) if e.name().as_ref() == start.name().as_ref() => {
                                break;
                            }

                            Event::Eof => break,

                            _ => {}
                        }

                        buf.clear();
                    }

                   Some(out)
                }
            }
        )+
    };
}

impl_from_lsx! {
    /// Contains the metadata required by `modsettings.lsx` for a mod.
    ModuleShortDesc {
        "Folder" => folder: String,
        "MD5" => md5: String,
        "Name" => name: String,
        "UUID" => uuid: Uuid,
        "Version64" => version: i64,
    }

    /// Contains the metadata parsed from `meta.lsx` in a BG3 PAK file.
    ModuleInfo {
        "Author" => author: String,
        "CharacterCreationLevelName" => character_creation_level_name: String,
        "Description" => description: String,
        "Folder" => folder: String,
        "GMTemplate" => gm_template: String,
        "LobbyLevelName" => lobby_level_name: String,
        "MD5" => md5: String,
        "MainMenuBackgroundVideo" => main_menu_background_video: String,
        "MenuLevelName" => menu_level_name: String,
        "Name" => name: String,
        "NumPlayers" => num_players: u8,
        "PhotoBooth" => photo_booth: String,
        "PublishHandle" => publish_handle: u64,
        "StartupLevelName" => startup_level_name: String,
        "Tags" => tags: String,
        "Type" => module_type: String,
        "UUID" => uuid: Uuid,
        "Version64" => version: i64,
    }
}

impl From<ModuleInfo> for ModuleShortDesc {
    fn from(mod_info: ModuleInfo) -> Self {
        Self {
            folder: mod_info.folder,
            md5: mod_info.md5,
            name: mod_info.name,
            uuid: mod_info.uuid,
            version: mod_info.version,
        }
    }
}

#[derive(Debug)]
pub struct Info {
    pub dependencies: Vec<ModuleShortDesc>,
    pub module_info: ModuleInfo,
}

impl Info {
    pub(crate) fn parse(reader: impl BufRead) -> Result<Self> {
        use quick_xml::events::Event;

        let mut reader = quick_xml::Reader::from_reader(reader);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();

        let mut dependencies = Vec::new();
        let mut module_info: Option<ModuleInfo> = None;

        loop {
            match reader
                .read_event_into(&mut buf)
                .map_err(|e| Error::InvalidMetadataFile {
                    context: e.to_string(),
                })? {
                Event::Start(e) if e.name().as_ref() == b"node" => {
                    // Extract node id attr
                    let mut id_val = None;
                    for a in e.attributes() {
                        let a = a.map_err(|e| Error::InvalidMetadataFile {
                            context: e.to_string(),
                        })?;
                        if a.key.as_ref() == b"id" {
                            id_val = Some(
                                a.normalized_value(XmlVersion::Explicit1_0)
                                    .map_err(|e| Error::InvalidMetadataFile {
                                        context: e.to_string(),
                                    })?
                                    .to_string(),
                            );
                            break;
                        }
                    }
                    let Some(id) = id_val else { continue };

                    match id.as_str() {
                        "ModuleShortDesc" => {
                            let parsed = ModuleShortDesc::from_lsx_node(&mut reader, e.clone())
                                .ok_or_else(|| Error::InvalidMetadataFile {
                                    context: "failed to parse ModuleShortDesc node".to_string(),
                                })?;
                            dependencies.push(parsed);
                        }
                        "ModuleInfo" => {
                            let parsed = ModuleInfo::from_lsx_node(&mut reader, e.clone())
                                .ok_or_else(|| Error::InvalidMetadataFile {
                                    context: "failed to parse ModuleInfo node".to_string(),
                                })?;
                            module_info = Some(parsed);
                        }
                        _ => {}
                    }
                }

                Event::Eof => break,
                _ => {}
            }

            buf.clear();
        }

        Ok(Self {
            dependencies,
            module_info: module_info.ok_or(Error::MissingModuleInfo)?,
        })
    }
}
