use crate::models::SteamGame;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const MAGIC_APPINFO_26: u32 = 0x07564426;
const MAGIC_APPINFO_27: u32 = 0x07564427;
const MAGIC_APPINFO_28: u32 = 0x07564428;
const MAGIC_APPINFO_29: u32 = 0x07564429;

pub fn get_steam_installation_path() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(steam_key) = hkcu.open_subkey("Software\\Valve\\Steam") {
        if let Ok(steam_path) = steam_key.get_value::<String, _>("SteamPath") {
            let normalized = steam_path.replace('/', "\\");
            if Path::new(&normalized).exists() {
                return Some(normalized);
            }
        }
    }

    let default_path = PathBuf::from(r"C:\Program Files (x86)\Steam");
    if default_path.exists() {
        return Some(default_path.to_string_lossy().to_string());
    }

    None
}

pub fn get_owned_app_ids_and_last_played(steam_path: &str) -> (HashSet<u32>, HashMap<u32, u64>, HashMap<u32, u32>) {
    let mut owned_ids = HashSet::new();
    let mut last_played_map = HashMap::new();
    let mut playtime_map = HashMap::new();

    let user_data_dir = Path::new(steam_path).join("userdata");
    if user_data_dir.exists() {
        if let Ok(user_entries) = fs::read_dir(&user_data_dir) {
            let mut user_dirs = Vec::new();
            for user_entry in user_entries.flatten() {
                let user_dir = user_entry.path();
                if user_dir.is_dir() {
                    let local_config = user_dir.join("config").join("localconfig.vdf");
                    let modified = if local_config.exists() {
                        fs::metadata(&local_config)
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    } else {
                        std::time::SystemTime::UNIX_EPOCH
                    };
                    user_dirs.push((user_dir, modified));
                }
            }

            user_dirs.sort_by(|a, b| b.1.cmp(&a.1));

            for (user_dir, _) in user_dirs {
                let local_config = user_dir.join("config").join("localconfig.vdf");
                if local_config.exists() {
                    parse_vdf_for_apps(
                        &local_config,
                        &mut owned_ids,
                        Some(&mut last_played_map),
                        Some(&mut playtime_map),
                    );
                }

                scan_shared_configs_recursively(&user_dir, &mut owned_ids);
            }
        }
    }

    let mut library_folders = Path::new(steam_path).join("steamapps").join("libraryfolders.vdf");
    if !library_folders.exists() {
        library_folders = Path::new(steam_path).join("config").join("libraryfolders.vdf");
    }

    if library_folders.exists() {
        parse_library_folders(&library_folders, &mut owned_ids);
    }

    let steamapps_dir = Path::new(steam_path).join("steamapps");
    scan_app_manifests(&steamapps_dir, &mut owned_ids);

    (owned_ids, last_played_map, playtime_map)
}

fn scan_shared_configs_recursively(dir: &Path, app_ids: &mut HashSet<u32>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.to_lowercase().ends_with("sharedconfig.vdf") {
                        parse_vdf_for_apps(&path, app_ids, None, None);
                    }
                }
            } else if path.is_dir() {
                scan_shared_configs_recursively(&path, app_ids);
            }
        }
    }
}

