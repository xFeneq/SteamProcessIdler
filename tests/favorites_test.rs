use SteamProcessIdler::models::SteamGame;

#[test]
fn test_favorites_sorting() {
    let game1 = SteamGame::new(730, "Counter-Strike 2".to_string());
    let game2 = SteamGame::new(570, "Dota 2".to_string());
    let game3 = SteamGame::new(440, "Team Fortress 2".to_string());

    let favorites = vec![440];

    let mut games = vec![game1, game2, game3];
    games.sort_by(|a, b| {
        let fav_a = favorites.contains(&a.app_id);
        let fav_b = favorites.contains(&b.app_id);
        let cmp = fav_b.cmp(&fav_a);
        if cmp == std::cmp::Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            cmp
        }
    });

    assert_eq!(games[0].app_id, 440);
    assert_eq!(games[1].app_id, 730);
    assert_eq!(games[2].app_id, 570);
}
