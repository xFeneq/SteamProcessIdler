use crate::models::SteamGame;
use crate::settings::AppSettings;
use std::time::{Duration, Instant};

pub fn random_range_u64(min: u64, max: u64) -> u64 {
    if min >= max {
        return min;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1234567);
    let mut x = (nanos as u64) ^ 0x9E3779B97F4A7C15;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    let r = x.wrapping_mul(0x2545F4914F6CDD1D);
    min + (r % (max - min + 1))
}

pub fn random_range_u32(min: u32, max: u32) -> u32 {
    random_range_u64(min as u64, max as u64) as u32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannerState {
    Idle,
    RunningGame {
        index: usize,
        app_id: u32,
        game_name: String,
        target_duration: Duration,
        started_at: Instant,
    },
    Cooldown {
        next_index: usize,
        next_app_id: u32,
        next_game_name: String,
        target_cooldown: Duration,
        started_at: Instant,
    },
    Completed,
}

#[derive(Clone, Debug)]
pub enum PlannerAction {
    None,
    StartGame(SteamGame, Duration),
    StopGame(u32),
    QueueFinished,
}

pub struct SessionQueuePlanner {
    pub queue: Vec<SteamGame>,
    pub state: PlannerState,
}

impl Default for SessionQueuePlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionQueuePlanner {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            state: PlannerState::Idle,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, PlannerState::RunningGame { .. } | PlannerState::Cooldown { .. })
    }

    pub fn current_running_game(&self) -> Option<&SteamGame> {
        if let PlannerState::RunningGame { index, .. } = self.state {
            self.queue.get(index)
        } else {
            None
        }
    }

    pub fn add_game(&mut self, game: SteamGame) -> bool {
        if self.queue.iter().any(|g| g.app_id == game.app_id) {
            return false;
        }
        self.queue.push(game);
        true
    }

    pub fn remove_at(&mut self, index: usize) -> Option<SteamGame> {
        if index < self.queue.len() {
            if let PlannerState::RunningGame { index: cur_idx, .. } = self.state {
                if cur_idx == index {
                    return None;
                }
            }
            Some(self.queue.remove(index))
        } else {
            None
        }
    }

    pub fn move_up(&mut self, index: usize) {
        if index > 0 && index < self.queue.len() && !self.is_active() {
            self.queue.swap(index, index - 1);
        }
    }

    pub fn move_down(&mut self, index: usize) {
        if index + 1 < self.queue.len() && !self.is_active() {
            self.queue.swap(index, index + 1);
        }
    }

    pub fn clear(&mut self) {
        if !self.is_active() {
            self.queue.clear();
            self.state = PlannerState::Idle;
        }
    }

    pub fn fill_safe_dc_games(&mut self, available_games: &[SteamGame]) -> usize {
        let mut added = 0;
        for game in available_games {
            if game.is_discord_detectable
                && (game.is_never_played || game.playtime_minutes == 0)
                && !game.is_vac_protected
                && !game.is_third_party_anti_cheat
                && !self.queue.iter().any(|q| q.app_id == game.app_id)
            {
                self.queue.push(game.clone());
                added += 1;
            }
        }
        added
    }

    pub fn calculate_game_duration(settings: &AppSettings) -> Duration {
        let millis = if settings.planner_use_random_duration {
            let min_ms = settings
                .planner_duration_unit
                .to_millis(settings.planner_min_duration_value)
                .max(100);
            let max_ms = settings
                .planner_duration_unit
                .to_millis(settings.planner_max_duration_value)
                .max(min_ms);
            random_range_u64(min_ms, max_ms)
        } else {
            settings
                .planner_duration_unit
                .to_millis(settings.planner_duration_value)
                .max(100)
        };

        let buffer_ms = if millis >= 900_000 { 45_000 } else { 0 };
        Duration::from_millis(millis + buffer_ms)
    }

    pub fn calculate_cooldown(settings: &AppSettings) -> Duration {
        let millis = if settings.planner_use_random_cooldown {
            let min_ms = settings
                .planner_cooldown_unit
                .to_millis(settings.planner_min_cooldown_value)
                .max(100);
            let max_ms = settings
                .planner_cooldown_unit
                .to_millis(settings.planner_max_cooldown_value)
                .max(min_ms);
            random_range_u64(min_ms, max_ms)
        } else {
            settings
                .planner_cooldown_unit
                .to_millis(settings.planner_cooldown_value)
                .max(100)
        };
        Duration::from_millis(millis)
    }

    pub fn start(&mut self, settings: &AppSettings) -> PlannerAction {
        if self.queue.is_empty() {
            return PlannerAction::None;
        }

        let first_game = self.queue[0].clone();
        let duration = Self::calculate_game_duration(settings);

        self.state = PlannerState::RunningGame {
            index: 0,
            app_id: first_game.app_id,
            game_name: first_game.name.clone(),
            target_duration: duration,
            started_at: Instant::now(),
        };

        PlannerAction::StartGame(first_game, duration)
    }

    pub fn stop(&mut self) -> PlannerAction {
        match self.state {
            PlannerState::RunningGame { app_id, .. } => {
                self.state = PlannerState::Idle;
                PlannerAction::StopGame(app_id)
            }
            PlannerState::Cooldown { .. } => {
                self.state = PlannerState::Idle;
                PlannerAction::None
            }
            _ => {
                self.state = PlannerState::Idle;
                PlannerAction::None
            }
        }
    }

    pub fn tick(&mut self, settings: &AppSettings) -> PlannerAction {
        match self.state {
            PlannerState::RunningGame {
                index,
                app_id,
                target_duration,
                started_at,
                ..
            } => {
                if started_at.elapsed() >= target_duration {
                    let next_index = index + 1;
                    if next_index < self.queue.len() {
                        let next_game = self.queue[next_index].clone();
                        let cooldown = Self::calculate_cooldown(settings);
                        self.state = PlannerState::Cooldown {
                            next_index,
                            next_app_id: next_game.app_id,
                            next_game_name: next_game.name,
                            target_cooldown: cooldown,
                            started_at: Instant::now(),
                        };
                        PlannerAction::StopGame(app_id)
                    } else {
                        self.state = PlannerState::Completed;
                        PlannerAction::StopGame(app_id)
                    }
                } else {
                    PlannerAction::None
                }
            }
            PlannerState::Cooldown {
                next_index,
                target_cooldown,
                started_at,
                ..
            } => {
                if started_at.elapsed() >= target_cooldown {
                    if next_index < self.queue.len() {
                        let next_game = self.queue[next_index].clone();
                        let duration = Self::calculate_game_duration(settings);
                        self.state = PlannerState::RunningGame {
                            index: next_index,
                            app_id: next_game.app_id,
                            game_name: next_game.name.clone(),
                            target_duration: duration,
                            started_at: Instant::now(),
                        };
                        PlannerAction::StartGame(next_game, duration)
                    } else {
                        self.state = PlannerState::Completed;
                        PlannerAction::QueueFinished
                    }
                } else {
                    PlannerAction::None
                }
            }
            _ => PlannerAction::None,
        }
    }

    pub fn status_text(&self) -> String {
        match &self.state {
            PlannerState::Idle => {
                if self.queue.is_empty() {
                    "Queue empty".to_string()
                } else {
                    format!("Ready ({} games in queue)", self.queue.len())
                }
            }
            PlannerState::RunningGame {
                index,
                game_name,
                target_duration,
                started_at,
                ..
            } => {
                let elapsed = started_at.elapsed();
                let rem = target_duration.saturating_sub(elapsed);
                let rem_secs = rem.as_secs();
                let time_str = if rem_secs >= 60 {
                    format!("{:02}:{:02}", rem_secs / 60, rem_secs % 60)
                } else if rem_secs > 0 {
                    format!("{}s", rem_secs)
                } else {
                    format!("{}ms", rem.as_millis())
                };
                format!(
                    "Game {}/{}: {} • Stop in {}",
                    index + 1,
                    self.queue.len(),
                    game_name,
                    time_str
                )
            }
            PlannerState::Cooldown {
                next_index,
                next_game_name,
                target_cooldown,
                started_at,
                ..
            } => {
                let elapsed = started_at.elapsed();
                let rem = target_cooldown.saturating_sub(elapsed);
                let rem_secs = rem.as_secs();
                let time_str = if rem_secs > 0 {
                    format!("{}s", rem_secs)
                } else {
                    format!("{}ms", rem.as_millis())
                };
                format!(
                    "Cooldown: Next {}/{} ({}) in {}",
                    next_index + 1,
                    self.queue.len(),
                    next_game_name,
                    time_str
                )
            }
            PlannerState::Completed => {
                format!("Queue completed ({} games finished)", self.queue.len())
            }
        }
    }

    pub fn progress_ratio(&self) -> f32 {
        if self.queue.is_empty() {
            return 0.0;
        }

        let total = self.queue.len() as f32;

        match &self.state {
            PlannerState::Idle => 0.0,
            PlannerState::Completed => 1.0,
            PlannerState::RunningGame {
                index,
                target_duration,
                started_at,
                ..
            } => {
                let dur = target_duration.as_millis().max(1) as f32;
                let elapsed = (started_at.elapsed().as_millis() as f32).min(dur);
                let item_prog = elapsed / dur;
                ((*index as f32) + item_prog) / total
            }
            PlannerState::Cooldown {
                next_index,
                target_cooldown,
                started_at,
                ..
            } => {
                let dur = target_cooldown.as_millis().max(1) as f32;
                let elapsed = (started_at.elapsed().as_millis() as f32).min(dur);
                let cool_prog = elapsed / dur;
                let base = *next_index as f32;
                (base - 0.05 + (cool_prog * 0.05)).max(0.0) / total
            }
        }
    }
}