fn parse_vdf_for_apps(
    file_path: &Path,
    app_ids: &mut HashSet<u32>,
    mut last_played_map: Option<&mut HashMap<u32, u64>>,
    mut playtime_map: Option<&mut HashMap<u32, u32>>,
) {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut in_apps_block = false;
    let mut current_app_id: Option<u32> = None;
    let mut brace_depth: i32 = 0;
    let mut apps_block_depth: i32 = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "{" {
            brace_depth += 1;
            continue;
        } else if trimmed == "}" {
            brace_depth -= 1;
            if in_apps_block && brace_depth <= apps_block_depth {
                in_apps_block = false;
                current_app_id = None;
            } else if in_apps_block && brace_depth == apps_block_depth + 1 {
                current_app_id = None;
            }
            continue;
        }

        if !in_apps_block {
            if trimmed.eq_ignore_ascii_case("\"apps\"") || trimmed.eq_ignore_ascii_case("\"UserAppConfig\"") {
                in_apps_block = true;
                apps_block_depth = brace_depth;
            }
        } else if brace_depth == apps_block_depth + 1 {
            if let Some(id_str) = extract_first_quoted_string(trimmed) {
                if let Ok(id) = id_str.parse::<u32>() {
                    if id > 0 {
                        app_ids.insert(id);
                        current_app_id = Some(id);
                    }
                }
            }
        } else if brace_depth == apps_block_depth + 2 {
            if let Some(app_id) = current_app_id {
                if trimmed.starts_with("\"LastPlayed\"") {
                    if let Some(val_str) = extract_second_quoted_string(trimmed) {
                        if let Ok(timestamp) = val_str.parse::<u64>() {
                            if let Some(ref mut map) = last_played_map {
                                let entry = map.entry(app_id).or_insert(0);
                                if timestamp > *entry {
                                    *entry = timestamp;
                                }
                            }
                        }
                    }
                } else if trimmed.starts_with("\"Playtime2wks\"") || trimmed.starts_with("\"Playtime\"") {
                    if let Some(val_str) = extract_second_quoted_string(trimmed) {
                        if let Ok(minutes) = val_str.parse::<u32>() {
                            if let Some(ref mut map) = playtime_map {
                                let entry = map.entry(app_id).or_insert(0);
                                if minutes > *entry {
                                    *entry = minutes;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn parse_library_folders(file_path: &Path, app_ids: &mut HashSet<u32>) {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut in_apps = false;
    let mut current_folder_path: Option<PathBuf> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("\"path\"") {
            if let Some(folder_path) = extract_second_quoted_string(trimmed) {
                let p = PathBuf::from(folder_path.replace(r"\\", r"\"));
                if p.exists() {
                    let steamapps = p.join("steamapps");
                    scan_app_manifests(&steamapps, app_ids);
                    current_folder_path = Some(p);
                }
            }
        }

        if trimmed == "\"apps\"" {
            in_apps = true;
            continue;
        }

        if in_apps {
            if trimmed == "}" {
                in_apps = false;
                continue;
            }

            if let Some(id_str) = extract_first_quoted_string(trimmed) {
                if let Ok(id) = id_str.parse::<u32>() {
                    if id > 0 {
                        app_ids.insert(id);
                    }
                }
            }
        }
    }

    if let Some(folder) = current_folder_path {
        let steamapps = folder.join("steamapps");
        scan_app_manifests(&steamapps, app_ids);
    }
}

fn scan_app_manifests(steamapps_dir: &Path, app_ids: &mut HashSet<u32>) {
    if !steamapps_dir.exists() {
        return;
    }

    if let Ok(entries) = fs::read_dir(steamapps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                if filename.starts_with("appmanifest_") && filename.ends_with(".acf") {
                    let id_part = &filename["appmanifest_".len()..filename.len() - ".acf".len()];
                    if let Ok(id) = id_part.parse::<u32>() {
                        if id > 0 {
                            app_ids.insert(id);
                        }
                    }
                }
            }
        }
    }
}

fn extract_first_quoted_string(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

fn extract_second_quoted_string(line: &str) -> Option<&str> {
    let first_end = line.find('"')? + 1 + line[line.find('"')? + 1..].find('"')? + 1;
    let second_start = line[first_end..].find('"')? + first_end + 1;
    let second_end = line[second_start..].find('"')? + second_start;
    Some(&line[second_start..second_end])
}

pub fn scan_games(custom_steam_path: Option<&str>) -> Vec<SteamGame> {
    let steam_path_str = match custom_steam_path {
        Some(p) => p.to_string(),
        None => match get_steam_installation_path() {
            Some(p) => p,
            None => return Vec::new(),
        },
    };

    let (_owned_app_ids, last_played_map, playtime_map) = get_owned_app_ids_and_last_played(&steam_path_str);
    let app_info_path = Path::new(&steam_path_str).join("appcache").join("appinfo.vdf");

    if !app_info_path.exists() {
        return Vec::new();
    }

    parse_app_info_vdf(&app_info_path, &steam_path_str, &last_played_map, &playtime_map)
}

fn parse_app_info_vdf(
    app_info_path: &Path,
    steam_path: &str,
    last_played_map: &HashMap<u32, u64>,
    playtime_map: &HashMap<u32, u32>,
) -> Vec<SteamGame> {
    let library_folders = get_steam_library_folders(steam_path);
    let mut file = match File::open(app_info_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let file_len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return Vec::new(),
    };

    let mut magic_bytes = [0u8; 4];
    if file.read_exact(&mut magic_bytes).is_err() {
        return Vec::new();
    }
    let magic = u32::from_le_bytes(magic_bytes);

    if magic != MAGIC_APPINFO_26
        && magic != MAGIC_APPINFO_27
        && magic != MAGIC_APPINFO_28
        && magic != MAGIC_APPINFO_29
    {
        return Vec::new();
    }

    let mut universe_bytes = [0u8; 4];
    if file.read_exact(&mut universe_bytes).is_err() {
        return Vec::new();
    }

    let string_table = if magic >= MAGIC_APPINFO_28 {
        let mut string_table_offset_bytes = [0u8; 8];
        if file.read_exact(&mut string_table_offset_bytes).is_err() {
            return Vec::new();
        }
        let string_table_offset = u64::from_le_bytes(string_table_offset_bytes);

        let current_pos = match file.stream_position() {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        let strings = if string_table_offset > 0 && string_table_offset < file_len {
            if file.seek(SeekFrom::Start(string_table_offset)).is_ok() {
                read_string_table(&mut file)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let _ = file.seek(SeekFrom::Start(current_pos));
        strings
    } else {
        Vec::new()
    };

    let mut games = Vec::new();

    loop {
        let mut app_id_bytes = [0u8; 4];
        match file.read_exact(&mut app_id_bytes) {
            Ok(_) => {}
            Err(_) => break,
        }
        let app_id = u32::from_le_bytes(app_id_bytes);
        if app_id == 0 {
            break;
        }

        let mut size_bytes = [0u8; 4];
        if file.read_exact(&mut size_bytes).is_err() {
            break;
        }
        let entry_data_size = u32::from_le_bytes(size_bytes);

        let mut info_state_bytes = [0u8; 4];
        let _ = file.read_exact(&mut info_state_bytes);

        let mut last_updated_bytes = [0u8; 4];
        let _ = file.read_exact(&mut last_updated_bytes);

        let mut access_token_bytes = [0u8; 8];
        let _ = file.read_exact(&mut access_token_bytes);

        let mut hash_bytes = [0u8; 20];
        let _ = file.read_exact(&mut hash_bytes);

        let mut change_number_bytes = [0u8; 4];
        let _ = file.read_exact(&mut change_number_bytes);

        if magic >= MAGIC_APPINFO_27 {
            let mut binary_data_hash_bytes = [0u8; 20];
            let _ = file.read_exact(&mut binary_data_hash_bytes);
        }

        let header_fields_size: u32 = if magic >= MAGIC_APPINFO_27 {
            4 + 4 + 8 + 20 + 4 + 20
        } else {
            4 + 4 + 8 + 20 + 4
        };

        if entry_data_size <= header_fields_size {
            continue;
        }

        let kv_data_size = (entry_data_size - header_fields_size) as usize;
        let mut kv_data = vec![0u8; kv_data_size];
        if file.read_exact(&mut kv_data).is_err() {
            break;
        }

        let last_played_unix = last_played_map.get(&app_id).copied().unwrap_or(0);
        let playtime_minutes = playtime_map.get(&app_id).copied().unwrap_or(0);

        if let Some(game) = parse_kv_data(
            app_id,
            &kv_data,
            &string_table,
            steam_path,
            &library_folders,
            last_played_unix,
            playtime_minutes,
        ) {
            games.push(game);
        }
    }

    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    games
}

fn read_string_table<R: Read>(reader: &mut R) -> Vec<String> {
    let mut count_bytes = [0u8; 4];
    if reader.read_exact(&mut count_bytes).is_err() {
        return Vec::new();
    }
    let count = u32::from_le_bytes(count_bytes) as usize;

    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        let mut string_bytes = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match reader.read_exact(&mut byte) {
                Ok(_) => {
                    if byte[0] == 0 {
                        break;
                    }
                    string_bytes.push(byte[0]);
                }
                Err(_) => break,
            }
        }
        table.push(String::from_utf8_lossy(&string_bytes).to_string());
    }

    table
}

struct KvReader<'a> {
    data: &'a [u8],
    offset: usize,
    string_table: &'a [String],
}

impl<'a> KvReader<'a> {
    fn new(data: &'a [u8], string_table: &'a [String]) -> Self {
        Self {
            data,
            offset: 0,
            string_table,
        }
    }

    fn read_byte(&mut self) -> Option<u8> {
        if self.offset < self.data.len() {
            let b = self.data[self.offset];
            self.offset += 1;
            Some(b)
        } else {
            None
        }
    }

    fn read_u32(&mut self) -> Option<u32> {
        if self.offset + 4 <= self.data.len() {
            let val = u32::from_le_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
            ]);
            self.offset += 4;
            Some(val)
        } else {
            None
        }
    }

    fn read_cstring(&mut self) -> Option<String> {
        let start = self.offset;
        while self.offset < self.data.len() {
            if self.data[self.offset] == 0 {
                let s = String::from_utf8_lossy(&self.data[start..self.offset]).to_string();
                self.offset += 1;
                return Some(s);
            }
            self.offset += 1;
        }
        None
    }

    fn read_key(&mut self) -> Option<String> {
        if !self.string_table.is_empty() {
            let idx = self.read_u32()? as usize;
            if idx < self.string_table.len() {
                Some(self.string_table[idx].clone())
            } else {
                Some(String::new())
            }
        } else {
            self.read_cstring()
        }
    }

    fn skip_value(&mut self, node_type: u8) {
        match node_type {
            0x01 => {
                let _ = self.read_cstring();
            }
            0x02 => {
                self.offset += 4;
            }
            0x03 => {
                self.offset += 4;
            }
            0x04 => {
                self.offset += 4;
            }
            0x05 => {
                self.offset += 4;
            }
            0x06 => {
                self.offset += 4;
            }
            0x07 => {
                self.offset += 8;
            }
            _ => {}
        }
    }

    fn skip_section(&mut self) {
        let mut depth = 1;
        while depth > 0 {
            let node_type = match self.read_byte() {
                Some(t) => t,
                None => break,
            };

            if node_type == 0x08 || node_type == 0x0B {
                depth -= 1;
                continue;
            }

            let _ = self.read_key();
            if node_type == 0x00 {
                depth += 1;
            } else {
                self.skip_value(node_type);
            }
        }
    }
}

fn get_steam_library_folders(steam_path: &str) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let main_steamapps = Path::new(steam_path).join("steamapps");
    if main_steamapps.exists() {
        folders.push(PathBuf::from(steam_path));
    }

    let mut library_folders = Path::new(steam_path).join("steamapps").join("libraryfolders.vdf");
    if !library_folders.exists() {
        library_folders = Path::new(steam_path).join("config").join("libraryfolders.vdf");
    }

    if library_folders.exists() {
        if let Ok(content) = fs::read_to_string(&library_folders) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("\"path\"") {
                    if let Some(folder_str) = extract_second_quoted_string(trimmed) {
                        let p = PathBuf::from(folder_str.replace(r"\\", r"\"));
                        if p.exists() && !folders.contains(&p) {
                            folders.push(p);
                        }
                    }
                }
            }
        }
    }

    if folders.is_empty() {
        folders.push(PathBuf::from(steam_path));
    }

    folders
}

fn check_local_install_for_anti_cheat(library_folders: &[PathBuf], install_dir: &str) -> bool {
    let trimmed = install_dir.trim();
    if trimmed.is_empty() {
        return false;
    }

    let candidate_subdirs = [
        "EasyAntiCheat",
        "EasyAntiCheat_EOS",
        "BattlEye",
        "GameGuard",
        "XIGNCODE",
    ];

    for lib in library_folders {
        let game_dir = lib.join("steamapps").join("common").join(trimmed);
        if game_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&game_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            for cand in &candidate_subdirs {
                                if name.eq_ignore_ascii_case(cand) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

fn has_anti_cheat_signature(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("easyanticheat")
        || t.contains("battleye")
        || t.contains("vanguard")
        || t.contains("ricochet")
        || t.contains("eac_launcher")
        || t.contains("gameguard")
        || t.contains("xigncode")
        || t.contains("equ8")
        || t.contains("fairfight")
        || t.contains("ace-base")
        || t.contains("anticheat")
        || t.contains("denuvo")
        || t.contains("1826330")
}

fn scan_section_for_anti_cheat(reader: &mut KvReader<'_>, is_third_party_anti_cheat: &mut bool) {
    let mut depth = 1;
    while depth > 0 {
        let node_type = match reader.read_byte() {
            Some(t) => t,
            None => break,
        };

        if node_type == 0x08 || node_type == 0x0B {
            depth -= 1;
            continue;
        }

        let key = reader.read_key().unwrap_or_default();
        if has_anti_cheat_signature(&key) {
            *is_third_party_anti_cheat = true;
        }

        if node_type == 0x00 {
            depth += 1;
        } else if node_type == 0x01 {
            let val = reader.read_cstring().unwrap_or_default();
            if has_anti_cheat_signature(&val) {
                *is_third_party_anti_cheat = true;
            }
        } else if node_type == 0x02 {
            let val = reader.read_u32().unwrap_or_default();
            if val == 1826330 {
                *is_third_party_anti_cheat = true;
            }
        } else {
            reader.skip_value(node_type);
        }
    }
}

fn parse_kv_data(
    app_id: u32,
    kv_data: &[u8],
    string_table: &[String],
    steam_path: &str,
    library_folders: &[PathBuf],
    last_played_unix: u64,
    playtime_minutes: u32,
) -> Option<SteamGame> {
    let mut reader = KvReader::new(kv_data, string_table);

    let mut name = None;
    let mut app_type = None;
    let mut clienticon = None;
    let mut install_dir = String::new();
    let mut is_vac_protected = false;
    let mut is_third_party_anti_cheat = false;
    let mut candidate_exes = Vec::new();

    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        if node_type == 0x00 {
            if key.eq_ignore_ascii_case("appinfo") {
                parse_appinfo_section(
                    &mut reader,
                    &mut name,
                    &mut app_type,
                    &mut clienticon,
                    &mut install_dir,
                    &mut is_vac_protected,
                    &mut is_third_party_anti_cheat,
                    &mut candidate_exes,
                );
            } else if key.eq_ignore_ascii_case("common") {
                parse_common_section(
                    &mut reader,
                    &mut name,
                    &mut app_type,
                    &mut clienticon,
                    &mut install_dir,
                    &mut is_vac_protected,
                    &mut is_third_party_anti_cheat,
                );
            } else if key.eq_ignore_ascii_case("config") {
                parse_config_section(
                    &mut reader,
                    &mut candidate_exes,
                    &mut is_third_party_anti_cheat,
                );
            } else if key.eq_ignore_ascii_case("extended") || key.eq_ignore_ascii_case("depots") {
                scan_section_for_anti_cheat(&mut reader, &mut is_third_party_anti_cheat);
            } else {
                reader.skip_section();
            }
        } else {
            reader.skip_value(node_type);
        }
    }

    let is_game = app_type
        .as_deref()
        .map(|t| t.eq_ignore_ascii_case("game"))
        .unwrap_or(false);

    if !is_game {
        return None;
    }

    let game_name = name.unwrap_or_else(|| format!("App {}", app_id));
    if game_name.is_empty() {
        return None;
    }

    let mut scored_exes: Vec<(i32, String)> = candidate_exes
        .into_iter()
        .map(|exe| (score_executable(&exe, &game_name, &install_dir), exe))
        .collect();
    scored_exes.sort_by(|a, b| b.0.cmp(&a.0));

    let available: Vec<String> = scored_exes.into_iter().map(|(_, exe)| exe).collect();
    let primary_exe = available.first().cloned().unwrap_or_else(|| "game.exe".to_string());

    let icon_path = resolve_game_icon(app_id, clienticon.as_deref(), steam_path);
    let is_never_played = last_played_unix == 0 && playtime_minutes == 0;

    if !is_third_party_anti_cheat && check_local_install_for_anti_cheat(library_folders, &install_dir) {
        is_third_party_anti_cheat = true;
    }

    if is_vac_protected {
        is_third_party_anti_cheat = false;
    }

    let banner_path = find_local_banner(steam_path, app_id);
    let default_executable = primary_exe.clone();

    Some(SteamGame {
        app_id,
        name: game_name,
        install_dir,
        executable_relative_path: primary_exe,
        default_executable,
        available_executables: available,
        icon_path,
        banner_path,
        client_icon_hash: clienticon,
        is_running: false,
        is_custom: false,
        is_discord_detectable: false,
        is_never_played,
        is_vac_protected,
        is_third_party_anti_cheat,
        last_played_unix,
        playtime_minutes,
    })
}

fn parse_appinfo_section(
    reader: &mut KvReader<'_>,
    name: &mut Option<String>,
    app_type: &mut Option<String>,
    clienticon: &mut Option<String>,
    install_dir: &mut String,
    is_vac_protected: &mut bool,
    is_third_party_anti_cheat: &mut bool,
    candidate_exes: &mut Vec<String>,
) {
    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        if node_type == 0x00 {
            if key.eq_ignore_ascii_case("common") {
                parse_common_section(reader, name, app_type, clienticon, install_dir, is_vac_protected, is_third_party_anti_cheat);
            } else if key.eq_ignore_ascii_case("config") {
                parse_config_section(reader, candidate_exes, is_third_party_anti_cheat);
            } else if key.eq_ignore_ascii_case("extended") || key.eq_ignore_ascii_case("depots") {
                scan_section_for_anti_cheat(reader, is_third_party_anti_cheat);
            } else {
                reader.skip_section();
            }
        } else {
            reader.skip_value(node_type);
        }
    }
}

fn parse_common_section(
    reader: &mut KvReader<'_>,
    name: &mut Option<String>,
    app_type: &mut Option<String>,
    clienticon: &mut Option<String>,
    install_dir: &mut String,
    is_vac_protected: &mut bool,
    is_third_party_anti_cheat: &mut bool,
) {
    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        if has_anti_cheat_signature(&key) {
            *is_third_party_anti_cheat = true;
        }

        match node_type {
            0x00 => {
                if key.eq_ignore_ascii_case("category") {
                    parse_category_section(reader, is_vac_protected);
                } else {
                    reader.skip_section();
                }
            }
            0x01 => {
                let val = reader.read_cstring().unwrap_or_default();
                if has_anti_cheat_signature(&val) {
                    *is_third_party_anti_cheat = true;
                }
                if key.eq_ignore_ascii_case("name") && name.is_none() {
                    *name = Some(val);
                } else if key.eq_ignore_ascii_case("type") && app_type.is_none() {
                    *app_type = Some(val);
                } else if key.eq_ignore_ascii_case("clienticon") && clienticon.is_none() {
                    *clienticon = Some(val);
                } else if key.eq_ignore_ascii_case("installdir") && install_dir.is_empty() {
                    *install_dir = val;
                } else if key.eq_ignore_ascii_case("category_8") && val == "1" {
                    *is_vac_protected = true;
                }
            }
            0x02 => {
                let val = reader.read_u32().unwrap_or_default();
                if val == 1826330 {
                    *is_third_party_anti_cheat = true;
                }
                if key.eq_ignore_ascii_case("category_8") && val == 1 {
                    *is_vac_protected = true;
                }
            }
            _ => {
                reader.skip_value(node_type);
            }
        }
    }
}

fn parse_category_section(reader: &mut KvReader<'_>, is_vac_protected: &mut bool) {
    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        match node_type {
            0x00 => {
                reader.skip_section();
            }
            0x01 => {
                let val = reader.read_cstring().unwrap_or_default();
                if (key == "8" || key.eq_ignore_ascii_case("category_8")) && val == "1" {
                    *is_vac_protected = true;
                }
            }
            0x02 => {
                let val = reader.read_u32().unwrap_or_default();
                if (key == "8" || key.eq_ignore_ascii_case("category_8")) && val == 1 {
                    *is_vac_protected = true;
                }
            }
            _ => {
                reader.skip_value(node_type);
            }
        }
    }
}

fn parse_config_section(
    reader: &mut KvReader<'_>,
    candidate_exes: &mut Vec<String>,
    is_third_party_anti_cheat: &mut bool,
) {
    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        if node_type == 0x00 {
            if key.eq_ignore_ascii_case("launch") {
                parse_launch_section(reader, candidate_exes, is_third_party_anti_cheat);
            } else {
                reader.skip_section();
            }
        } else {
            reader.skip_value(node_type);
        }
    }
}

fn parse_launch_section(
    reader: &mut KvReader<'_>,
    candidate_exes: &mut Vec<String>,
    is_third_party_anti_cheat: &mut bool,
) {
    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let _launch_index_key = reader.read_key().unwrap_or_default();
        if node_type == 0x00 {
            parse_single_launch_item(reader, candidate_exes, is_third_party_anti_cheat);
        } else {
            reader.skip_value(node_type);
        }
    }
}

fn parse_single_launch_item(
    reader: &mut KvReader<'_>,
    candidate_exes: &mut Vec<String>,
    is_third_party_anti_cheat: &mut bool,
) {
    let mut executable = None;
    let mut is_windows = true;

    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        if node_type == 0x00 {
            if key.eq_ignore_ascii_case("config") {
                parse_launch_config_os(reader, &mut is_windows);
            } else {
                reader.skip_section();
            }
        } else if node_type == 0x01 {
            let val = reader.read_cstring().unwrap_or_default();
            if has_anti_cheat_signature(&val) || has_anti_cheat_signature(&key) {
                *is_third_party_anti_cheat = true;
            }

            if key.eq_ignore_ascii_case("executable") {
                executable = Some(val);
            }
        } else {
            reader.skip_value(node_type);
        }
    }

    if is_windows {
        if let Some(exe) = executable {
            let normalized = exe.replace('/', "\\");
            if normalized.to_lowercase().ends_with(".exe") && !candidate_exes.contains(&normalized) {
                candidate_exes.push(normalized);
            }
        }
    }
}

fn parse_launch_config_os(reader: &mut KvReader<'_>, is_windows: &mut bool) {
    while let Some(node_type) = reader.read_byte() {
        if node_type == 0x08 || node_type == 0x0B {
            break;
        }

        let key = reader.read_key().unwrap_or_default();
        if node_type == 0x01 {
            let val = reader.read_cstring().unwrap_or_default();
            if key.eq_ignore_ascii_case("oslist") {
                let lower = val.to_lowercase();
                if !lower.contains("windows") && !lower.contains("win32") && !lower.contains("win64") {
                    *is_windows = false;
                }
            }
        } else if node_type == 0x00 {
            reader.skip_section();
        } else {
            reader.skip_value(node_type);
        }
    }
}

fn score_executable(exe: &str, game_name: &str, install_dir: &str) -> i32 {
    let lower_exe = exe.to_lowercase();
    let lower_game = game_name.to_lowercase().replace(' ', "");
    let lower_dir = install_dir.to_lowercase().replace(' ', "");

    let file_name = Path::new(exe)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(exe)
        .to_lowercase();

    if file_name.contains("unins")
        || file_name.contains("crash")
        || file_name.contains("report")
        || file_name.contains("setup")
        || file_name.contains("config")
        || file_name.contains("dxsetup")
        || file_name.contains("vcredist")
    {
        return -100;
    }

    let mut score = 0;

    if !exe.contains('\\') && !exe.contains('/') {
        score += 30;
    }

    if !lower_dir.is_empty() && file_name.contains(&lower_dir) {
        score += 40;
    }

    let clean_game_name = lower_game
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();
    let clean_file_name = file_name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();

    if !clean_game_name.is_empty() && clean_file_name.contains(&clean_game_name) {
        score += 50;
    }

    if file_name.contains("launcher") {
        score -= 10;
    }

    if lower_exe.contains("bin64") || lower_exe.contains("x64") || lower_exe.contains("win64") {
        score += 15;
    }

    score
}

pub fn resolve_game_icon(app_id: u32, client_icon: Option<&str>, steam_path: &str) -> Option<String> {
    find_local_icon(steam_path, app_id, client_icon)
}

fn find_local_icon(steam_path: &str, app_id: u32, client_icon: Option<&str>) -> Option<String> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let icon_cache_dir = local_app_data.join("SteamProcessIdler").join("IconCache");

    let cached_ico = icon_cache_dir.join(format!("{}.ico", app_id));
    if cached_ico.exists() {
        return Some(cached_ico.to_string_lossy().to_string());
    }

    let cached_png = icon_cache_dir.join(format!("{}.png", app_id));
    if cached_png.exists() {
        return Some(cached_png.to_string_lossy().to_string());
    }

    let cached_jpg = icon_cache_dir.join(format!("{}.jpg", app_id));
    if cached_jpg.exists() {
        return Some(cached_jpg.to_string_lossy().to_string());
    }

    let cached_capsule = icon_cache_dir.join(format!("{}_capsule.jpg", app_id));
    if cached_capsule.exists() {
        return Some(cached_capsule.to_string_lossy().to_string());
    }

    let cached_header = icon_cache_dir.join(format!("{}_header.jpg", app_id));
    if cached_header.exists() {
        return Some(cached_header.to_string_lossy().to_string());
    }

    let steam_dir = Path::new(steam_path);
    let library_cache_dir = steam_dir.join("appcache").join("librarycache");
    let app_cache_dir = library_cache_dir.join(app_id.to_string());

    if let Some(hash) = client_icon {
        if !hash.is_empty() {
            let primary_ico = steam_dir.join("steam").join("games").join(format!("{}.ico", hash));
            if primary_ico.exists() {
                return Some(primary_ico.to_string_lossy().to_string());
            }

            let app_cache_jpg = app_cache_dir.join(format!("{}.jpg", hash));
            if app_cache_jpg.exists() {
                return Some(app_cache_jpg.to_string_lossy().to_string());
            }

            let app_cache_png = app_cache_dir.join(format!("{}.png", hash));
            if app_cache_png.exists() {
                return Some(app_cache_png.to_string_lossy().to_string());
            }

            let app_cache_ico = app_cache_dir.join(format!("{}.ico", hash));
            if app_cache_ico.exists() {
                return Some(app_cache_ico.to_string_lossy().to_string());
            }
        }
    }

    let app_icon_jpg = library_cache_dir.join(format!("{}_icon.jpg", app_id));
    if app_icon_jpg.exists() {
        return Some(app_icon_jpg.to_string_lossy().to_string());
    }

    let app_icon_png = library_cache_dir.join(format!("{}_icon.png", app_id));
    if app_icon_png.exists() {
        return Some(app_icon_png.to_string_lossy().to_string());
    }

    let app_header_jpg = library_cache_dir.join(format!("{}_header.jpg", app_id));
    if app_header_jpg.exists() {
        return Some(app_header_jpg.to_string_lossy().to_string());
    }

    let app_library_jpg = library_cache_dir.join(format!("{}_library_600x900.jpg", app_id));
    if app_library_jpg.exists() {
        return Some(app_library_jpg.to_string_lossy().to_string());
    }

    let icon_jpg = app_cache_dir.join("icon.jpg");
    if icon_jpg.exists() {
        return Some(icon_jpg.to_string_lossy().to_string());
    }

    let header_jpg = app_cache_dir.join("header.jpg");
    if header_jpg.exists() {
        return Some(header_jpg.to_string_lossy().to_string());
    }

    let icon_png = app_cache_dir.join("icon.png");
    if icon_png.exists() {
        return Some(icon_png.to_string_lossy().to_string());
    }

    let icon_ico = app_cache_dir.join("icon.ico");
    if icon_ico.exists() {
        return Some(icon_ico.to_string_lossy().to_string());
    }

    None
}

