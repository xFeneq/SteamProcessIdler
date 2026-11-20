use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
    pub install_dir: String,
    pub executable_relative_path: String,
    #[serde(default)]
    pub default_executable: String,
    pub available_executables: Vec<String>,
    pub icon_path: Option<String>,
    #[serde(default)]
    pub banner_path: Option<String>,
    pub client_icon_hash: Option<String>,
    pub is_running: bool,
    pub is_custom: bool,
    pub is_discord_detectable: bool,
    pub is_never_played: bool,
    pub is_vac_protected: bool,
    pub is_third_party_anti_cheat: bool,
    pub last_played_unix: u64,
    #[serde(default)]
    pub playtime_minutes: u32,
}

impl SteamGame {
    pub fn new(app_id: u32, name: String) -> Self {
        Self {
            app_id,
            name,
            install_dir: String::new(),
            executable_relative_path: String::new(),
            default_executable: String::new(),
            available_executables: Vec::new(),
            icon_path: None,
            banner_path: None,
            client_icon_hash: None,
            is_running: false,
            is_custom: false,
            is_discord_detectable: false,
            is_never_played: false,
            is_vac_protected: false,
            is_third_party_anti_cheat: false,
            last_played_unix: 0,
            playtime_minutes: 0,
        }
    }

    pub fn playtime_formatted(&self) -> String {
        if self.playtime_minutes == 0 {
            "Never played".to_string()
        } else if self.playtime_minutes < 60 {
            format!("{} min", self.playtime_minutes)
        } else {
            let hrs = (self.playtime_minutes as f64) / 60.0_f64;
            format!("{:.1} hrs", hrs)
        }
    }

    pub fn last_played_formatted(&self) -> String {
        if self.last_played_unix == 0 {
            return "Never".to_string();
        }

        let ts = self.last_played_unix;
        let secs = ts % 86400;
        let days = ts / 86400;

        let z = days as i64 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;

        format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, hours, mins)
    }
}

pub struct ActiveSession {
    pub app_id: u32,
    pub game_name: String,
    pub process_name: String,
    pub pid: u32,
    pub start_time: Instant,
    pub temp_directory: PathBuf,
    pub scheduled_stop: Option<Instant>,
    pub scheduled_duration_secs: Option<u64>,
}

impl ActiveSession {
    pub fn elapsed_formatted(&self) -> String {
        let total_secs = self.start_time.elapsed().as_secs();
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    }

    pub fn auto_stop_remaining_formatted(&self) -> Option<String> {
        let stop_time = self.scheduled_stop?;
        let now = Instant::now();
        if stop_time > now {
            let rem = stop_time.duration_since(now).as_secs();
            let mins = rem / 60;
            let secs = rem % 60;
            Some(format!("{:02}:{:02}", mins, secs))
        } else {
            Some("00:00".to_string())
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameOverrideEntry {
    #[serde(rename = "AppId")]
    pub app_id: u32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "InstallDir", default)]
    pub install_dir: String,
    #[serde(rename = "ExecutableRelativePath")]
    pub executable_relative_path: String,
    #[serde(rename = "IsCustom", default)]
    pub is_custom: bool,
}
