use SteamProcessIdler::history::{
    format_duration_human, format_timestamp_human, GameHistoryStats, HistoryStore,
    SessionHistoryRecord,
};

#[test]
fn test_format_duration_human() {
    assert_eq!(format_duration_human(0), "0s");
    assert_eq!(format_duration_human(45), "45s");
    assert_eq!(format_duration_human(60), "1m");
    assert_eq!(format_duration_human(125), "2m 5s");
    assert_eq!(format_duration_human(3600), "1.0 hrs");
    assert_eq!(format_duration_human(7200), "2.0 hrs");
}

#[test]
fn test_format_timestamp_human() {
    assert_eq!(format_timestamp_human(0), "Never");
    let ts = 1700000000;
    let formatted = format_timestamp_human(ts);
    assert!(formatted.contains("-"));
    assert!(formatted.contains(":"));
}

#[test]
fn test_history_store_serialization() {
    let mut store = HistoryStore::default();
    store.games.insert(
        730,
        GameHistoryStats {
            app_id: 730,
            game_name: "Counter-Strike 2".to_string(),
            total_idled_seconds: 3600,
            session_count: 2,
            last_session_unix: 1700000000,
        },
    );
    store.recent_sessions.push(SessionHistoryRecord {
        app_id: 730,
        game_name: "Counter-Strike 2".to_string(),
        duration_seconds: 1800,
        timestamp_unix: 1700000000,
    });

    let json = serde_json::to_string(&store).expect("Failed to serialize history store");
    let deserialized: HistoryStore =
        serde_json::from_str(&json).expect("Failed to deserialize history store");

    assert_eq!(deserialized.games.len(), 1);
    assert_eq!(deserialized.games.get(&730).unwrap().total_idled_seconds, 3600);
    assert_eq!(deserialized.recent_sessions.len(), 1);
}
