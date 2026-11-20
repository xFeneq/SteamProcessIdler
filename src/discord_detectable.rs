use crate::models::SteamGame;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscordExecutable {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub is_launcher: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscordSku {
    #[serde(default)]
    pub distributor: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub sku: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscordDetectableApplication {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub executables: Vec<DiscordExecutable>,
    #[serde(default)]
    pub third_party_skus: Vec<DiscordSku>,
}

pub fn get_detectable_cache_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    local_app_data.join("SteamProcessIdler").join("discord_detectable.json")
}

pub fn fetch_or_load_detectable(force_refresh: bool) -> Vec<DiscordDetectableApplication> {
    let cache_path = get_detectable_cache_path();

    if !force_refresh && cache_path.exists() {
        if let Ok(metadata) = fs::metadata(&cache_path) {
            if metadata.len() > 100 {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                        if elapsed < Duration::from_secs(24 * 3600) {
                            if let Ok(content) = fs::read_to_string(&cache_path) {
                                if let Ok(apps) = serde_json::from_str::<Vec<DiscordDetectableApplication>>(&content) {
                                    if !apps.is_empty() {
                                        return apps;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();

    match agent
        .get("https://discord.com/api/v9/applications/detectable")
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .set("Accept", "application/json")
        .call()
    {
        Ok(response) => {
            let mut reader = response.into_reader();
            let mut content = String::new();
            if reader.read_to_string(&mut content).is_ok() {
                if let Ok(apps) = serde_json::from_str::<Vec<DiscordDetectableApplication>>(&content) {
                    if !apps.is_empty() {
                        if let Some(parent) = cache_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(&cache_path, &content);
                        return apps;
                    }
                }
            }
            load_cached_fallback(&cache_path)
        }
        Err(_) => load_cached_fallback(&cache_path),
    }
}

fn load_cached_fallback(cache_path: &Path) -> Vec<DiscordDetectableApplication> {
    if cache_path.exists() {
        if let Ok(content) = fs::read_to_string(cache_path) {
            if let Ok(apps) = serde_json::from_str::<Vec<DiscordDetectableApplication>>(&content) {
                if !apps.is_empty() {
                    return apps;
                }
            }
        }
    }
    Vec::new()
}

pub fn enrich_games_with_discord_data(
    games: &mut [SteamGame],
    detectable_apps: &[DiscordDetectableApplication],
) {
    if detectable_apps.is_empty() {
        return;
    }

    for game in games.iter_mut() {
        let app_id_str = game.app_id.to_string();

        let mut all_game_exe_names = Vec::new();
        if !game.executable_relative_path.is_empty() {
            if let Some(file_name) = Path::new(&game.executable_relative_path)
                .file_name()
                .and_then(|f| f.to_str())
            {
                all_game_exe_names.push(file_name.to_lowercase());
            }
        }
        for exe in &game.available_executables {
            if let Some(file_name) = Path::new(exe).file_name().and_then(|f| f.to_str()) {
                let lower = file_name.to_lowercase();
                if !all_game_exe_names.contains(&lower) {
                    all_game_exe_names.push(lower);
                }
            }
        }

        let matched_app = detectable_apps.iter().find(|app| {
            let matches_sku = app.third_party_skus.iter().any(|sku| {
                let is_steam = sku
                    .distributor
                    .as_deref()
                    .map(|d| d.eq_ignore_ascii_case("steam"))
                    .unwrap_or(false);
                let sku_matches = sku
                    .sku
                    .as_deref()
                    .or(sku.id.as_deref())
                    .map(|id| id == app_id_str)
                    .unwrap_or(false);
                is_steam && sku_matches
            });

            if matches_sku {
                return true;
            }

            if all_game_exe_names.is_empty() {
                return false;
            }

            app.executables.iter().any(|exe| {
                let os_match = exe.os.is_empty() || exe.os.eq_ignore_ascii_case("win32");
                if !os_match {
                    return false;
                }
                let exe_name = Path::new(&exe.name)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(&exe.name)
                    .to_lowercase();
                all_game_exe_names.iter().any(|g| g == &exe_name)
            })
        });

        if let Some(app) = matched_app {
            game.is_discord_detectable = true;

            let preferred_exe = app
                .executables
                .iter()
                .filter(|exe| {
                    (exe.os.is_empty() || exe.os.eq_ignore_ascii_case("win32"))
                        && !exe.is_launcher.unwrap_or(false)
                })
                .find_map(|exe| {
                    let exe_name = Path::new(&exe.name)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(&exe.name)
                        .to_lowercase();
                    game.available_executables.iter().find(|game_exe| {
                        Path::new(game_exe)
                            .file_name()
                            .and_then(|f| f.to_str())
                            .map(|f| f.to_lowercase() == exe_name)
                            .unwrap_or(false)
                    })
                });

            if let Some(matching) = preferred_exe {
                game.executable_relative_path = matching.clone();
            }
        }
    }
}