pub fn download_missing_icon(app_id: u32, hash: Option<&str>) -> Option<String> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let icon_cache_dir = local_app_data.join("SteamProcessIdler").join("IconCache");
    let _ = fs::create_dir_all(&icon_cache_dir);

    if let Some(h) = hash {
        if !h.is_empty() {
            let ico_path = icon_cache_dir.join(format!("{}.ico", app_id));
            let ico_url = format!(
                "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{}/{}.ico",
                app_id, h
            );
            if let Ok(resp) = ureq::get(&ico_url).call() {
                let mut bytes = Vec::new();
                if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                    if fs::write(&ico_path, &bytes).is_ok() {
                        return Some(ico_path.to_string_lossy().to_string());
                    }
                }
            }

            let jpg_path = icon_cache_dir.join(format!("{}.jpg", app_id));
            let jpg_url = format!(
                "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{}/{}.jpg",
                app_id, h
            );
            if let Ok(resp) = ureq::get(&jpg_url).call() {
                let mut bytes = Vec::new();
                if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                    if fs::write(&jpg_path, &bytes).is_ok() {
                        return Some(jpg_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    let capsule_path = icon_cache_dir.join(format!("{}_capsule.jpg", app_id));
    let capsule_url = format!(
        "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/{}/capsule_184x69.jpg",
        app_id
    );
    if let Ok(resp) = ureq::get(&capsule_url).call() {
        let mut bytes = Vec::new();
        if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
            if fs::write(&capsule_path, &bytes).is_ok() {
                return Some(capsule_path.to_string_lossy().to_string());
            }
        }
    }

    let header_path = icon_cache_dir.join(format!("{}_header.jpg", app_id));
    let header_url = format!(
        "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg",
        app_id
    );
    if let Ok(resp) = ureq::get(&header_url).call() {
        let mut bytes = Vec::new();
        if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
            if fs::write(&header_path, &bytes).is_ok() {
                return Some(header_path.to_string_lossy().to_string());
            }
        }
    }

    None
}

pub fn get_banner_cache_dir() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    local_app_data.join("SteamProcessIdler").join("BannerCache")
}

