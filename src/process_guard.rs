use std::collections::HashSet;
use std::path::Path;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

pub const ANTI_CHEAT_PROCESSES: &[&str] = &[
    "easyanticheat.exe",
    "easyanticheat_eos.exe",
    "beservice.exe",
    "battleye.exe",
    "vgk.exe",
    "vgc.exe",
    "ace-tray.exe",
    "sguard64.exe",
    "gamemon.des",
    "xhunter1.exe",
    "equ8_conf.exe",
];

#[derive(Clone, Debug)]
pub struct ProcessSnapshotItem {
    pub pid: u32,
    pub exe_name: String,
}

pub fn get_system_processes() -> Vec<ProcessSnapshotItem> {
    let mut result = Vec::new();

    let snapshot = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(handle) => handle,
        Err(_) => return result,
    };

    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len])
                .trim()
                .to_lowercase();
            if !name.is_empty() {
                result.push(ProcessSnapshotItem {
                    pid: entry.th32ProcessID,
                    exe_name: name,
                });
            }

            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }

    let _ = unsafe { CloseHandle(snapshot) };
    result
}

pub fn extract_exe_filename(path_str: &str) -> String {
    let mut file_name = Path::new(path_str)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default()
        .to_lowercase();
    if !file_name.is_empty() && !file_name.ends_with(".exe") {
        file_name.push_str(".exe");
    }
    file_name
}

pub fn is_game_exe_running_externally(
    game_exe_name: &str,
    own_pids: &HashSet<u32>,
    processes: &[ProcessSnapshotItem],
) -> bool {
    let target = extract_exe_filename(game_exe_name);
    if target.is_empty() {
        return false;
    }

    processes
        .iter()
        .any(|p| p.exe_name == target && !own_pids.contains(&p.pid))
}

pub fn is_anti_cheat_running(processes: &[ProcessSnapshotItem]) -> bool {
    processes.iter().any(|p| {
        ANTI_CHEAT_PROCESSES
            .iter()
            .any(|&ac| p.exe_name.eq_ignore_ascii_case(ac))
    })
}
