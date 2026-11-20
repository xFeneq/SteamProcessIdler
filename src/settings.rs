use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortOrder {
    #[default]
    NameAsc,
    LeastPlayed,
    MostPlayed,
    DiscordFirst,
    FavoritesFirst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeUnit {
    Ms,
    Sec,
    #[default]
    Min,
    Hour,
}

impl TimeUnit {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ms => "ms",
            Self::Sec => "sec",
            Self::Min => "min",
            Self::Hour => "hour",
        }
    }

    pub fn to_millis(&self, val: u64) -> u64 {
        match self {
            Self::Ms => val,
            Self::Sec => val.saturating_mul(1_000),
            Self::Min => val.saturating_mul(60_000),
            Self::Hour => val.saturating_mul(3_600_000),
        }
    }

    pub fn min_allowed(&self) -> u64 {
        match self {
            Self::Ms => 100,
            Self::Sec => 1,
            Self::Min => 1,
            Self::Hour => 1,
        }
    }

    pub fn max_allowed(&self) -> u64 {
        match self {
            Self::Ms => 600_000,
            Self::Sec => 86_400,
            Self::Min => 1_440,
            Self::Hour => 72,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_true")]
    pub check_updates_on_startup: bool,
    #[serde(default = "default_true")]
    pub show_anti_cheat_notices: bool,

    #[serde(default = "default_true")]
    pub record_session_history: bool,
    #[serde(default = "default_true")]
    pub vac_shield_enabled: bool,
    #[serde(default)]
    pub stop_idling_on_steam_game_start: bool,

    #[serde(default)]
    pub favorite_app_ids: Vec<u32>,

    #[serde(default)]
    pub view_mode: ViewMode,
    #[serde(default)]
    pub sort_order: SortOrder,

    #[serde(default = "default_right_panel_width")]
    pub right_panel_width: f32,
    #[serde(default = "default_true")]
    pub right_panel_visible: bool,

    #[serde(default)]
    pub enable_auto_stop: bool,
    #[serde(default)]
    pub auto_stop_use_random: bool,
    #[serde(default)]
    pub auto_stop_unit: TimeUnit,
    #[serde(default = "default_auto_stop_value")]
    pub auto_stop_value: u64,
    #[serde(default = "default_auto_stop_min_value")]
    pub auto_stop_min_value: u64,
    #[serde(default = "default_auto_stop_max_value")]
    pub auto_stop_max_value: u64,

    #[serde(default)]
    pub planner_use_random_duration: bool,
    #[serde(default)]
    pub planner_duration_unit: TimeUnit,
    #[serde(default = "default_planner_duration_value")]
    pub planner_duration_value: u64,
    #[serde(default = "default_planner_min_duration_value")]
    pub planner_min_duration_value: u64,
    #[serde(default = "default_planner_max_duration_value")]
    pub planner_max_duration_value: u64,

    #[serde(default)]
    pub planner_use_random_cooldown: bool,
    #[serde(default = "default_cooldown_unit")]
    pub planner_cooldown_unit: TimeUnit,
    #[serde(default = "default_planner_cooldown_value")]
    pub planner_cooldown_value: u64,
    #[serde(default = "default_planner_min_cooldown_value")]
    pub planner_min_cooldown_value: u64,
    #[serde(default = "default_planner_max_cooldown_value")]
    pub planner_max_cooldown_value: u64,
}

fn default_true() -> bool {
    true
}

fn default_right_panel_width() -> f32 {
    340.0
}

fn default_cooldown_unit() -> TimeUnit {
    TimeUnit::Sec
}

fn default_auto_stop_value() -> u64 {
    15
}

fn default_auto_stop_min_value() -> u64 {
    16
}

fn default_auto_stop_max_value() -> u64 {
    22
}

fn default_planner_duration_value() -> u64 {
    15
}

fn default_planner_min_duration_value() -> u64 {
    16
}

fn default_planner_max_duration_value() -> u64 {
    22
}

fn default_planner_cooldown_value() -> u64 {
    30
}

fn default_planner_min_cooldown_value() -> u64 {
    15
}

fn default_planner_max_cooldown_value() -> u64 {
    90
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            check_updates_on_startup: true,
            show_anti_cheat_notices: true,
            record_session_history: true,
            vac_shield_enabled: true,
            stop_idling_on_steam_game_start: false,
            favorite_app_ids: Vec::new(),
            view_mode: ViewMode::List,
            sort_order: SortOrder::NameAsc,
            right_panel_width: default_right_panel_width(),
            right_panel_visible: true,
            enable_auto_stop: false,
            auto_stop_use_random: false,
            auto_stop_unit: TimeUnit::Min,
            auto_stop_value: default_auto_stop_value(),
            auto_stop_min_value: default_auto_stop_min_value(),
            auto_stop_max_value: default_auto_stop_max_value(),
            planner_use_random_duration: false,
            planner_duration_unit: TimeUnit::Min,
            planner_duration_value: default_planner_duration_value(),
            planner_min_duration_value: default_planner_min_duration_value(),
            planner_max_duration_value: default_planner_max_duration_value(),
            planner_use_random_cooldown: false,
            planner_cooldown_unit: TimeUnit::Sec,
            planner_cooldown_value: default_planner_cooldown_value(),
            planner_min_cooldown_value: default_planner_min_cooldown_value(),
            planner_max_cooldown_value: default_planner_max_cooldown_value(),
        }
    }
}

pub fn get_settings_file_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    local_app_data.join("SteamProcessIdler").join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = get_settings_file_path();
    if !path.exists() {
        return AppSettings::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(settings: &AppSettings) {
    let path = get_settings_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, json);
    }
}
