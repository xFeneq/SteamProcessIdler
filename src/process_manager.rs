use crate::models::{ActiveSession, SteamGame};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

pub fn validate_relative_path(path_str: &str, param_name: &str) -> Result<(), String> {
    if path_str.trim().is_empty() {
        return Ok(());
    }

    let path = Path::new(path_str);
    if path.is_absolute() {
        return Err(format!(
            "Parameter '{}' must be a relative path, but a rooted path was provided: '{}'",
            param_name, path_str
        ));
    }

    if path_str.contains("..") {
        return Err(format!(
            "Parameter '{}' cannot contain parent directory traversal ('..'): '{}'",
            param_name, path_str
        ));
    }

    Ok(())
}

pub fn get_sessions_root_dir() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    local_app_data.join("SteamProcessIdler").join("Sessions")
}

pub fn ensure_steam_api_dll(steam_path: Option<&str>) -> Option<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let app_data_dll = local_app_data.join("SteamProcessIdler").join("steam_api64.dll");

    if app_data_dll.exists() {
        return Some(app_data_dll);
    }

    if let Ok(exe_dir) = std::env::current_exe().map(|p| p.parent().unwrap_or(Path::new(".")).to_path_buf()) {
        let local_dll = exe_dir.join("steam_api64.dll");
        if local_dll.exists() {
            let _ = fs::create_dir_all(app_data_dll.parent().unwrap_or(Path::new(".")));
            let _ = fs::copy(&local_dll, &app_data_dll);
            return Some(app_data_dll);
        }
    }

    if let Some(steam_root) = steam_path {
        let p1 = Path::new(steam_root).join("steam_api64.dll");
        if p1.exists() {
            let _ = fs::create_dir_all(app_data_dll.parent().unwrap_or(Path::new(".")));
            let _ = fs::copy(&p1, &app_data_dll);
            return Some(app_data_dll);
        }

        let p2 = Path::new(steam_root).join("bin").join("steam_api64.dll");
        if p2.exists() {
            let _ = fs::create_dir_all(app_data_dll.parent().unwrap_or(Path::new(".")));
            let _ = fs::copy(&p2, &app_data_dll);
            return Some(app_data_dll);
        }

        let common_dir = Path::new(steam_root).join("steamapps").join("common");
        if common_dir.exists() {
            if let Some(found) = find_file_in_dir(&common_dir, "steam_api64.dll", 4) {
                let _ = fs::create_dir_all(app_data_dll.parent().unwrap_or(Path::new(".")));
                let _ = fs::copy(&found, &app_data_dll);
                return Some(app_data_dll);
            }
        }
    }

    None
}

fn find_file_in_dir(dir: &Path, target_filename: &str, max_depth: usize) -> Option<PathBuf> {
    if max_depth == 0 {
        return None;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.eq_ignore_ascii_case(target_filename) {
                        return Some(path);
                    }
                }
            } else if path.is_dir() {
                if let Some(res) = find_file_in_dir(&path, target_filename, max_depth - 1) {
                    return Some(res);
                }
            }
        }
    }

    None
}

pub fn start_game_session(game: &SteamGame, steam_path: Option<&str>) -> Result<ActiveSession, String> {
    validate_relative_path(&game.install_dir, "InstallDir")?;
    validate_relative_path(&game.executable_relative_path, "ExecutableRelativePath")?;

    let sessions_root = get_sessions_root_dir();
    let session_dir = sessions_root.join(game.app_id.to_string());
    let _ = fs::create_dir_all(&session_dir);

    let exe_sub_dir = Path::new(&game.executable_relative_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    validate_relative_path(&exe_sub_dir, "ExecutableSubDir")?;

    let target_dir = if !game.install_dir.is_empty() {
        session_dir.join(&game.install_dir).join(&exe_sub_dir)
    } else {
        session_dir.join(&exe_sub_dir)
    };

    fs::create_dir_all(&target_dir).map_err(|e| format!("Failed to create session target dir: {}", e))?;

    let host_exe = std::env::current_exe().map_err(|e| format!("Cannot locate host executable: {}", e))?;
    let mut exe_file_name = Path::new(&game.executable_relative_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "game.exe".to_string());

    if !exe_file_name.to_lowercase().ends_with(".exe") {
        exe_file_name.push_str(".exe");
    }

    let target_exe_path = target_dir.join(&exe_file_name);
    fs::copy(&host_exe, &target_exe_path).map_err(|e| format!("Failed to copy host executable: {}", e))?;

    if let Some(source_dll) = ensure_steam_api_dll(steam_path) {
        let dest_dll = target_dir.join("steam_api64.dll");
        let _ = fs::copy(&source_dll, &dest_dll);
    }

    let app_id_file = target_dir.join("steam_appid.txt");
    let _ = fs::write(app_id_file, game.app_id.to_string());

    let child = Command::new(&target_exe_path)
        .args(["--stub", &game.app_id.to_string(), "--name", &game.name])
        .current_dir(&target_dir)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("Failed to spawn stub process: {}", e))?;

    let pid = child.id();
    let pid_file = session_dir.join("session.pid");
    let _ = fs::write(pid_file, pid.to_string());

    Ok(ActiveSession {
        app_id: game.app_id,
        game_name: game.name.clone(),
        process_name: exe_file_name,
        pid,
        start_time: Instant::now(),
        temp_directory: session_dir,
        scheduled_stop: None,
        scheduled_duration_secs: None,
    })
}

pub fn stop_session(session: &ActiveSession) {
    let pid_str = session.pid.to_string();
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid_str])
        .creation_flags(0x08000000)
        .output();

    safe_delete_directory(&session.temp_directory, 5, 200);
}

pub fn stop_all_sessions(sessions: &mut Vec<ActiveSession>) {
    for session in sessions.drain(..) {
        stop_session(&session);
    }
}

pub fn cleanup_orphaned_sessions() {
    let sessions_root = get_sessions_root_dir();
    if !sessions_root.exists() {
        return;
    }

    if let Ok(entries) = fs::read_dir(&sessions_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let pid_file = path.join("session.pid");
                let is_running = if pid_file.exists() {
                    if let Ok(content) = fs::read_to_string(&pid_file) {
                        if let Ok(pid) = content.trim().parse::<u32>() {
                            is_pid_alive(pid)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !is_running {
                    safe_delete_directory(&path, 3, 100);
                }
            }
        }
    }
}

fn is_pid_alive(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .creation_flags(0x08000000)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        text.contains(&pid.to_string())
    } else {
        false
    }
}

pub fn safe_delete_directory(dir_path: &Path, max_retries: usize, delay_ms: u64) {
    if !dir_path.exists() {
        return;
    }

    for _ in 0..max_retries {
        if fs::remove_dir_all(dir_path).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }
}
