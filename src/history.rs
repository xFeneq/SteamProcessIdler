use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GameHistoryStats {
    pub app_id: u32,
    pub game_name: String,
    pub total_idled_seconds: u64,
    pub session_count: u32,
    pub last_session_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionHistoryRecord {
    pub app_id: u32,
    pub game_name: String,
    pub duration_seconds: u64,
    pub timestamp_unix: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HistoryStore {
    #[serde(default)]
    pub games: HashMap<u32, GameHistoryStats>,
    #[serde(default)]
    pub recent_sessions: Vec<SessionHistoryRecord>,
}

pub fn get_history_file_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    local_app_data.join("SteamProcessIdler").join("history.json")
}

pub fn load_history() -> HistoryStore {
    let path = get_history_file_path();
    if !path.exists() {
        return HistoryStore::default();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HistoryStore::default(),
    }
}

pub fn save_history(store: &HistoryStore) {
    let path = get_history_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path, json);
    }
}

pub fn clear_history_file() -> bool {
    let path = get_history_file_path();
    if path.exists() {
        fs::remove_file(path).is_ok()
    } else {
        true
    }
}

pub fn record_session_finish(enabled: bool, app_id: u32, game_name: &str, duration_seconds: u64) {
    if !enabled || duration_seconds == 0 {
        return;
    }

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut store = load_history();

    let entry = store.games.entry(app_id).or_insert_with(|| GameHistoryStats {
        app_id,
        game_name: game_name.to_string(),
        total_idled_seconds: 0,
        session_count: 0,
        last_session_unix: 0,
    });

    entry.game_name = game_name.to_string();
    entry.total_idled_seconds = entry.total_idled_seconds.saturating_add(duration_seconds);
    entry.session_count = entry.session_count.saturating_add(1);
    entry.last_session_unix = now_unix;

    store.recent_sessions.insert(
        0,
        SessionHistoryRecord {
            app_id,
            game_name: game_name.to_string(),
            duration_seconds,
            timestamp_unix: now_unix,
        },
    );

    if store.recent_sessions.len() > 100 {
        store.recent_sessions.truncate(100);
    }

    save_history(&store);
}

pub fn format_duration_human(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        let m = seconds / 60;
        let s = seconds % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m {}s", m, s)
        }
    } else {
        let hours = (seconds as f64) / 3600.0;
        format!("{:.1} hrs", hours)
    }
}

pub fn format_timestamp_human(ts: u64) -> String {
    if ts == 0 {
        return "Never".to_string();
    }

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
