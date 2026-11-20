#[test]
fn test_discord_detectable_enrichment() {
    let apps = SteamProcessIdler::discord_detectable::fetch_or_load_detectable(false);
    println!("Total Discord detectable apps: {}", apps.len());
    assert!(!apps.is_empty());

    let steam_path = "c:\\program files (x86)\\steam";
    let mut games = SteamProcessIdler::steam_scanner::scan_games(Some(steam_path));
    SteamProcessIdler::discord_detectable::enrich_games_with_discord_data(&mut games, &apps);

    let detectable_count = games.iter().filter(|g| g.is_discord_detectable).count();
    println!("Games matched with Discord: {} / {}", detectable_count, games.len());
    assert!(detectable_count > 100);
}
