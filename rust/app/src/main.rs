#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use reclaimhonour_app::app::ReclaimHonourApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 480.0])
            .with_title("Reclaim Honour"),
        ..Default::default()
    };
    eframe::run_native(
        "Reclaim Honour",
        options,
        Box::new(|cc| Ok(Box::new(ReclaimHonourApp::new(cc)))),
    )
}