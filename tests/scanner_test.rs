#[test]
fn test_all_games_count_and_playtime() {
    let steam_path = "c:\\program files (x86)\\steam";
    let games = SteamProcessIdler::steam_scanner::scan_games(Some(steam_path));
    println!("Total games scanned: {}", games.len());
    assert!(games.len() >= 279);
    assert!(games.iter().any(|g| g.playtime_minutes > 0));

    if let Some(cs2) = games.iter().find(|g| g.app_id == 730) {
        println!("CS2 Playtime: {} minutes ({})", cs2.playtime_minutes, cs2.playtime_formatted());
        println!("CS2 Last Played: {}", cs2.last_played_formatted());
        assert!(cs2.playtime_minutes > 6000);
        assert!(!cs2.last_played_formatted().starts_with("1970"));
    }
}
