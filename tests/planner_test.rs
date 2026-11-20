use SteamProcessIdler::models::SteamGame;
use SteamProcessIdler::planner::{random_range_u32, PlannerAction, SessionQueuePlanner};
use SteamProcessIdler::settings::{AppSettings, TimeUnit};
use std::time::Duration;

#[test]
fn test_planner_queue_operations() {
    let mut planner = SessionQueuePlanner::new();
    assert_eq!(planner.queue.len(), 0);

    let g1 = SteamGame::new(10, "Game 1".to_string());
    let g2 = SteamGame::new(20, "Game 2".to_string());
    let g3 = SteamGame::new(30, "Game 3".to_string());

    assert!(planner.add_game(g1.clone()));
    assert!(!planner.add_game(g1.clone()));
    assert_eq!(planner.queue.len(), 1);

    assert!(planner.add_game(g2.clone()));
    assert!(planner.add_game(g3.clone()));
    assert_eq!(planner.queue.len(), 3);

    planner.move_up(2);
    assert_eq!(planner.queue[1].app_id, 30);
    assert_eq!(planner.queue[2].app_id, 20);

    planner.move_down(0);
    assert_eq!(planner.queue[0].app_id, 30);
    assert_eq!(planner.queue[1].app_id, 10);

    let removed = planner.remove_at(1);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().app_id, 10);
    assert_eq!(planner.queue.len(), 2);

    planner.clear();
    assert_eq!(planner.queue.len(), 0);
}

#[test]
fn test_planner_fill_safe_dc_games() {
    let mut planner = SessionQueuePlanner::new();

    let mut safe_game = SteamGame::new(100, "Safe DC".to_string());
    safe_game.is_discord_detectable = true;
    safe_game.is_never_played = true;
    safe_game.is_vac_protected = false;
    safe_game.is_third_party_anti_cheat = false;

    let mut vac_game = SteamGame::new(200, "VAC Game".to_string());
    vac_game.is_discord_detectable = true;
    vac_game.is_never_played = true;
    vac_game.is_vac_protected = true;

    let mut non_dc_game = SteamGame::new(300, "Non DC".to_string());
    non_dc_game.is_discord_detectable = false;

    let available = vec![safe_game, vac_game, non_dc_game];
    let count = planner.fill_safe_dc_games(&available);
    assert_eq!(count, 1);
    assert_eq!(planner.queue.len(), 1);
    assert_eq!(planner.queue[0].app_id, 100);
}

#[test]
fn test_random_range() {
    for _ in 0..50 {
        let r = random_range_u32(16, 22);
        assert!((16..=22).contains(&r));
    }
}

#[test]
fn test_time_units_and_durations() {
    assert_eq!(TimeUnit::Ms.to_millis(500), 500);
    assert_eq!(TimeUnit::Sec.to_millis(10), 10_000);
    assert_eq!(TimeUnit::Min.to_millis(2), 120_000);
    assert_eq!(TimeUnit::Hour.to_millis(1), 3_600_000);

    let mut settings = AppSettings::default();
    settings.planner_duration_unit = TimeUnit::Ms;
    settings.planner_duration_value = 250;
    let dur = SessionQueuePlanner::calculate_game_duration(&settings);
    assert_eq!(dur, Duration::from_millis(250));

    settings.planner_duration_value = 50;
    let min_dur = SessionQueuePlanner::calculate_game_duration(&settings);
    assert_eq!(min_dur, Duration::from_millis(100));
}

#[test]
fn test_planner_lifecycle() {
    let mut planner = SessionQueuePlanner::new();
    let mut settings = AppSettings::default();
    settings.planner_duration_unit = TimeUnit::Sec;
    settings.planner_duration_value = 2;
    settings.planner_cooldown_unit = TimeUnit::Sec;
    settings.planner_cooldown_value = 1;

    let g1 = SteamGame::new(1, "First".to_string());
    let g2 = SteamGame::new(2, "Second".to_string());
    planner.add_game(g1);
    planner.add_game(g2);

    let action = planner.start(&settings);
    match action {
        PlannerAction::StartGame(game, dur) => {
            assert_eq!(game.app_id, 1);
            assert_eq!(dur, Duration::from_secs(2));
        }
        _ => panic!("Expected StartGame"),
    }

    assert!(planner.is_active());
    assert!(planner.status_text().contains("Game 1/2"));

    let stop_action = planner.stop();
    match stop_action {
        PlannerAction::StopGame(app_id) => {
            assert_eq!(app_id, 1);
        }
        _ => panic!("Expected StopGame"),
    }
    assert!(!planner.is_active());
}
