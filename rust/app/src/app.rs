use crate::preview;
use crate::profile;
use egui::{TextureHandle, TextureOptions};
use reclaimhonour_core::lsf_edit;
use std::path::Path;

#[derive(Clone)]
struct SaveEntry {
    guid: String,
    has_save: bool,
    has_thumbnail: bool,
}

pub struct ReclaimHonourApp {
    ctx: egui::Context,
    path: String,
    saves: Vec<SaveEntry>,
    selected: Option<usize>,
    preview: Option<TextureHandle>,
    status: Option<(String, String)>,
}

impl ReclaimHonourApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            ctx: cc.egui_ctx.clone(),
            path: profile::default_profile_dir(),
            saves: Vec::new(),
            selected: None,
            preview: None,
            status: None,
        }
    }

    fn expanded_dir(&self) -> String {
        profile::expand_env(&self.path)
    }

    fn profile_lsf_path(&self) -> String {
        format!("{}profile8.lsf", self.expanded_dir())
    }

    fn status(&mut self, title: &str, text: String) {
        self.status = Some((title.to_string(), text));
    }

    fn load_saves(&mut self) {
        let file_path = self.profile_lsf_path();
        if !std::path::Path::new(&file_path).exists() {
            self.status("Incorrect path", format!("profile8.lsf not found at: {file_path}"));
            return;
        }

        let bytes = match std::fs::read(&file_path) {
            Ok(b) => b,
            Err(e) => {
                self.status("Error on converting", e.to_string());
                return;
            }
        };

        let guids = match lsf_edit::list_guids(&bytes) {
            Ok(g) => g,
            Err(e) => {
                self.status("Error on converting", e.to_string());
                return;
            }
        };

        let story = format!("{}Savegames\\Story", self.expanded_dir());
        let story_path = Path::new(&story);

        let mut saves = Vec::with_capacity(guids.len());
        for guid in &guids {
            let dir = preview::find_save_dir(story_path, guid);
            let has_save = dir.is_some();
            let has_thumbnail = dir.as_ref().and_then(|d| preview::first_webp(d)).is_some();
            saves.push(SaveEntry {
                guid: guid.clone(),
                has_save,
                has_thumbnail,
            });
        }

        self.saves = saves;
        self.selected = self.saves.first().map(|_| 0);
        self.show_preview();
        self.status(
            "Saves loaded",
            format!("Found {} disabled Honour Mode session(s).", self.saves.len()),
        );
    }

    fn show_preview(&mut self) {
        self.preview = None;
        let Some(idx) = self.selected else { return };
        let Some(save) = self.saves.get(idx) else { return };

        if !save.has_save {
            self.preview = None;
            return;
        }

        let story = format!("{}Savegames\\Story", self.expanded_dir());
        let Some(dir) = preview::find_save_dir(std::path::Path::new(&story), &save.guid) else {
            return;
        };
        let Some(webp) = preview::first_webp(&dir) else {
            return;
        };

        match preview::decode_to_color_image(&webp) {
            Ok(img) => {
                let handle = self
                    .ctx
                    .load_texture("preview", img, TextureOptions::LINEAR);
                self.preview = Some(handle);
            }
            Err(_) => {
                self.preview = None;
            }
        }
    }

    fn reactivate(&mut self) {
        let Some(idx) = self.selected else {
            self.status("Save not selected", "Choose a save from the list first.".into());
            return;
        };
        let Some(save) = self.saves.get(idx).cloned() else {
            return;
        };
        let guid = save.guid.clone();

        let file_path = self.profile_lsf_path();

        // Fresh read from disk so the edit always matches the current file
        // (mirrors the original tool's re-read-before-overwrite behavior).
        let bytes = match std::fs::read(&file_path) {
            Ok(b) => b,
            Err(e) => {
                self.status("Error on processing", e.to_string());
                return;
            }
        };

        let out = match lsf_edit::reactivate(&bytes, &guid) {
            Ok(o) => o,
            Err(e) => {
                self.status("Error on processing", e.to_string());
                return;
            }
        };

        // Safety net: back up the current file before we overwrite it.
        let backup_path = format!("{file_path}.backup");
        if let Err(e) = std::fs::copy(&file_path, &backup_path) {
            self.status("Error on backup", e.to_string());
            return;
        }

        if let Err(e) = std::fs::write(&file_path, &out) {
            self.status("Error on writing", e.to_string());
            return;
        }

        // Re-list: the just-edited GUID is gone.
        self.saves.retain(|s| s.guid != guid);
        self.selected = self.saves.first().map(|_| 0);
        self.show_preview();
        self.status("Honour Mode reactivated", format!("Removed save {}.", guid));
    }
}