pub fn find_local_banner(steam_path: &str, app_id: u32) -> Option<String> {
    let banner_cache_dir = get_banner_cache_dir();
    let cached_banner = banner_cache_dir.join(format!("{}.jpg", app_id));
    if cached_banner.exists() {
        return Some(cached_banner.to_string_lossy().to_string());
    }

    let library_cache = Path::new(steam_path).join("appcache").join("librarycache");
    if library_cache.exists() {
        let header = library_cache.join(format!("{}_header.jpg", app_id));
        if header.exists() {
            return Some(header.to_string_lossy().to_string());
        }

        let capsule = library_cache.join(format!("{}_capsule_231x87.jpg", app_id));
        if capsule.exists() {
            return Some(capsule.to_string_lossy().to_string());
        }

        let library_600 = library_cache.join(format!("{}_library_600x900.jpg", app_id));
        if library_600.exists() {
            return Some(library_600.to_string_lossy().to_string());
        }
    }

    None
}

pub fn download_missing_banner(app_id: u32) -> Option<String> {
    let banner_cache_dir = get_banner_cache_dir();
    let _ = fs::create_dir_all(&banner_cache_dir);
    let target_path = banner_cache_dir.join(format!("{}.jpg", app_id));

    let header_url = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg", app_id);
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    if let Ok(resp) = agent.get(&header_url).set("User-Agent", "Mozilla/5.0").call() {
        if resp.status() == 200 {
            let mut bytes = Vec::new();
            if resp.into_reader().read_to_end(&mut bytes).is_ok() && bytes.len() > 500 {
                if fs::write(&target_path, &bytes).is_ok() {
                    return Some(target_path.to_string_lossy().to_string());
                }
            }
        }
    }

    let capsule_url = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{}/capsule_231x87.jpg", app_id);
    if let Ok(resp) = agent.get(&capsule_url).set("User-Agent", "Mozilla/5.0").call() {
        if resp.status() == 200 {
            let mut bytes = Vec::new();
            if resp.into_reader().read_to_end(&mut bytes).is_ok() && bytes.len() > 500 {
                if fs::write(&target_path, &bytes).is_ok() {
                    return Some(target_path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}
