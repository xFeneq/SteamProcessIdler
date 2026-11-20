#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use SteamProcessIdler::stub_runner;
use SteamProcessIdler::ui::SteamProcessIdlerApp;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if let Some(pos) = args.iter().position(|a| a == "--stub") {
        if let Some(app_id_str) = args.get(pos + 1) {
            if let Ok(app_id) = app_id_str.parse::<u32>() {
                let name = if let Some(name_pos) = args.iter().position(|a| a == "--name") {
                    args.get(name_pos + 1).cloned()
                } else {
                    None
                };

                stub_runner::run(app_id, name);
                return Ok(());
            }
        }
    }

    let icon_bytes = include_bytes!("../assets/icon_256.png");
    let icon_data = if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        Some(Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }))
    } else {
        None
    };

    let title = format!("SteamProcessIdler v{}", env!("CARGO_PKG_VERSION"));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([1120.0, 720.0])
        .with_min_inner_size([880.0, 520.0]);

    if let Some(icon) = icon_data {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "SteamProcessIdler",
        native_options,
        Box::new(|cc| Ok(Box::new(SteamProcessIdlerApp::new(cc)))),
    )
}
