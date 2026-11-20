use serde::Deserialize;
use std::fs::File;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

pub struct UpdateInfo {
    pub latest_version: String,
    pub changelog: String,
    pub download_url: String,
}

fn parse_version(v: &str) -> Vec<u32> {
    v.trim_start_matches('v')
        .trim()
        .split(['.', '-', '+'])
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let lat = parse_version(latest);
    let cur = parse_version(current);
    let max_len = lat.len().max(cur.len());
    for i in 0..max_len {
        let l = lat.get(i).copied().unwrap_or(0);
        let c = cur.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }
    false
}

pub fn check_for_updates() -> Option<UpdateInfo> {
    let current_version = env!("CARGO_PKG_VERSION");
    let response = ureq::get("https://api.github.com/repos/xFeneq/SteamProcessIdler/releases/latest")
        .set("User-Agent", "SteamProcessIdler")
        .call()
        .ok()?;

    let release: GitHubRelease = response.into_json().ok()?;
    let tag = release.tag_name.trim_start_matches('v').trim();

    if !is_newer_version(tag, current_version) {
        return None;
    }

    let download_url = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".exe"))
        .map(|a| a.browser_download_url.clone())?;

    Some(UpdateInfo {
        latest_version: tag.to_string(),
        changelog: release.body.unwrap_or_default(),
        download_url,
    })
}

pub fn install_update(download_url: &str) -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    let temp_exe = temp_dir.join("SteamProcessIdler_update.exe");

    let response = ureq::get(download_url)
        .set("User-Agent", "SteamProcessIdler")
        .call()
        .map_err(|e| format!("Failed to download update: {}", e))?;

    let mut reader = response.into_reader();
    let mut file = File::create(&temp_exe).map_err(|e| format!("Failed to create temp file: {}", e))?;
    std::io::copy(&mut reader, &mut file).map_err(|e| format!("Failed to write update binary: {}", e))?;
    drop(file);

    let current_exe = std::env::current_exe().map_err(|e| format!("Failed to get current exe path: {}", e))?;

    launch_updater_script(&temp_exe, &current_exe)?;
    std::process::exit(0);
}

fn launch_updater_script(temp_exe: &PathBuf, current_exe: &PathBuf) -> Result<(), String> {
    let temp_str = temp_exe.to_string_lossy();
    let current_str = current_exe.to_string_lossy();

    let cmd_line = format!(
        "timeout /t 2 /nobreak >nul & move /Y \"{}\" \"{}\" & start \"\" \"{}\"",
        temp_str, current_str, current_str
    );

    Command::new("cmd.exe")
        .args(["/C", &cmd_line])
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("Failed to spawn updater command: {}", e))?;

    Ok(())
}
