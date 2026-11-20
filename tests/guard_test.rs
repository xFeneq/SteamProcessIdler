use SteamProcessIdler::process_guard::{
    extract_exe_filename, is_anti_cheat_running, is_game_exe_running_externally,
    ProcessSnapshotItem, ANTI_CHEAT_PROCESSES,
};
use std::collections::HashSet;

#[test]
fn test_extract_exe_filename() {
    assert_eq!(extract_exe_filename("cs2.exe"), "cs2.exe");
    assert_eq!(extract_exe_filename("game/bin/cs2.exe"), "cs2.exe");
    assert_eq!(extract_exe_filename("game\\bin\\cs2"), "cs2.exe");
    assert_eq!(extract_exe_filename("Dota 2\\dota2.EXE"), "dota2.exe");
}

#[test]
fn test_is_anti_cheat_running() {
    let empty: Vec<ProcessSnapshotItem> = Vec::new();
    assert!(!is_anti_cheat_running(&empty));

    let normal_processes = vec![
        ProcessSnapshotItem {
            pid: 100,
            exe_name: "explorer.exe".to_string(),
        },
        ProcessSnapshotItem {
            pid: 101,
            exe_name: "steam.exe".to_string(),
        },
    ];
    assert!(!is_anti_cheat_running(&normal_processes));

    for &ac_name in ANTI_CHEAT_PROCESSES {
        let ac_processes = vec![ProcessSnapshotItem {
            pid: 200,
            exe_name: ac_name.to_string(),
        }];
        assert!(is_anti_cheat_running(&ac_processes));
    }
}

#[test]
fn test_is_game_exe_running_externally() {
    let mut own_pids = HashSet::new();
    own_pids.insert(1000);

    let processes = vec![
        ProcessSnapshotItem {
            pid: 1000,
            exe_name: "cs2.exe".to_string(),
        },
        ProcessSnapshotItem {
            pid: 2000,
            exe_name: "dota2.exe".to_string(),
        },
    ];

    assert!(!is_game_exe_running_externally("cs2.exe", &own_pids, &processes));
    assert!(is_game_exe_running_externally("dota2.exe", &own_pids, &processes));
    assert!(!is_game_exe_running_externally("tf2.exe", &own_pids, &processes));
}