impl eframe::App for ReclaimHonourApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Reclaim Honour");

            ui.horizontal(|ui| {
                ui.label("BG3 Appdata Path:");
                ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(f32::INFINITY));
            });

            ui.add_space(4.0);

            if ui.button("Load Saves").clicked() {
                self.load_saves();
            }

            ui.add_space(4.0);

            let selection_before = self.selected;
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Save to reactivate:");
                    egui::ComboBox::from_id_salt("guid_combo")
                        .selected_text(self.selected_text())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (i, save) in self.saves.iter().enumerate() {
                                let label = if save.has_save {
                                    format!("✓ {}…", save.guid.chars().take(8).collect::<String>())
                                } else {
                                    format!("✗ {} (orphaned - no save files)", save.guid.chars().take(8).collect::<String>())
                                };
                                ui.selectable_value(&mut self.selected, Some(i), &label)
                                    .on_hover_text(&save.guid);
                            }
                        });
                });

                if let Some(idx) = self.selected {
                    if let Some(save) = self.saves.get(idx) {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(2.0);
                        ui.label(&save.guid);
                        ui.add_space(2.0);
                        if save.has_save {
                            ui.colored_label(egui::Color32::from_rgb(100, 200, 100), "✓ Save files exist");
                        } else {
                            ui.colored_label(egui::Color32::from_rgb(200, 100, 100), "✗ No save files found (orphaned entry)");
                            ui.small("Reactivating won't restore deleted saves, it'll just remove this stale entry.");
                        }
                        if save.has_thumbnail {
                            ui.colored_label(egui::Color32::from_rgb(150, 150, 200), "✓ Thumbnail available");
                        }
                    }
                }
            });

            if self.selected != selection_before {
                self.show_preview();
            }

            ui.add_space(8.0);
            if ui.button("Reactivate Honour Mode").clicked() {
                self.reactivate();
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label("Thumbnail preview:");
            if let Some(tex) = &self.preview {
                ui.add(
                    egui::Image::from_texture(tex)
                        .max_width(ui.available_width())
                        .max_height(300.0),
                );
            } else if let Some(idx) = self.selected {
                if let Some(save) = self.saves.get(idx) {
                    if save.has_save {
                        ui.small("No thumbnail WebP found for this save.");
                    } else {
                        ui.small("No preview - save files don't exist.");
                    }
                }
            }
        });

        if self.status.is_some() {
            let response =
                egui::Modal::new(egui::Id::new("status_modal")).show(ui.ctx(), |ui| {
                    ui.set_min_width(360.0);
                    if let Some((title, text)) = &self.status {
                        ui.strong(title);
                        ui.add_space(4.0);
                        ui.label(text);
                    }
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.status = None;
                    }
                });
            if response.should_close() {
                self.status = None;
            }
        }
    }
}

impl ReclaimHonourApp {
    fn selected_text(&self) -> String {
        self.selected
            .and_then(|i| self.saves.get(i))
            .map(|s| {
                let short = s.guid.chars().take(8).collect::<String>();
                if s.has_save {
                    format!("✓ {short}")
                } else {
                    format!("✗ {short} (orphaned)")
                }
            })
            .unwrap_or_else(|| "Select a save…".to_string())
    }
}