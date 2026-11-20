use crate::models::{GameOverrideEntry, SteamGame};
use std::fs;
use std::path::PathBuf;

pub fn get_overrides_file_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    local_app_data.join("SteamProcessIdler").join("custom_games.json")
}

pub fn load_overrides() -> Vec<GameOverrideEntry> {
    let path = get_overrides_file_path();
    if !path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_overrides(entries: &[GameOverrideEntry]) {
    let path = get_overrides_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(entries) {
        let _ = fs::write(path, json);
    }
}

pub fn save_or_update_override(game: &SteamGame) {
    let mut entries = load_overrides();
    entries.retain(|e| e.app_id != game.app_id);

    entries.push(GameOverrideEntry {
        app_id: game.app_id,
        name: game.name.clone(),
        install_dir: game.install_dir.clone(),
        executable_relative_path: game.executable_relative_path.clone(),
        is_custom: game.is_custom,
    });

    save_overrides(&entries);
}

pub fn remove_override(app_id: u32) {
    let mut entries = load_overrides();
    let initial_len = entries.len();
    entries.retain(|e| e.app_id != app_id);
    if entries.len() != initial_len {
        save_overrides(&entries);
    }
}

pub fn apply_overrides(games: &mut Vec<SteamGame>, custom_games: &mut Vec<SteamGame>) {
    let overrides = load_overrides();
    if overrides.is_empty() {
        return;
    }

    for entry in overrides {
        if entry.is_custom {
            if let Some(existing) = custom_games.iter_mut().find(|g| g.app_id == entry.app_id) {
                existing.name = entry.name;
                existing.install_dir = entry.install_dir;
                existing.executable_relative_path = entry.executable_relative_path.clone();
                if !existing.available_executables.iter().any(|e| e.eq_ignore_ascii_case(&entry.executable_relative_path)) {
                    existing.available_executables.insert(0, entry.executable_relative_path);
                }
            } else {
                let mut new_game = SteamGame::new(entry.app_id, entry.name);
                new_game.install_dir = entry.install_dir;
                new_game.executable_relative_path = entry.executable_relative_path.clone();
                new_game.available_executables = vec![entry.executable_relative_path];
                new_game.is_custom = true;
                custom_games.push(new_game);
            }
        } else if let Some(existing) = games.iter_mut().find(|g| g.app_id == entry.app_id) {
            if !entry.install_dir.is_empty() {
                existing.install_dir = entry.install_dir;
            }
            if !entry.executable_relative_path.is_empty() {
                existing.executable_relative_path = entry.executable_relative_path.clone();
                if !existing.available_executables.iter().any(|e| e.eq_ignore_ascii_case(&entry.executable_relative_path)) {
                    existing.available_executables.insert(0, entry.executable_relative_path);
                }
            }
        }
    }
}
