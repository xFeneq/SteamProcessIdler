use crate::discord_detectable::{
    enrich_games_with_discord_data, fetch_or_load_detectable, DiscordDetectableApplication,
};
use crate::history::{
    clear_history_file, format_duration_human, format_timestamp_human, load_history,
    record_session_finish, HistoryStore,
};
use crate::models::{ActiveSession, SteamGame};
use crate::overrides::{apply_overrides, remove_override, save_or_update_override};
use crate::planner::{random_range_u64, PlannerAction, SessionQueuePlanner};
use crate::process_guard::{
    get_system_processes, is_anti_cheat_running, is_game_exe_running_externally,
    ProcessSnapshotItem,
};
use crate::process_manager::{start_game_session, stop_all_sessions, stop_session};
use crate::settings::{load_settings, save_settings, AppSettings, SortOrder, TimeUnit, ViewMode};
use crate::steam_scanner::{
    download_missing_banner, download_missing_icon, get_steam_installation_path, scan_games,
};
use crate::updater::{check_for_updates, install_update, UpdateInfo};
use egui::{
    pos2, vec2, Align, Align2, Color32, ColorImage, CornerRadius, FontId, Frame, Layout, Margin,
    Order, RichText, ScrollArea, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RightPanelTab {
    #[default]
    Active,
    Queue,
}

pub struct SteamProcessIdlerApp {
    games: Vec<SteamGame>,
    custom_games: Vec<SteamGame>,
    active_sessions: Vec<ActiveSession>,
    settings: AppSettings,
    search_query: String,
    filter_favorites: bool,
    filter_discord: bool,
    filter_never_played: bool,
    filter_hide_vac: bool,
    filter_hide_ac: bool,
    status_message: Option<(String, std::time::Instant)>,
    steam_path: Option<String>,

    icon_textures: HashMap<u32, TextureHandle>,
    banner_textures: HashMap<u32, TextureHandle>,
    app_icon_texture: Option<TextureHandle>,

    discord_detectable_cache: Vec<DiscordDetectableApplication>,
    discord_rx: Receiver<Vec<DiscordDetectableApplication>>,
    discord_tx: Sender<Vec<DiscordDetectableApplication>>,

    update_info: Option<UpdateInfo>,
    is_checking_updates: bool,
    update_rx: Receiver<Option<UpdateInfo>>,
    update_tx: Sender<Option<UpdateInfo>>,

    show_settings_modal: bool,
    show_edit_game_modal: bool,
    show_details_modal: bool,
    show_dc_info_modal: bool,
    show_vac_info_modal: bool,
    show_ac_info_modal: bool,
    show_history_modal: bool,
    show_duplicate_process_modal: Option<String>,
    show_vac_shield_blocked_modal: Option<String>,
    show_vac_shield_disable_warning_modal: bool,

    editing_game: Option<SteamGame>,
    is_editing_custom: bool,
    edit_exe_path: String,
    edit_game_name: String,
    edit_app_id_str: String,

    selected_details_game: Option<SteamGame>,
    pending_vac_notice_game: Option<SteamGame>,
    pending_ac_notice_game: Option<SteamGame>,

    right_panel_tab: RightPanelTab,
    planner: SessionQueuePlanner,

    history_store: HistoryStore,
    last_process_scan: std::time::Instant,
    cached_system_processes: Vec<ProcessSnapshotItem>,
    is_vac_or_ac_active_in_system: bool,
}

impl SteamProcessIdlerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (discord_tx, discord_rx) = channel();
        let (update_tx, update_rx) = channel();

        let settings = load_settings();
        let steam_path = get_steam_installation_path();
        let history_store = load_history();

        let mut app = Self {
            games: Vec::new(),
            custom_games: Vec::new(),
            active_sessions: Vec::new(),
            settings,
            search_query: String::new(),
            filter_favorites: false,
            filter_discord: false,
            filter_never_played: false,
            filter_hide_vac: false,
            filter_hide_ac: false,
            status_message: None,
            steam_path,
            icon_textures: HashMap::new(),
            banner_textures: HashMap::new(),
            app_icon_texture: None,
            discord_detectable_cache: Vec::new(),
            discord_rx,
            discord_tx,
            update_info: None,
            is_checking_updates: false,
            update_rx,
            update_tx,
            show_settings_modal: false,
            show_edit_game_modal: false,
            show_details_modal: false,
            show_dc_info_modal: false,
            show_vac_info_modal: false,
            show_ac_info_modal: false,
            show_history_modal: false,
            show_duplicate_process_modal: None,
            show_vac_shield_blocked_modal: None,
            show_vac_shield_disable_warning_modal: false,
            editing_game: None,
            is_editing_custom: false,
            edit_exe_path: String::new(),
            edit_game_name: String::new(),
            edit_app_id_str: String::new(),
            selected_details_game: None,
            pending_vac_notice_game: None,
            pending_ac_notice_game: None,
            right_panel_tab: RightPanelTab::Active,
            planner: SessionQueuePlanner::new(),
            history_store,
            last_process_scan: std::time::Instant::now() - Duration::from_secs(10),
            cached_system_processes: Vec::new(),
            is_vac_or_ac_active_in_system: false,
        };

        app.apply_steam_theme(&cc.egui_ctx);
        app.app_icon_texture = load_embedded_app_icon(&cc.egui_ctx);

        app.refresh_library();

        let tx = app.discord_tx.clone();
        thread::spawn(move || {
            let list = fetch_or_load_detectable(false);
            let _ = tx.send(list);
        });

        if app.settings.check_updates_on_startup {
            app.check_updates_async();
        }

        app
    }

    fn has_active_modal(&self) -> bool {
        self.show_settings_modal
            || self.show_edit_game_modal
            || self.show_details_modal
            || self.show_dc_info_modal
            || self.show_vac_info_modal
            || self.show_ac_info_modal
            || self.show_history_modal
            || self.show_duplicate_process_modal.is_some()
            || self.show_vac_shield_blocked_modal.is_some()
            || self.show_vac_shield_disable_warning_modal
            || self.pending_vac_notice_game.is_some()
            || self.pending_ac_notice_game.is_some()
    }

    fn apply_steam_theme(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        let mut visuals = egui::Visuals::dark();

        visuals.window_fill = Color32::from_rgb(0x17, 0x1A, 0x21);
        visuals.panel_fill = Color32::from_rgb(0x17, 0x1A, 0x21);

        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(0x1B, 0x28, 0x38);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E));
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);

        visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x21, 0x2C, 0x3D);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E));
        visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0xE5, 0xEC, 0xEE));

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2A, 0x38, 0x4D);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x66, 0xC0, 0xF4));
        visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

        visuals.widgets.active.bg_fill = Color32::from_rgb(0x32, 0x48, 0x65);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x66, 0xC0, 0xF4));
        visuals.widgets.active.corner_radius = CornerRadius::same(6);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

        visuals.selection.bg_fill = Color32::from_rgb(0x2A, 0x47, 0x5E);
        visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x66, 0xC0, 0xF4));

        style.visuals = visuals;
        ctx.set_style(style);
    }

    fn refresh_library(&mut self) {
        self.games = scan_games(self.steam_path.as_deref());

        self.apply_overrides_to_all_games();

        if !self.discord_detectable_cache.is_empty() {
            enrich_games_with_discord_data(&mut self.games, &self.discord_detectable_cache);
            enrich_games_with_discord_data(&mut self.custom_games, &self.discord_detectable_cache);
        }

        for session in &self.active_sessions {
            if let Some(game) = self.games.iter_mut().find(|g| g.app_id == session.app_id) {
                game.is_running = true;
            }
            if let Some(game) = self.custom_games.iter_mut().find(|g| g.app_id == session.app_id) {
                game.is_running = true;
            }
        }
    }

    fn check_updates_async(&mut self) {
        self.is_checking_updates = true;
        let tx = self.update_tx.clone();
        thread::spawn(move || {
            let info = check_for_updates();
            let _ = tx.send(info);
        });
    }

    fn apply_overrides_to_all_games(&mut self) {
        apply_overrides(&mut self.games, &mut self.custom_games);
    }

    fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), std::time::Instant::now()));
    }

    fn matches_filters(&self, game: &SteamGame) -> bool {
        if !self.search_query.is_empty() {
            let q = self.search_query.to_lowercase();
            let matches_name = game.name.to_lowercase().contains(&q);
            let matches_id = game.app_id.to_string().contains(&q);
            let matches_exe = game.executable_relative_path.to_lowercase().contains(&q);
            if !matches_name && !matches_id && !matches_exe {
                return false;
            }
        }

        if self.filter_favorites && !self.settings.favorite_app_ids.contains(&game.app_id) {
            return false;
        }

        if self.filter_discord && !game.is_discord_detectable {
            return false;
        }

        if self.filter_never_played && (!game.is_never_played && game.playtime_minutes > 0) {
            return false;
        }

        if self.filter_hide_vac && game.is_vac_protected {
            return false;
        }

        if self.filter_hide_ac && game.is_third_party_anti_cheat {
            return false;
        }

        true
    }

    fn can_launch_game(&mut self, game: &SteamGame) -> bool {
        let own_pids: HashSet<u32> = self
            .active_sessions
            .iter()
            .map(|s| s.pid)
            .chain(std::iter::once(std::process::id()))
            .collect();

        if is_game_exe_running_externally(
            &game.executable_relative_path,
            &own_pids,
            &self.cached_system_processes,
        ) {
            self.show_duplicate_process_modal = Some(game.name.clone());
            return false;
        }

        if self.settings.vac_shield_enabled && (game.is_vac_protected || game.is_third_party_anti_cheat) {
            if self.is_vac_or_ac_active_in_system {
                self.show_vac_shield_blocked_modal = Some(game.name.clone());
                return false;
            }
        }

        true
    }

    fn try_launch_game(&mut self, game: SteamGame) {
        if !self.can_launch_game(&game) {
            return;
        }

        if self.settings.show_anti_cheat_notices {
            if game.is_vac_protected {
                self.pending_vac_notice_game = Some(game);
                return;
            }
            if game.is_third_party_anti_cheat {
                self.pending_ac_notice_game = Some(game);
                return;
            }
        }

        let scheduled_duration = if self.settings.enable_auto_stop {
            let millis = if self.settings.auto_stop_use_random {
                let min_ms = self
                    .settings
                    .auto_stop_unit
                    .to_millis(self.settings.auto_stop_min_value)
                    .max(100);
                let max_ms = self
                    .settings
                    .auto_stop_unit
                    .to_millis(self.settings.auto_stop_max_value)
                    .max(min_ms);
                random_range_u64(min_ms, max_ms)
            } else {
                self.settings
                    .auto_stop_unit
                    .to_millis(self.settings.auto_stop_value)
                    .max(100)
            };
            let buffer_ms = if millis >= 900_000 { 45_000 } else { 0 };
            Some(Duration::from_millis(millis + buffer_ms))
        } else {
            None
        };

        self.execute_launch_game_with_duration(&game, scheduled_duration);
    }

    fn execute_launch_game_with_duration(&mut self, game: &SteamGame, scheduled_duration: Option<Duration>) {
        if self.active_sessions.iter().any(|s| s.app_id == game.app_id) {
            return;
        }

        if !self.can_launch_game(game) {
            return;
        }

        match start_game_session(game, self.steam_path.as_deref()) {
            Ok(mut session) => {
                if let Some(dur) = scheduled_duration {
                    session.scheduled_stop = Some(std::time::Instant::now() + dur);
                    session.scheduled_duration_secs = Some(dur.as_secs().max(1));
                }

                self.active_sessions.push(session);
                if let Some(g) = self.games.iter_mut().find(|g| g.app_id == game.app_id) {
                    g.is_running = true;
                }
                if let Some(g) = self.custom_games.iter_mut().find(|g| g.app_id == game.app_id) {
                    g.is_running = true;
                }
                self.set_status(&format!("Launched simulation for {}", game.name));
            }
            Err(e) => {
                self.set_status(&format!("Failed to launch {}: {}", game.name, e));
            }
        }
    }

    fn stop_game(&mut self, app_id: u32) {
        if let Some(idx) = self.active_sessions.iter().position(|s| s.app_id == app_id) {
            let mut session = self.active_sessions.remove(idx);
            let name = session.game_name.clone();
            let duration = session.start_time.elapsed().as_secs();
            stop_session(&mut session);

            record_session_finish(self.settings.record_session_history, app_id, &name, duration);
            if self.settings.record_session_history {
                self.history_store = load_history();
            }

            if let Some(g) = self.games.iter_mut().find(|g| g.app_id == app_id) {
                g.is_running = false;
            }
            if let Some(g) = self.custom_games.iter_mut().find(|g| g.app_id == app_id) {
                g.is_running = false;
            }
            self.set_status(&format!("Stopped simulation for {}", name));
        }
    }

    fn stop_all(&mut self) {
        for s in &self.active_sessions {
            let duration = s.start_time.elapsed().as_secs();
            record_session_finish(self.settings.record_session_history, s.app_id, &s.game_name, duration);
        }
        if self.settings.record_session_history {
            self.history_store = load_history();
        }

        stop_all_sessions(&mut self.active_sessions);
        for g in &mut self.games {
            g.is_running = false;
        }
        for g in &mut self.custom_games {
            g.is_running = false;
        }
        self.set_status("Stopped all active sessions.");
    }

    fn get_game_icon_texture(&mut self, ctx: &egui::Context, app_id: u32, icon_path: Option<&str>) -> Option<TextureHandle> {
        if let Some(tex) = self.icon_textures.get(&app_id) {
            return Some(tex.clone());
        }

        if let Some(path_str) = icon_path {
            let path = Path::new(path_str);
            if path.exists() {
                if let Some(color_img) = load_image_to_color_image(path) {
                    let tex = ctx.load_texture(
                        format!("game_icon_{}", app_id),
                        color_img,
                        TextureOptions::LINEAR,
                    );
                    self.icon_textures.insert(app_id, tex.clone());
                    return Some(tex);
                }
            }
        }

        if let Some(game) = self.games.iter().find(|g| g.app_id == app_id) {
            if let Some(ref hash) = game.client_icon_hash {
                let h = hash.clone();
                let aid = app_id;
                thread::spawn(move || {
                    let _ = download_missing_icon(aid, Some(&h));
                });
            }
        }

        None
    }

    fn get_game_banner_texture(
        &mut self,
        ctx: &egui::Context,
        app_id: u32,
        banner_path: Option<&str>,
        icon_path: Option<&str>,
    ) -> Option<TextureHandle> {
        if let Some(tex) = self.banner_textures.get(&app_id) {
            return Some(tex.clone());
        }

        if let Some(path_str) = banner_path {
            let path = Path::new(path_str);
            if path.exists() {
                if let Some(color_img) = load_image_to_color_image(path) {
                    let tex = ctx.load_texture(
                        format!("game_banner_{}", app_id),
                        color_img,
                        TextureOptions::LINEAR,
                    );
                    self.banner_textures.insert(app_id, tex.clone());
                    return Some(tex);
                }
            }
        }

        if let Some(icon_path_str) = icon_path {
            let path = Path::new(icon_path_str);
            if path.exists() {
                if let Some(color_img) = generate_synthetic_banner_image(path) {
                    let tex = ctx.load_texture(
                        format!("synthetic_banner_{}", app_id),
                        color_img,
                        TextureOptions::LINEAR,
                    );
                    self.banner_textures.insert(app_id, tex.clone());
                    return Some(tex);
                }
            }
        }

        let aid = app_id;
        thread::spawn(move || {
            let _ = download_missing_banner(aid);
        });

        None
    }
}

impl eframe::App for SteamProcessIdlerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));

        if self.last_process_scan.elapsed() >= Duration::from_millis(1500) {
            self.last_process_scan = std::time::Instant::now();
            self.cached_system_processes = get_system_processes();

            let own_pids: HashSet<u32> = self
                .active_sessions
                .iter()
                .map(|s| s.pid)
                .chain(std::iter::once(std::process::id()))
                .collect();

            let ac_running = is_anti_cheat_running(&self.cached_system_processes);
            let real_vac_running = self.games.iter().chain(self.custom_games.iter()).any(|g| {
                (g.is_vac_protected || g.is_third_party_anti_cheat)
                    && is_game_exe_running_externally(&g.executable_relative_path, &own_pids, &self.cached_system_processes)
            });
            self.is_vac_or_ac_active_in_system = ac_running || real_vac_running;

            if self.settings.vac_shield_enabled && self.is_vac_or_ac_active_in_system {
                let mut stopped_protected = Vec::new();
                for s in &self.active_sessions {
                    if let Some(g) = self.games.iter().chain(self.custom_games.iter()).find(|g| g.app_id == s.app_id) {
                        if g.is_vac_protected || g.is_third_party_anti_cheat {
                            stopped_protected.push((s.app_id, s.game_name.clone()));
                        }
                    }
                }
                for (app_id, name) in stopped_protected {
                    self.stop_game(app_id);
                    self.set_status(&format!("VAC/AC Shield stopped {} for account safety.", name));
                }

                if self.planner.is_active() {
                    if let Some(cg) = self.planner.current_running_game() {
                        if cg.is_vac_protected || cg.is_third_party_anti_cheat {
                            let action = self.planner.stop();
                            if let PlannerAction::StopGame(id) = action {
                                self.stop_game(id);
                            }
                            self.set_status("Session queue paused: VAC or Anti-Cheat detected in system.");
                        }
                    }
                }
            }

            let mut duplicate_running = Vec::new();
            for s in &self.active_sessions {
                if let Some(g) = self.games.iter().chain(self.custom_games.iter()).find(|g| g.app_id == s.app_id) {
                    if is_game_exe_running_externally(&g.executable_relative_path, &own_pids, &self.cached_system_processes) {
                        duplicate_running.push((s.app_id, s.game_name.clone()));
                    }
                }
            }
            for (app_id, name) in duplicate_running {
                self.stop_game(app_id);
                self.set_status(&format!("Stopped {} idler because real game was launched.", name));
            }

            if self.settings.stop_idling_on_steam_game_start && !self.active_sessions.is_empty() {
                let any_real_running = self.games.iter().chain(self.custom_games.iter()).any(|g| {
                    is_game_exe_running_externally(&g.executable_relative_path, &own_pids, &self.cached_system_processes)
                });
                if any_real_running {
                    self.stop_all();
                    self.planner.stop();
                    self.set_status("Stopped all idler sessions: real Steam game started.");
                }
            }
        }

        if let Ok(list) = self.discord_rx.try_recv() {
            self.discord_detectable_cache = list;
            enrich_games_with_discord_data(&mut self.games, &self.discord_detectable_cache);
            enrich_games_with_discord_data(&mut self.custom_games, &self.discord_detectable_cache);
        }

        if let Ok(info) = self.update_rx.try_recv() {
            self.is_checking_updates = false;
            self.update_info = info;
        }

        let planner_action = self.planner.tick(&self.settings);
        match planner_action {
            PlannerAction::StartGame(game, duration) => {
                self.execute_launch_game_with_duration(&game, Some(duration));
            }
            PlannerAction::StopGame(app_id) => {
                self.stop_game(app_id);
            }
            PlannerAction::QueueFinished => {
                self.set_status("Session queue completed all scheduled games.");
            }
            PlannerAction::None => {}
        }

        let mut auto_stopped_ids = Vec::new();
        for session in &self.active_sessions {
            if let Some(stop_time) = session.scheduled_stop {
                if std::time::Instant::now() >= stop_time {
                    auto_stopped_ids.push(session.app_id);
                }
            }
        }
        for aid in auto_stopped_ids {
            self.stop_game(aid);
            self.set_status(&format!("Auto-stopped simulation for AppID {}.", aid));
        }

        let is_any_modal_open = self.has_active_modal();

        self.render_top_panel(ctx);

        if self.settings.right_panel_visible {
            egui::SidePanel::right("active_sessions_panel")
                .resizable(true)
                .default_width(self.settings.right_panel_width)
                .min_width(280.0)
                .max_width(520.0)
                .frame(
                    Frame::NONE
                        .fill(Color32::from_rgb(0x16, 0x20, 0x2D))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E))),
                )
                .show(ctx, |ui| {
                    let actual_width = ui.max_rect().width();
                    if (actual_width - self.settings.right_panel_width).abs() > 2.0 {
                        self.settings.right_panel_width = actual_width.clamp(280.0, 520.0);
                        save_settings(&self.settings);
                    }
                    self.render_right_panel_content(ui);
                });
        }

        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x17, 0x1A, 0x21))
                    .inner_margin(Margin::same(12)),
            )
            .show(ctx, |ui| {
                self.render_center_content(ctx, ui, is_any_modal_open);
            });

        if !self.settings.right_panel_visible {
            let screen_rect = ctx.screen_rect();
            let avail = ctx.available_rect();
            let handle_pos = pos2(screen_rect.max.x - 26.0, avail.min.y + 10.0);
            egui::Area::new(egui::Id::new("expand_panel_handle_area"))
                .order(Order::Middle)
                .fixed_pos(handle_pos)
                .show(ctx, |ui| {
                    let btn_res = render_panel_expand_handle(ui);
                    if btn_res.clicked() && !is_any_modal_open {
                        self.settings.right_panel_visible = true;
                        save_settings(&self.settings);
                    }
                });
        }

        if is_any_modal_open {
            let screen_rect = ctx.screen_rect();
            let mut backdrop_clicked = false;

            egui::Area::new(egui::Id::new("modal_backdrop"))
                .order(Order::Middle)
                .fixed_pos(screen_rect.min)
                .show(ctx, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(screen_rect.size(), egui::Sense::click());
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::ZERO,
                        Color32::from_black_alpha(155),
                    );
                    if response.clicked() {
                        backdrop_clicked = true;
                    }
                });

            if backdrop_clicked || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_settings_modal = false;
                self.show_edit_game_modal = false;
                self.show_details_modal = false;
                self.show_dc_info_modal = false;
                self.show_vac_info_modal = false;
                self.show_ac_info_modal = false;
                self.show_history_modal = false;
                self.show_duplicate_process_modal = None;
                self.show_vac_shield_blocked_modal = None;
                self.show_vac_shield_disable_warning_modal = false;
                self.selected_details_game = None;
                self.pending_vac_notice_game = None;
                self.pending_ac_notice_game = None;
            }
        }

        self.render_settings_modal(ctx);
        self.render_edit_game_modal(ctx);
        self.render_details_modal(ctx);
        self.render_vac_safety_modal(ctx);
        self.render_ac_safety_modal(ctx);
        self.render_dc_info_modal(ctx);
        self.render_vac_info_modal(ctx);
        self.render_ac_info_modal(ctx);
        self.render_history_modal(ctx);
        self.render_duplicate_process_modal(ctx);
        self.render_vac_shield_blocked_modal(ctx);
        self.render_vac_shield_disable_warning_modal(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.planner.stop();
        for s in &self.active_sessions {
            let dur = s.start_time.elapsed().as_secs();
            record_session_finish(self.settings.record_session_history, s.app_id, &s.game_name, dur);
        }
        stop_all_sessions(&mut self.active_sessions);
    }
}

impl SteamProcessIdlerApp {
    fn render_top_panel(&mut self, ctx: &egui::Context) {
        let is_any_modal_open = self.has_active_modal();

        egui::TopBottomPanel::top("top_bar")
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .inner_margin(Margin::symmetric(18, 12))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E))),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(ref icon) = self.app_icon_texture {
                        ui.image((icon.id(), vec2(34.0, 34.0)));
                        ui.add_space(8.0);
                    }

                    ui.label(
                        RichText::new("SteamProcessIdler")
                            .font(FontId::proportional(18.0))
                            .color(Color32::WHITE)
                            .strong(),
                    );

                    ui.add_space(8.0);
                    render_badge(
                        ui,
                        &format!("v{}", env!("CARGO_PKG_VERSION")),
                        Color32::from_rgb(0x2A, 0x47, 0x5E),
                    );

                    ui.add_space(8.0);
                    if !self.settings.vac_shield_enabled {
                        let shield_btn = render_shield_badge(
                            ui,
                            "VAC/AC Shield: Disabled",
                            Color32::from_rgb(0x99, 0x1B, 0x1B),
                            Color32::from_rgb(0xFC, 0xA5, 0xA5),
                            "VAC & Anti-Cheat Shield is disabled. Click to open Settings.",
                            !is_any_modal_open || self.show_settings_modal,
                        );
                        if shield_btn.clicked() && (!is_any_modal_open || self.show_settings_modal) {
                            self.show_settings_modal = true;
                        }
                    } else if self.is_vac_or_ac_active_in_system {
                        render_shield_badge(
                            ui,
                            "VAC/AC Shield: Locked (Game Active)",
                            Color32::from_rgb(0xC2, 0x41, 0x0C),
                            Color32::WHITE,
                            "A VAC or Anti-Cheat protected process is active in the system. Protected games are locked.",
                            false,
                        );
                    } else {
                        render_shield_badge(
                            ui,
                            "VAC/AC Shield: Active",
                            Color32::from_rgb(0x27, 0x7E, 0x46),
                            Color32::WHITE,
                            "VAC and Anti-Cheat protection is active.",
                            false,
                        );
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let gear_enabled = !is_any_modal_open || self.show_settings_modal;
                        if render_gear_button(ui, self.show_settings_modal, gear_enabled).clicked() {
                            if self.show_settings_modal {
                                self.show_settings_modal = false;
                            } else if !is_any_modal_open {
                                self.show_settings_modal = true;
                            }
                        }

                        ui.add_space(8.0);

                        let history_enabled = !is_any_modal_open || self.show_history_modal;
                        if render_top_bar_button(ui, "History", [75.0, 32.0], history_enabled).clicked() {
                            if self.show_history_modal {
                                self.show_history_modal = false;
                            } else if !is_any_modal_open {
                                self.show_history_modal = true;
                            }
                        }

                        ui.add_space(8.0);

                        let add_custom_enabled = !is_any_modal_open || (self.show_edit_game_modal && self.is_editing_custom);
                        if render_top_bar_button(ui, "+ Add Custom", [105.0, 32.0], add_custom_enabled).clicked() {
                            if self.show_edit_game_modal && self.is_editing_custom {
                                self.show_edit_game_modal = false;
                                self.editing_game = None;
                            } else if !is_any_modal_open {
                                let mut new_game = SteamGame::new(0, String::new());
                                new_game.executable_relative_path = "game.exe".to_string();
                                new_game.default_executable = "game.exe".to_string();
                                new_game.available_executables = vec!["game.exe".to_string()];
                                new_game.is_custom = true;
                                self.editing_game = Some(new_game);
                                self.is_editing_custom = true;
                                self.show_edit_game_modal = true;
                            }
                        }

                        ui.add_space(8.0);

                        let refresh_enabled = !is_any_modal_open;
                        if render_top_bar_button(ui, "Refresh Library", [115.0, 32.0], refresh_enabled).clicked() {
                            self.refresh_library();
                        }

                        if let Some(ref update) = self.update_info {
                            ui.add_space(8.0);
                            let update_enabled = !is_any_modal_open || self.show_settings_modal;
                            let update_btn = egui::Button::new(
                                RichText::new(format!("Update Available (v{})", update.latest_version))
                                    .color(Color32::from_rgb(0x17, 0x1A, 0x21))
                                    .font(FontId::proportional(12.0))
                                    .strong(),
                            )
                            .fill(if update_enabled {
                                Color32::from_rgb(0x66, 0xC0, 0xF4)
                            } else {
                                Color32::from_rgb(0x3B, 0x5D, 0x7A)
                            })
                            .corner_radius(CornerRadius::same(6));

                            if ui.add_enabled(update_enabled, update_btn).clicked() {
                                self.show_settings_modal = true;
                            }
                        }
                    });
                });
            });
    }

    fn render_right_panel_content(&mut self, ui: &mut Ui) {
        Frame::NONE
            .fill(Color32::from_rgb(0x16, 0x20, 0x2D))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let active_label = format!("Active ({})", self.active_sessions.len());
                    let is_active_tab = self.right_panel_tab == RightPanelTab::Active;
                    let tab_active_btn = render_tab_button(ui, &active_label, is_active_tab);
                    if tab_active_btn.clicked() {
                        self.right_panel_tab = RightPanelTab::Active;
                    }

                    ui.add_space(4.0);

                    let queue_label = format!("Queue ({})", self.planner.queue.len());
                    let is_queue_tab = self.right_panel_tab == RightPanelTab::Queue;
                    let tab_queue_btn = render_tab_button(ui, &queue_label, is_queue_tab);
                    if tab_queue_btn.clicked() {
                        self.right_panel_tab = RightPanelTab::Queue;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let collapse_btn = render_panel_collapse_button(ui);
                        if collapse_btn.clicked() {
                            self.settings.right_panel_visible = false;
                            save_settings(&self.settings);
                        }
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                match self.right_panel_tab {
                    RightPanelTab::Active => self.render_active_sessions_tab(ui),
                    RightPanelTab::Queue => self.render_queue_planner_tab(ui),
                }
            });
    }

    fn render_active_sessions_tab(&mut self, ui: &mut Ui) {
        let mut stop_all_clicked = false;
        let mut stopped_app_id = None;

        let available_height = ui.available_height();
        let bottom_button_height = 42.0;
        let content_height = (available_height - bottom_button_height - 10.0).max(100.0);

        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), content_height),
            Layout::top_down(Align::Min),
            |ui| {
                if self.active_sessions.is_empty() {
                    Frame::NONE
                        .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(20))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.add_space(16.0);
                                render_empty_sessions_icon(ui);
                                ui.add_space(10.0);
                                ui.label(
                                    RichText::new("No active sessions")
                                        .font(FontId::proportional(14.0))
                                        .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                        .strong(),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(
                                        "Select a game from the list on the left and click \"Launch\" to simulate the process in the background.",
                                    )
                                    .font(FontId::proportional(11.5))
                                    .color(Color32::from_rgb(0x55, 0x67, 0x72)),
                                );
                                ui.add_space(16.0);
                            });
                        });
                } else {
                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for session in &self.active_sessions {
                            Frame::NONE
                                .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x66, 0xC0, 0xF4)))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::same(12))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(&session.game_name)
                                                    .color(Color32::from_rgb(0x66, 0xC0, 0xF4))
                                                    .font(FontId::proportional(13.5))
                                                    .strong(),
                                            );
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                render_elapsed_badge(ui, &session.elapsed_formatted());
                                            });
                                        });

                                        if let Some(rem_str) = session.auto_stop_remaining_formatted() {
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(format!("Auto-stop in: {}", rem_str))
                                                        .font(FontId::proportional(11.0))
                                                        .color(Color32::from_rgb(0x66, 0xC0, 0xF4)),
                                                );
                                            });
                                        }

                                        ui.add_space(6.0);

                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(format!("PID: {}  •  Exe: {}", session.pid, session.process_name))
                                                    .font(FontId::proportional(11.0))
                                                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                                            );
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                let stop_btn = render_stop_button(ui, vec2(56.0, 24.0), "Stop", true);
                                                if stop_btn.clicked() {
                                                    stopped_app_id = Some(session.app_id);
                                                }
                                            });
                                        });
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    });
                }
            },
        );

        ui.add_space(10.0);

        let has_sessions = !self.active_sessions.is_empty();
        let stop_all_btn = render_stop_button(ui, vec2(ui.available_width(), 38.0), "Stop All Sessions", has_sessions);
        if has_sessions && stop_all_btn.clicked() {
            stop_all_clicked = true;
        }

        if stop_all_clicked {
            self.stop_all();
        }

        if let Some(id) = stopped_app_id {
            self.stop_game(id);
        }
    }

    fn render_queue_planner_tab(&mut self, ui: &mut Ui) {
        let is_running = self.planner.is_active();

        ui.horizontal(|ui| {
            if is_running {
                let stop_queue_btn = render_stop_button(ui, vec2(105.0, 30.0), "Stop Queue", true);
                if stop_queue_btn.clicked() {
                    let action = self.planner.stop();
                    if let PlannerAction::StopGame(app_id) = action {
                        self.stop_game(app_id);
                    }
                }
            } else {
                let can_start = !self.planner.queue.is_empty();
                let start_queue_btn = render_launch_button(ui, vec2(105.0, 30.0), "Start Queue", can_start);
                if can_start && start_queue_btn.clicked() {
                    let action = self.planner.start(&self.settings);
                    if let PlannerAction::StartGame(game, duration) = action {
                        self.execute_launch_game_with_duration(&game, Some(duration));
                    }
                }
            }

            ui.add_space(4.0);

            let fill_safe_btn = egui::Button::new(
                RichText::new("+ Fill Safe")
                    .color(Color32::from_rgb(0xE5, 0xEC, 0xEE))
                    .font(FontId::proportional(11.5)),
            )
            .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
            .corner_radius(CornerRadius::same(6));

            if ui.add_enabled(!is_running, fill_safe_btn).on_hover_text("Add all safe Discord detectable & never played games").clicked() {
                let added = self.planner.fill_safe_dc_games(&self.games);
                self.set_status(&format!("Added {} safe games to queue.", added));
            }

            ui.add_space(4.0);

            let clear_btn = egui::Button::new(
                RichText::new("Clear")
                    .color(Color32::from_rgb(0xE5, 0xEC, 0xEE))
                    .font(FontId::proportional(11.5)),
            )
            .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
            .corner_radius(CornerRadius::same(6));

            if ui.add_enabled(!is_running && !self.planner.queue.is_empty(), clear_btn).clicked() {
                self.planner.clear();
            }
        });

        ui.add_space(8.0);

        let progress = self.planner.progress_ratio();
        ui.add(
            egui::ProgressBar::new(progress)
                .show_percentage()
                .fill(Color32::from_rgb(0x66, 0xC0, 0xF4))
                .corner_radius(CornerRadius::same(4)),
        );

        ui.add_space(4.0);
        ui.label(
            RichText::new(self.planner.status_text())
                .font(FontId::proportional(11.5))
                .color(Color32::from_rgb(0x66, 0xC0, 0xF4)),
        );

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label(
            RichText::new("Queue Configuration")
                .font(FontId::proportional(12.5))
                .color(Color32::WHITE)
                .strong(),
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if toggle_switch(ui, &mut self.settings.planner_use_random_duration).changed() {
                save_settings(&self.settings);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Randomized duration").font(FontId::proportional(11.5)));
        });

        if self.settings.planner_use_random_duration {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Min:").font(FontId::proportional(11.0)));
                let min_r = self.settings.planner_duration_unit.min_allowed();
                let max_r = self.settings.planner_duration_unit.max_allowed();
                if ui.add(egui::DragValue::new(&mut self.settings.planner_min_duration_value).range(min_r..=max_r)).changed() {
                    save_settings(&self.settings);
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Max:").font(FontId::proportional(11.0)));
                if ui.add(egui::DragValue::new(&mut self.settings.planner_max_duration_value).range(min_r..=max_r)).changed() {
                    save_settings(&self.settings);
                }
                ui.add_space(4.0);
                let mut unit = self.settings.planner_duration_unit;
                let mut unit_changed = false;
                egui::ComboBox::from_id_salt("planner_dur_unit_combo")
                    .selected_text(unit.label())
                    .width(55.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut unit, TimeUnit::Ms, "ms").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Sec, "sec").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Min, "min").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Hour, "hour").clicked() { unit_changed = true; }
                    });
                if unit_changed {
                    self.settings.planner_duration_unit = unit;
                    save_settings(&self.settings);
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Duration:").font(FontId::proportional(11.0)));
                let min_r = self.settings.planner_duration_unit.min_allowed();
                let max_r = self.settings.planner_duration_unit.max_allowed();
                if ui.add(egui::DragValue::new(&mut self.settings.planner_duration_value).range(min_r..=max_r)).changed() {
                    save_settings(&self.settings);
                }
                ui.add_space(4.0);
                let mut unit = self.settings.planner_duration_unit;
                let mut unit_changed = false;
                egui::ComboBox::from_id_salt("planner_dur_unit_combo")
                    .selected_text(unit.label())
                    .width(55.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut unit, TimeUnit::Ms, "ms").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Sec, "sec").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Min, "min").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Hour, "hour").clicked() { unit_changed = true; }
                    });
                if unit_changed {
                    self.settings.planner_duration_unit = unit;
                    save_settings(&self.settings);
                }
            });
        }

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if toggle_switch(ui, &mut self.settings.planner_use_random_cooldown).changed() {
                save_settings(&self.settings);
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Randomized cooldown").font(FontId::proportional(11.5)));
        });

        if self.settings.planner_use_random_cooldown {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Min:").font(FontId::proportional(11.0)));
                let min_r = self.settings.planner_cooldown_unit.min_allowed();
                let max_r = self.settings.planner_cooldown_unit.max_allowed();
                if ui.add(egui::DragValue::new(&mut self.settings.planner_min_cooldown_value).range(min_r..=max_r)).changed() {
                    save_settings(&self.settings);
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Max:").font(FontId::proportional(11.0)));
                if ui.add(egui::DragValue::new(&mut self.settings.planner_max_cooldown_value).range(min_r..=max_r)).changed() {
                    save_settings(&self.settings);
                }
                ui.add_space(4.0);
                let mut unit = self.settings.planner_cooldown_unit;
                let mut unit_changed = false;
                egui::ComboBox::from_id_salt("planner_cool_unit_combo")
                    .selected_text(unit.label())
                    .width(55.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut unit, TimeUnit::Ms, "ms").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Sec, "sec").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Min, "min").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Hour, "hour").clicked() { unit_changed = true; }
                    });
                if unit_changed {
                    self.settings.planner_cooldown_unit = unit;
                    save_settings(&self.settings);
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Cooldown:").font(FontId::proportional(11.0)));
                let min_r = self.settings.planner_cooldown_unit.min_allowed();
                let max_r = self.settings.planner_cooldown_unit.max_allowed();
                if ui.add(egui::DragValue::new(&mut self.settings.planner_cooldown_value).range(min_r..=max_r)).changed() {
                    save_settings(&self.settings);
                }
                ui.add_space(4.0);
                let mut unit = self.settings.planner_cooldown_unit;
                let mut unit_changed = false;
                egui::ComboBox::from_id_salt("planner_cool_unit_combo")
                    .selected_text(unit.label())
                    .width(55.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut unit, TimeUnit::Ms, "ms").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Sec, "sec").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Min, "min").clicked() { unit_changed = true; }
                        if ui.selectable_value(&mut unit, TimeUnit::Hour, "hour").clicked() { unit_changed = true; }
                    });
                if unit_changed {
                    self.settings.planner_cooldown_unit = unit;
                    save_settings(&self.settings);
                }
            });
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        let mut remove_index = None;
        let mut move_up_index = None;
        let mut move_down_index = None;

        let queue_len = self.planner.queue.len();

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.planner.queue.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new("Queue is empty")
                            .font(FontId::proportional(13.0))
                            .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Click '+' on any game card to add it to this sequential queue.")
                            .font(FontId::proportional(11.0))
                            .color(Color32::from_rgb(0x55, 0x67, 0x72)),
                    );
                });
            } else {
                for (idx, game) in self.planner.queue.iter().enumerate() {
                    Frame::NONE
                        .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{}.", idx + 1))
                                        .font(FontId::proportional(11.5))
                                        .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                                );

                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&game.name)
                                            .font(FontId::proportional(12.5))
                                            .color(Color32::from_rgb(0xE5, 0xE7, 0xEB))
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(format!("AppID: {}", game.app_id))
                                            .font(FontId::proportional(10.5))
                                            .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                                    );
                                });

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if render_cross_button(ui, vec2(20.0, 20.0), !is_running).clicked() {
                                        remove_index = Some(idx);
                                    }

                                    if idx + 1 < queue_len {
                                        if render_arrow_down_button(ui, vec2(20.0, 20.0), !is_running).clicked() {
                                            move_down_index = Some(idx);
                                        }
                                    }

                                    if idx > 0 {
                                        if render_arrow_up_button(ui, vec2(20.0, 20.0), !is_running).clicked() {
                                            move_up_index = Some(idx);
                                        }
                                    }
                                });
                            });
                        });
                    ui.add_space(4.0);
                }
            }
        });

        if let Some(idx) = remove_index {
            self.planner.remove_at(idx);
        }
        if let Some(idx) = move_up_index {
            self.planner.move_up(idx);
        }
        if let Some(idx) = move_down_index {
            self.planner.move_down(idx);
        }
    }

    fn render_center_content(&mut self, ctx: &egui::Context, ui: &mut Ui, is_any_modal_open: bool) {
        let mut filtered_indices: Vec<(bool, usize)> = {
            let mut list = Vec::new();

            for (idx, game) in self.custom_games.iter().enumerate() {
                if self.matches_filters(game) {
                    list.push((true, idx));
                }
            }

            for (idx, game) in self.games.iter().enumerate() {
                if self.matches_filters(game) {
                    list.push((false, idx));
                }
            }

            list
        };

        self.sort_filtered_games(&mut filtered_indices);

        let total_count = self.games.len() + self.custom_games.len();

        ui.horizontal(|ui| {
            let search_edit = egui::TextEdit::singleline(&mut self.search_query)
                .hint_text("Search by name, AppID, or .exe...")
                .font(FontId::proportional(14.0))
                .margin(Margin::symmetric(12, 8))
                .vertical_align(Align::Center);
            ui.add_sized([340.0, 34.0], search_edit);

            ui.add_space(8.0);
            render_counter_pill(ui, filtered_indices.len(), total_count);

            ui.add_space(8.0);
            if render_view_mode_buttons(ui, &mut self.settings.view_mode).changed() {
                save_settings(&self.settings);
            }

            if let Some((ref msg, time)) = self.status_message {
                if time.elapsed().as_secs() < 5 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(msg)
                                .color(Color32::from_rgb(0x66, 0xC0, 0xF4))
                                .font(FontId::proportional(11.5)),
                        );
                    });
                }
            }
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            filter_pill(ui, "Favorites", &mut self.filter_favorites);
            ui.add_space(6.0);
            filter_pill(ui, "DC", &mut self.filter_discord);
            ui.add_space(6.0);
            filter_pill(ui, "Never Played", &mut self.filter_never_played);
            ui.add_space(6.0);
            filter_pill(ui, "Hide VAC", &mut self.filter_hide_vac);
            ui.add_space(6.0);
            filter_pill(ui, "Hide AC", &mut self.filter_hide_ac);

            ui.add_space(12.0);
            ui.label(
                RichText::new("Sort:")
                    .font(FontId::proportional(11.5))
                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
            );

            let sort_label = match self.settings.sort_order {
                SortOrder::NameAsc => "Name (A–Z)",
                SortOrder::LeastPlayed => "Least Played",
                SortOrder::MostPlayed => "Most Played",
                SortOrder::DiscordFirst => "DC Detectable First",
                SortOrder::FavoritesFirst => "Favorites First",
            };

            egui::ComboBox::from_id_salt("sort_order_combo")
                .selected_text(RichText::new(sort_label).font(FontId::proportional(11.5)))
                .width(150.0)
                .show_ui(ui, |ui| {
                    let mut order = self.settings.sort_order;
                    let mut changed = false;
                    if ui.selectable_value(&mut order, SortOrder::NameAsc, "Name (A–Z)").clicked() {
                        changed = true;
                    }
                    if ui.selectable_value(&mut order, SortOrder::LeastPlayed, "Least Played").clicked() {
                        changed = true;
                    }
                    if ui.selectable_value(&mut order, SortOrder::MostPlayed, "Most Played").clicked() {
                        changed = true;
                    }
                    if ui.selectable_value(&mut order, SortOrder::DiscordFirst, "DC Detectable First").clicked() {
                        changed = true;
                    }
                    if ui.selectable_value(&mut order, SortOrder::FavoritesFirst, "Favorites First").clicked() {
                        changed = true;
                    }
                    if changed {
                        self.settings.sort_order = order;
                        save_settings(&self.settings);
                    }
                });
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);

        match self.settings.view_mode {
            ViewMode::List => self.render_list_view(ctx, ui, &filtered_indices, is_any_modal_open),
            ViewMode::Grid => self.render_grid_view(ctx, ui, &filtered_indices, is_any_modal_open),
        }
    }

    fn sort_filtered_games(&self, indices: &mut [(bool, usize)]) {
        indices.sort_by(|a, b| {
            let game_a = if a.0 { &self.custom_games[a.1] } else { &self.games[a.1] };
            let game_b = if b.0 { &self.custom_games[b.1] } else { &self.games[b.1] };

            match self.settings.sort_order {
                SortOrder::NameAsc => game_a.name.to_lowercase().cmp(&game_b.name.to_lowercase()),
                SortOrder::LeastPlayed => {
                    let cmp = game_a.playtime_minutes.cmp(&game_b.playtime_minutes);
                    if cmp == std::cmp::Ordering::Equal {
                        game_a.name.to_lowercase().cmp(&game_b.name.to_lowercase())
                    } else {
                        cmp
                    }
                }
                SortOrder::MostPlayed => {
                    let cmp = game_b.playtime_minutes.cmp(&game_a.playtime_minutes);
                    if cmp == std::cmp::Ordering::Equal {
                        game_a.name.to_lowercase().cmp(&game_b.name.to_lowercase())
                    } else {
                        cmp
                    }
                }
                SortOrder::DiscordFirst => {
                    let cmp = game_b.is_discord_detectable.cmp(&game_a.is_discord_detectable);
                    if cmp == std::cmp::Ordering::Equal {
                        game_a.name.to_lowercase().cmp(&game_b.name.to_lowercase())
                    } else {
                        cmp
                    }
                }
                SortOrder::FavoritesFirst => {
                    let fav_a = self.settings.favorite_app_ids.contains(&game_a.app_id);
                    let fav_b = self.settings.favorite_app_ids.contains(&game_b.app_id);
                    let cmp = fav_b.cmp(&fav_a);
                    if cmp == std::cmp::Ordering::Equal {
                        game_a.name.to_lowercase().cmp(&game_b.name.to_lowercase())
                    } else {
                        cmp
                    }
                }
            }
        });
    }

    fn render_list_view(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        filtered_indices: &[(bool, usize)],
        is_any_modal_open: bool,
    ) {
        let mut launched_game = None;
        let mut stopped_app_id = None;
        let mut edit_game_action = None;
        let mut details_game_action = None;
        let mut queue_add_action = None;
        let mut dc_info_action = false;
        let mut vac_info_action = false;
        let mut ac_info_action = false;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let scroll_clip_rect = ui.clip_rect();

            for &(is_custom, idx) in filtered_indices {
                let (app_id, icon_path) = if is_custom {
                    (self.custom_games[idx].app_id, self.custom_games[idx].icon_path.clone())
                } else {
                    (self.games[idx].app_id, self.games[idx].icon_path.clone())
                };

                let icon_tex = self.get_game_icon_texture(ctx, app_id, icon_path.as_deref());

                let game = if is_custom {
                    &mut self.custom_games[idx]
                } else {
                    &mut self.games[idx]
                };

                let card_id = ui.make_persistent_id(format!("card_{}", game.app_id));
                let prev_rect: Option<egui::Rect> = ui.data(|d| d.get_temp(card_id));
                let is_hovered = !is_any_modal_open && ui.input(|i| {
                    i.pointer
                        .hover_pos()
                        .map_or(false, |pos| prev_rect.map_or(false, |r| r.contains(pos) && scroll_clip_rect.contains(pos)))
                });

                let is_running = game.is_running;

                let card_bg = if is_running {
                    Color32::from_rgb(0x1F, 0x2E, 0x40)
                } else if is_hovered {
                    Color32::from_rgb(0x22, 0x33, 0x48)
                } else {
                    Color32::from_rgb(0x1B, 0x28, 0x38)
                };

                let card_stroke = Stroke::new(
                    if is_running { 1.5_f32 } else { 1.0_f32 },
                    if is_running {
                        Color32::from_rgb(0x66, 0xC0, 0xF4)
                    } else if is_hovered {
                        Color32::from_rgb(0x41, 0x68, 0x8B)
                    } else {
                        Color32::from_rgb(0x2A, 0x47, 0x5E)
                    },
                );

                let mut launch_btn_rect = None;
                let mut edit_btn_rect = None;
                let mut queue_btn_rect = None;
                let mut star_btn_rect = None;
                let mut combo_rect = None;
                let mut dc_badge_rect = None;
                let mut vac_badge_rect = None;
                let mut ac_badge_rect = None;

                let card_response = Frame::NONE
                    .fill(card_bg)
                    .stroke(card_stroke)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_min_height(56.0);

                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), 56.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let icon_size = vec2(42.0, 42.0);
                                if let Some(ref tex) = icon_tex {
                                    ui.image((tex.id(), icon_size));
                                } else {
                                    render_icon_placeholder(ui, icon_size, &game.name);
                                }

                                ui.add_space(10.0);

                                ui.vertical(|ui| {
                                    ui.add_space(7.0);

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(&game.name)
                                                .color(Color32::from_rgb(0xE5, 0xE7, 0xEB))
                                                .font(FontId::proportional(14.5))
                                                .strong(),
                                        );

                                        if game.is_custom {
                                            render_badge(ui, "CUSTOM", Color32::from_rgb(0x6A, 0x4B, 0x94));
                                        }
                                        if game.is_discord_detectable {
                                            let dc_res = render_clickable_badge(
                                                ui,
                                                "DC",
                                                Color32::from_rgb(0x58, 0x65, 0xF2),
                                                "Recognized in Discord's detectable games database. Click for details.",
                                            );
                                            dc_badge_rect = Some(dc_res.rect);
                                            if !is_any_modal_open && dc_res.clicked() {
                                                dc_info_action = true;
                                            }
                                        }
                                        if game.is_vac_protected {
                                            let vac_res = render_clickable_badge(
                                                ui,
                                                "VAC",
                                                Color32::from_rgb(0xDC, 0x26, 0x26),
                                                "Valve Anti-Cheat (VAC) protected. Click for details.",
                                            );
                                            vac_badge_rect = Some(vac_res.rect);
                                            if !is_any_modal_open && vac_res.clicked() {
                                                vac_info_action = true;
                                            }
                                        }
                                        if game.is_third_party_anti_cheat {
                                            let ac_res = render_clickable_badge(
                                                ui,
                                                "AC",
                                                Color32::from_rgb(0xD9, 0x77, 0x06),
                                                "Third-party Anti-Cheat detected. Click for details.",
                                            );
                                            ac_badge_rect = Some(ac_res.rect);
                                            if !is_any_modal_open && ac_res.clicked() {
                                                ac_info_action = true;
                                            }
                                        }
                                    });

                                    ui.add_space(3.0);

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("AppID: {}", game.app_id))
                                                .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                                .font(FontId::proportional(11.5)),
                                        );

                                        ui.label(
                                            RichText::new("  •  ")
                                                .color(Color32::from_rgb(0x55, 0x67, 0x72))
                                                .font(FontId::proportional(11.5)),
                                        );

                                        if game.available_executables.len() > 1 {
                                            ui.label(
                                                RichText::new("Exe: ")
                                                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                                    .font(FontId::proportional(11.5)),
                                            );

                                            ui.scope(|ui| {
                                                ui.spacing_mut().button_padding = vec2(6.0, 3.0);
                                                let mut current_exe = game.executable_relative_path.clone();
                                                let combo_res = egui::ComboBox::from_id_salt(format!("combo_{}", game.app_id))
                                                    .selected_text(
                                                        RichText::new(&current_exe)
                                                            .font(FontId::proportional(11.5))
                                                            .color(Color32::from_rgb(0x66, 0xC0, 0xF4)),
                                                    )
                                                    .width(210.0)
                                                    .show_ui(ui, |ui| {
                                                        for exe in &game.available_executables {
                                                            if ui.selectable_value(&mut current_exe, exe.clone(), exe).clicked() {
                                                                game.executable_relative_path = exe.clone();
                                                                save_or_update_override(game);
                                                            }
                                                        }
                                                    });
                                                combo_rect = Some(combo_res.response.rect);
                                            });
                                        } else {
                                            ui.label(
                                                RichText::new(format!("Exe: {}", game.executable_relative_path))
                                                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                                    .font(FontId::proportional(11.5)),
                                            );
                                        }
                                    });
                                });

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.add_space(12.0);

                                    if game.is_running {
                                        let res = render_stop_button(ui, vec2(106.0, 34.0), "Stop", !is_any_modal_open);
                                        launch_btn_rect = Some(res.rect);
                                        if !is_any_modal_open && res.clicked() {
                                            stopped_app_id = Some(game.app_id);
                                        }
                                    } else {
                                        let res = render_launch_button(ui, vec2(106.0, 34.0), "Launch", !is_any_modal_open);
                                        launch_btn_rect = Some(res.rect);
                                        if !is_any_modal_open && res.clicked() {
                                            launched_game = Some(game.clone());
                                        }
                                    }

                                    ui.add_space(6.0);

                                    let edit_enabled = !is_any_modal_open && !game.is_running;
                                    let res = render_edit_text_button(ui, vec2(62.0, 34.0), edit_enabled);
                                    edit_btn_rect = Some(res.rect);
                                    if edit_enabled && res.clicked() {
                                        edit_game_action = Some((game.clone(), is_custom));
                                    }

                                    ui.add_space(6.0);

                                    let res = render_plus_icon_button(ui, vec2(34.0, 34.0), 6, !is_any_modal_open)
                                        .on_hover_text("Add to Session Queue");
                                    queue_btn_rect = Some(res.rect);
                                    if !is_any_modal_open && res.clicked() {
                                        queue_add_action = Some(game.clone());
                                    }

                                    ui.add_space(6.0);

                                    let is_fav = self.settings.favorite_app_ids.contains(&game.app_id);
                                    let star_res = render_favorite_star_button(ui, is_fav, 34.0);
                                    star_btn_rect = Some(star_res.rect);
                                    if !is_any_modal_open && star_res.clicked() {
                                        if is_fav {
                                            self.settings.favorite_app_ids.retain(|&id| id != game.app_id);
                                        } else {
                                            self.settings.favorite_app_ids.push(game.app_id);
                                        }
                                        save_settings(&self.settings);
                                    }
                                });
                            },
                        );
                    });

                let card_rect = card_response.response.rect;
                ui.data_mut(|d| d.insert_temp(card_id, card_rect));
                ui.add_space(5.0);

                if !is_any_modal_open {
                    let is_inside_interactive = |pos: egui::Pos2| -> bool {
                        launch_btn_rect.map_or(false, |r| r.contains(pos))
                            || edit_btn_rect.map_or(false, |r| r.contains(pos))
                            || queue_btn_rect.map_or(false, |r| r.contains(pos))
                            || star_btn_rect.map_or(false, |r| r.contains(pos))
                            || combo_rect.map_or(false, |r| r.contains(pos))
                            || dc_badge_rect.map_or(false, |r| r.contains(pos))
                            || vac_badge_rect.map_or(false, |r| r.contains(pos))
                            || ac_badge_rect.map_or(false, |r| r.contains(pos))
                    };

                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if ctx.input(|i| i.pointer.primary_clicked())
                            && scroll_clip_rect.contains(pos)
                            && card_rect.contains(pos)
                            && !is_inside_interactive(pos)
                        {
                            details_game_action = Some(game.clone());
                        }
                    }

                    if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                        if scroll_clip_rect.contains(pos)
                            && card_rect.contains(pos)
                            && !is_inside_interactive(pos)
                        {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    }
                }
            }
        });

        if let Some(game) = launched_game {
            self.try_launch_game(game);
        }

        if let Some(id) = stopped_app_id {
            self.stop_game(id);
        }

        if let Some((game, is_custom)) = edit_game_action {
            self.editing_game = Some(game);
            self.is_editing_custom = is_custom;
            self.show_edit_game_modal = true;
        }

        if let Some(game) = details_game_action {
            self.selected_details_game = Some(game);
            self.show_details_modal = true;
        }

        if let Some(game) = queue_add_action {
            if self.planner.add_game(game.clone()) {
                self.set_status(&format!("Added {} to session queue.", game.name));
            } else {
                self.set_status(&format!("{} is already in session queue.", game.name));
            }
        }

        if dc_info_action {
            self.show_dc_info_modal = true;
        }

        if vac_info_action {
            self.show_vac_info_modal = true;
        }

        if ac_info_action {
            self.show_ac_info_modal = true;
        }
    }

    fn render_grid_view(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        filtered_indices: &[(bool, usize)],
        is_any_modal_open: bool,
    ) {
        let mut launched_game = None;
        let mut stopped_app_id = None;
        let mut edit_game_action = None;
        let mut details_game_action = None;
        let mut queue_add_action = None;
        let mut dc_info_action = false;
        let mut vac_info_action = false;
        let mut ac_info_action = false;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let usable_width = (ui.available_width() - 18.0).max(220.0);
            let spacing = 10.0_f32;
            let target_col_width = 240.0_f32;
            let cols = ((usable_width + spacing) / (target_col_width + spacing)).floor().max(1.0) as usize;
            let total_gaps = (cols.saturating_sub(1) as f32) * spacing;
            let col_outer_width = ((usable_width - total_gaps) / (cols as f32)).floor().max(180.0);
            let frame_horizontal_padding = 16.0_f32;
            let card_inner_width = (col_outer_width - frame_horizontal_padding).max(160.0);
            let banner_height = (card_inner_width * (215.0 / 460.0)).round();
            let card_inner_height = banner_height + 6.0 + 18.0 + 4.0 + 20.0 + 8.0 + 30.0;
            let scroll_clip_rect = ui.clip_rect();

            for chunk in filtered_indices.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    for (item_idx, &(is_custom, idx)) in chunk.iter().enumerate() {
                        let (app_id, banner_path, icon_path) = if is_custom {
                            (
                                self.custom_games[idx].app_id,
                                self.custom_games[idx].banner_path.clone(),
                                self.custom_games[idx].icon_path.clone(),
                            )
                        } else {
                            (
                                self.games[idx].app_id,
                                self.games[idx].banner_path.clone(),
                                self.games[idx].icon_path.clone(),
                            )
                        };

                        let banner_tex = self.get_game_banner_texture(ctx, app_id, banner_path.as_deref(), icon_path.as_deref());

                        let game = if is_custom {
                            &mut self.custom_games[idx]
                        } else {
                            &mut self.games[idx]
                        };

                        let is_running = game.is_running;

                        let card_bg = if is_running {
                            Color32::from_rgb(0x1F, 0x2E, 0x40)
                        } else {
                            Color32::from_rgb(0x1B, 0x28, 0x38)
                        };

                        let card_stroke = Stroke::new(
                            if is_running { 1.5_f32 } else { 1.0_f32 },
                            if is_running {
                                Color32::from_rgb(0x66, 0xC0, 0xF4)
                            } else {
                                Color32::from_rgb(0x2A, 0x47, 0x5E)
                            },
                        );

                        let mut launch_btn_rect = None;
                        let mut edit_btn_rect = None;
                        let mut queue_btn_rect = None;
                        let mut star_btn_rect = None;
                        let mut dc_badge_rect = None;
                        let mut vac_badge_rect = None;
                        let mut ac_badge_rect = None;

                        let card_response = Frame::NONE
                            .fill(card_bg)
                            .stroke(card_stroke)
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::same(8))
                            .show(ui, |ui| {
                                ui.set_width(card_inner_width);
                                ui.set_height(card_inner_height);

                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                    let banner_size = vec2(card_inner_width, banner_height);
                                    let (banner_rect, _) = ui.allocate_exact_size(banner_size, egui::Sense::hover());

                                    if let Some(ref tex) = banner_tex {
                                        ui.painter().image(
                                            tex.id(),
                                            banner_rect,
                                            egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        render_banner_placeholder_at(ui, banner_rect, &game.name);
                                    }

                                    ui.add_space(6.0);

                                    let title_cut = if game.name.len() > 30 {
                                        format!("{}...", &game.name[..28])
                                    } else {
                                        game.name.clone()
                                    };
                                    ui.add_sized(
                                        [card_inner_width, 18.0],
                                        egui::Label::new(
                                            RichText::new(title_cut)
                                                .font(FontId::proportional(12.5))
                                                .color(Color32::from_rgb(0xE5, 0xE7, 0xEB))
                                                .strong(),
                                        ).truncate(),
                                    );

                                    ui.add_space(4.0);

                                    ui.allocate_ui_with_layout(
                                        vec2(card_inner_width, 20.0),
                                        Layout::left_to_right(Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                                            if game.is_custom {
                                                render_badge(ui, "CUSTOM", Color32::from_rgb(0x6A, 0x4B, 0x94));
                                            }
                                            if game.is_discord_detectable {
                                                let dc_res = render_clickable_badge(
                                                    ui,
                                                    "DC",
                                                    Color32::from_rgb(0x58, 0x65, 0xF2),
                                                    "Recognized in Discord's detectable games database. Click for details.",
                                                );
                                                dc_badge_rect = Some(dc_res.rect);
                                                if !is_any_modal_open && dc_res.clicked() {
                                                    dc_info_action = true;
                                                }
                                            }
                                            if game.is_vac_protected {
                                                let vac_res = render_clickable_badge(
                                                    ui,
                                                    "VAC",
                                                    Color32::from_rgb(0xDC, 0x26, 0x26),
                                                    "Valve Anti-Cheat (VAC) protected. Click for details.",
                                                );
                                                vac_badge_rect = Some(vac_res.rect);
                                                if !is_any_modal_open && vac_res.clicked() {
                                                    vac_info_action = true;
                                                }
                                            }
                                            if game.is_third_party_anti_cheat {
                                                let ac_res = render_clickable_badge(
                                                    ui,
                                                    "AC",
                                                    Color32::from_rgb(0xD9, 0x77, 0x06),
                                                    "Third-party Anti-Cheat detected. Click for details.",
                                                );
                                                ac_badge_rect = Some(ac_res.rect);
                                                if !is_any_modal_open && ac_res.clicked() {
                                                    ac_info_action = true;
                                                }
                                            }
                                        },
                                    );

                                    ui.add_space(8.0);

                                    let btn_size = 30.0_f32;
                                    let btn_gap = 4.0_f32;
                                    let launch_width = (card_inner_width - 3.0 * btn_size - 3.0 * btn_gap).max(65.0);
                                    ui.allocate_ui_with_layout(
                                        vec2(card_inner_width, 30.0),
                                        Layout::left_to_right(Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing = vec2(btn_gap, 0.0);
                                            if game.is_running {
                                                let res = render_stop_button(ui, vec2(launch_width, 30.0), "Stop", !is_any_modal_open);
                                                launch_btn_rect = Some(res.rect);
                                                if res.clicked() && !is_any_modal_open {
                                                    stopped_app_id = Some(game.app_id);
                                                }
                                            } else {
                                                let res = render_launch_button(ui, vec2(launch_width, 30.0), "Launch", !is_any_modal_open);
                                                launch_btn_rect = Some(res.rect);
                                                if res.clicked() && !is_any_modal_open {
                                                    launched_game = Some(game.clone());
                                                }
                                            }

                                            let edit_enabled = !is_any_modal_open && !game.is_running;
                                            let res = render_edit_icon_button(ui, vec2(btn_size, btn_size), 6, edit_enabled);
                                            edit_btn_rect = Some(res.rect);
                                            if res.clicked() && edit_enabled {
                                                edit_game_action = Some((game.clone(), is_custom));
                                            }

                                            let res = render_plus_icon_button(ui, vec2(btn_size, btn_size), 6, !is_any_modal_open)
                                                .on_hover_text("Add to Session Queue");
                                            queue_btn_rect = Some(res.rect);
                                            if res.clicked() && !is_any_modal_open {
                                                queue_add_action = Some(game.clone());
                                            }

                                            let is_fav = self.settings.favorite_app_ids.contains(&game.app_id);
                                            let star_res = render_favorite_star_button(ui, is_fav, btn_size)
                                                .on_hover_text("Toggle Favorite");
                                            star_btn_rect = Some(star_res.rect);
                                            if star_res.clicked() && !is_any_modal_open {
                                                if is_fav {
                                                    self.settings.favorite_app_ids.retain(|&id| id != game.app_id);
                                                } else {
                                                    self.settings.favorite_app_ids.push(game.app_id);
                                                }
                                                save_settings(&self.settings);
                                            }
                                        },
                                    );
                                });
                            });

                        let card_rect = card_response.response.rect;

                        if !is_any_modal_open {
                            let is_inside_interactive = |pos: egui::Pos2| -> bool {
                                launch_btn_rect.map_or(false, |r| r.contains(pos))
                                    || edit_btn_rect.map_or(false, |r| r.contains(pos))
                                    || queue_btn_rect.map_or(false, |r| r.contains(pos))
                                    || star_btn_rect.map_or(false, |r| r.contains(pos))
                                    || dc_badge_rect.map_or(false, |r| r.contains(pos))
                                    || vac_badge_rect.map_or(false, |r| r.contains(pos))
                                    || ac_badge_rect.map_or(false, |r| r.contains(pos))
                            };

                            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                                if ctx.input(|i| i.pointer.primary_clicked())
                                    && scroll_clip_rect.contains(pos)
                                    && card_rect.contains(pos)
                                    && !is_inside_interactive(pos)
                                {
                                    details_game_action = Some(game.clone());
                                }
                            }

                            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                                if scroll_clip_rect.contains(pos)
                                    && card_rect.contains(pos)
                                    && !is_inside_interactive(pos)
                                {
                                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                            }
                        }

                        if item_idx + 1 < chunk.len() {
                            ui.add_space(spacing);
                        }
                    }
                });
                ui.add_space(spacing);
            }
        });

        if let Some(game) = launched_game {
            self.try_launch_game(game);
        }

        if let Some(id) = stopped_app_id {
            self.stop_game(id);
        }

        if let Some((game, is_custom)) = edit_game_action {
            self.editing_game = Some(game);
            self.is_editing_custom = is_custom;
            self.show_edit_game_modal = true;
        }

        if let Some(game) = details_game_action {
            self.selected_details_game = Some(game);
            self.show_details_modal = true;
        }

        if let Some(game) = queue_add_action {
            if self.planner.add_game(game.clone()) {
                self.set_status(&format!("Added {} to session queue.", game.name));
            } else {
                self.set_status(&format!("{} is already in session queue.", game.name));
            }
        }

        if dc_info_action {
            self.show_dc_info_modal = true;
        }

        if vac_info_action {
            self.show_vac_info_modal = true;
        }

        if ac_info_action {
            self.show_ac_info_modal = true;
        }
    }

    fn render_settings_modal(&mut self, ctx: &egui::Context) {
        if !self.show_settings_modal {
            return;
        }

        let mut close_modal = false;

        egui::Window::new("Settings")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(500.0, 520.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
                    ui.heading("General Preferences");
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if toggle_switch(ui, &mut self.settings.check_updates_on_startup).changed() {
                            save_settings(&self.settings);
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Check for updates automatically on startup")
                                .font(FontId::proportional(13.0))
                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                        );
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if toggle_switch(ui, &mut self.settings.show_anti_cheat_notices).changed() {
                            save_settings(&self.settings);
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Show Anti-Cheat / VAC safety notice before launching")
                                .font(FontId::proportional(13.0))
                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                        );
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.heading("Safety & Anti-Cheat Shield");
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        let mut shield_val = self.settings.vac_shield_enabled;
                        if toggle_switch(ui, &mut shield_val).changed() {
                            if !shield_val {
                                self.show_vac_shield_disable_warning_modal = true;
                            } else {
                                self.settings.vac_shield_enabled = true;
                                save_settings(&self.settings);
                            }
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Enable VAC & Anti-Cheat Safety Shield")
                                .font(FontId::proportional(13.0))
                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                        );
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if toggle_switch(ui, &mut self.settings.stop_idling_on_steam_game_start).changed() {
                            save_settings(&self.settings);
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Stop all idling when any real Steam game starts")
                                .font(FontId::proportional(13.0))
                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                        );
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.heading("Session History & Statistics");
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if toggle_switch(ui, &mut self.settings.record_session_history).changed() {
                            save_settings(&self.settings);
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Record session history & statistics")
                                .font(FontId::proportional(13.0))
                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                        );
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        let clear_btn = egui::Button::new(
                            RichText::new("Clear History")
                                .color(Color32::from_rgb(0xFF, 0xC0, 0xC0))
                                .font(FontId::proportional(12.0)),
                        )
                        .fill(Color32::from_rgb(0x3D, 0x21, 0x21))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x5E, 0x2A, 0x2A)))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([120.0, 28.0], clear_btn).clicked() {
                            clear_history_file();
                            self.history_store = load_history();
                            self.set_status("Session history cleared.");
                        }

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Deletes history.json from disk and resets statistics.")
                                .font(FontId::proportional(11.0))
                                .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                        );
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.heading("Session Auto-Stop");
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if toggle_switch(ui, &mut self.settings.enable_auto_stop).changed() {
                            save_settings(&self.settings);
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Enable automatic session stop for launched games")
                                .font(FontId::proportional(13.0))
                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                        );
                    });
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new("Automatically stops manually launched games after a set duration (useful for Discord Quests).")
                            .font(FontId::proportional(11.5))
                            .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                    );

                    if self.settings.enable_auto_stop {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if toggle_switch(ui, &mut self.settings.auto_stop_use_random).changed() {
                                save_settings(&self.settings);
                            }
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Use randomized interval")
                                    .font(FontId::proportional(12.0)),
                            );
                        });

                        ui.add_space(4.0);
                        if self.settings.auto_stop_use_random {
                            ui.horizontal(|ui| {
                                ui.label("Min:");
                                let min_r = self.settings.auto_stop_unit.min_allowed();
                                let max_r = self.settings.auto_stop_unit.max_allowed();
                                if ui.add(egui::DragValue::new(&mut self.settings.auto_stop_min_value).range(min_r..=max_r)).changed() {
                                    save_settings(&self.settings);
                                }
                                ui.add_space(6.0);
                                ui.label("Max:");
                                if ui.add(egui::DragValue::new(&mut self.settings.auto_stop_max_value).range(min_r..=max_r)).changed() {
                                    save_settings(&self.settings);
                                }
                                ui.add_space(6.0);
                                let mut unit = self.settings.auto_stop_unit;
                                let mut unit_changed = false;
                                egui::ComboBox::from_id_salt("auto_stop_unit_combo")
                                    .selected_text(unit.label())
                                    .width(55.0)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_value(&mut unit, TimeUnit::Ms, "ms").clicked() { unit_changed = true; }
                                        if ui.selectable_value(&mut unit, TimeUnit::Sec, "sec").clicked() { unit_changed = true; }
                                        if ui.selectable_value(&mut unit, TimeUnit::Min, "min").clicked() { unit_changed = true; }
                                        if ui.selectable_value(&mut unit, TimeUnit::Hour, "hour").clicked() { unit_changed = true; }
                                    });
                                if unit_changed {
                                    self.settings.auto_stop_unit = unit;
                                    save_settings(&self.settings);
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Fixed duration:");
                                let min_r = self.settings.auto_stop_unit.min_allowed();
                                let max_r = self.settings.auto_stop_unit.max_allowed();
                                if ui.add(egui::DragValue::new(&mut self.settings.auto_stop_value).range(min_r..=max_r)).changed() {
                                    save_settings(&self.settings);
                                }
                                ui.add_space(6.0);
                                let mut unit = self.settings.auto_stop_unit;
                                let mut unit_changed = false;
                                egui::ComboBox::from_id_salt("auto_stop_unit_combo")
                                    .selected_text(unit.label())
                                    .width(55.0)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_value(&mut unit, TimeUnit::Ms, "ms").clicked() { unit_changed = true; }
                                        if ui.selectable_value(&mut unit, TimeUnit::Sec, "sec").clicked() { unit_changed = true; }
                                        if ui.selectable_value(&mut unit, TimeUnit::Min, "min").clicked() { unit_changed = true; }
                                        if ui.selectable_value(&mut unit, TimeUnit::Hour, "hour").clicked() { unit_changed = true; }
                                    });
                                if unit_changed {
                                    self.settings.auto_stop_unit = unit;
                                    save_settings(&self.settings);
                                }
                            });
                        }
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("Includes an automatic +45s buffer when duration is at least 15 min for Discord detection.")
                                .font(FontId::proportional(11.0))
                                .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                        );
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.heading("Application Updates");
                    ui.add_space(6.0);

                    ui.label(format!("Current Version: v{}", env!("CARGO_PKG_VERSION")));

                    if let Some(ref update) = self.update_info {
                        ui.label(
                            RichText::new(format!("Latest Version: v{}", update.latest_version))
                                .color(Color32::from_rgb(0x66, 0xC0, 0xF4))
                                .strong(),
                        );

                        ui.add_space(4.0);
                        ui.label("Changelog:");
                        ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                            ui.label(RichText::new(&update.changelog).font(FontId::proportional(11.0)));
                        });

                        ui.add_space(8.0);
                        let install_btn = egui::Button::new(
                            RichText::new("Download & Install Update")
                                .color(Color32::from_rgb(0x17, 0x1A, 0x21))
                                .font(FontId::proportional(12.5))
                                .strong(),
                        )
                        .fill(Color32::from_rgb(0x66, 0xC0, 0xF4))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([220.0, 32.0], install_btn).clicked() {
                            let url = update.download_url.clone();
                            stop_all_sessions(&mut self.active_sessions);
                            if let Err(e) = install_update(&url) {
                                self.set_status(&format!("Update error: {}", e));
                            }
                        }
                    } else if self.is_checking_updates {
                        ui.label("Checking for updates...");
                    } else {
                        ui.label(
                            RichText::new("You are running the latest version.")
                                .color(Color32::from_rgb(0x57, 0xCB, 0x58)),
                        );

                        ui.add_space(6.0);
                        let check_btn = egui::Button::new(
                            RichText::new("Check for Updates")
                                .color(Color32::WHITE)
                                .font(FontId::proportional(12.0)),
                        )
                        .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([150.0, 30.0], check_btn).clicked() {
                            self.check_updates_async();
                        }
                    }
                });

                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.0)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([80.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_settings_modal = false;
        }
    }

    fn render_details_modal(&mut self, ctx: &egui::Context) {
        if !self.show_details_modal {
            return;
        }

        let mut close_modal = false;
        let mut toggle_launch = None;
        let is_any_modal_open = self.has_active_modal();

        if let Some(game) = self.selected_details_game.clone() {
            let is_running = game.is_running;

            egui::Window::new("Game Details")
                .order(Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .fixed_size(vec2(480.0, 320.0))
                .frame(
                    Frame::NONE
                        .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(20)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let icon_tex = self.get_game_icon_texture(ctx, game.app_id, game.icon_path.as_deref());
                        if let Some(ref tex) = icon_tex {
                            ui.image((tex.id(), vec2(48.0, 48.0)));
                        } else {
                            render_icon_placeholder(ui, vec2(48.0, 48.0), &game.name);
                        }

                        ui.add_space(12.0);

                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(&game.name)
                                    .color(Color32::WHITE)
                                    .font(FontId::proportional(16.0))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(format!("AppID: {}", game.app_id))
                                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                    .font(FontId::proportional(12.0)),
                            );
                        });

                        let is_fav = self.settings.favorite_app_ids.contains(&game.app_id);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if render_favorite_star_button(ui, is_fav, 34.0).clicked() {
                                if is_fav {
                                    self.settings.favorite_app_ids.retain(|&id| id != game.app_id);
                                } else {
                                    self.settings.favorite_app_ids.push(game.app_id);
                                }
                                save_settings(&self.settings);
                            }
                        });
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    egui::Grid::new("game_details_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Executable Path:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            ui.label(RichText::new(&game.executable_relative_path).color(Color32::WHITE));
                            ui.end_row();

                            ui.label(RichText::new("Total Playtime:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            ui.label(RichText::new(game.playtime_formatted()).color(Color32::WHITE));
                            ui.end_row();

                            ui.label(RichText::new("Last Played:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            ui.label(RichText::new(game.last_played_formatted()).color(Color32::WHITE));
                            ui.end_row();

                            ui.label(RichText::new("Discord Detectable:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            if game.is_discord_detectable {
                                ui.label(RichText::new("Yes").color(Color32::from_rgb(0x58, 0x65, 0xF2)).strong());
                            } else {
                                ui.label(RichText::new("No").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            }
                            ui.end_row();

                            ui.label(RichText::new("Protection:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            ui.horizontal(|ui| {
                                if game.is_vac_protected {
                                    render_badge(ui, "VAC", Color32::from_rgb(0xDC, 0x26, 0x26));
                                }
                                if game.is_third_party_anti_cheat {
                                    render_badge(ui, "AC", Color32::from_rgb(0xD9, 0x77, 0x06));
                                }
                                if !game.is_vac_protected && !game.is_third_party_anti_cheat {
                                    ui.label(RichText::new("None (Safe)").color(Color32::from_rgb(0x57, 0xCB, 0x58)));
                                }
                            });
                            ui.end_row();

                            let stats = self.history_store.games.get(&game.app_id);

                            ui.label(RichText::new("Idler Playtime:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            let idled_str = stats.map_or("0s".to_string(), |s| format_duration_human(s.total_idled_seconds));
                            ui.label(RichText::new(idled_str).color(Color32::from_rgb(0x66, 0xC0, 0xF4)).strong());
                            ui.end_row();

                            ui.label(RichText::new("Idler Sessions:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            let sessions_str = stats.map_or("0".to_string(), |s| s.session_count.to_string());
                            ui.label(RichText::new(sessions_str).color(Color32::WHITE));
                            ui.end_row();

                            ui.label(RichText::new("Last Idled:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                            let last_idled_str = stats.map_or("Never".to_string(), |s| format_timestamp_human(s.last_session_unix));
                            ui.label(RichText::new(last_idled_str).color(Color32::WHITE));
                            ui.end_row();
                        });

                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        if is_running {
                            let stop_btn = render_stop_button(ui, vec2(130.0, 36.0), "Stop Session", !is_any_modal_open || self.show_details_modal);
                            if stop_btn.clicked() {
                                toggle_launch = Some((game.clone(), true));
                            }
                        } else {
                            let launch_btn = render_launch_button(ui, vec2(140.0, 36.0), "Launch Session", !is_any_modal_open || self.show_details_modal);
                            if launch_btn.clicked() {
                                toggle_launch = Some((game.clone(), false));
                            }
                        }

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let close_btn = egui::Button::new(
                                RichText::new("Close")
                                    .color(Color32::WHITE)
                                    .font(FontId::proportional(12.5)),
                            )
                            .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                            .corner_radius(CornerRadius::same(6));

                            if ui.add_sized([85.0, 36.0], close_btn).clicked() {
                                close_modal = true;
                            }
                        });
                    });
                });
        }

        if close_modal {
            self.show_details_modal = false;
            self.selected_details_game = None;
        }

        if let Some((game, should_stop)) = toggle_launch {
            if should_stop {
                self.stop_game(game.app_id);
            } else {
                self.try_launch_game(game);
            }
            self.show_details_modal = false;
            self.selected_details_game = None;
        }
    }

    fn render_dc_info_modal(&mut self, ctx: &egui::Context) {
        if !self.show_dc_info_modal {
            return;
        }

        let mut close_modal = false;

        egui::Window::new("Discord Detectable Game")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(480.0, 240.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x58, 0x65, 0xF2)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    render_badge(ui, "DC", Color32::from_rgb(0x58, 0x65, 0xF2));
                    ui.add_space(6.0);
                    ui.heading("Discord Detectable Application");
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    RichText::new("This game is registered in Discord's official database of detectable games.")
                        .font(FontId::proportional(12.5))
                        .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                );

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "When simulated through SteamProcessIdler, Discord will recognize the application as actively running. This makes it ideal for fulfilling Discord Quests and earning game activity badges when idling for approximately 15 to 20 minutes.",
                    )
                    .font(FontId::proportional(11.5))
                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                );

                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.0)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([80.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_dc_info_modal = false;
        }
    }

    fn render_vac_info_modal(&mut self, ctx: &egui::Context) {
        if !self.show_vac_info_modal {
            return;
        }

        let mut close_modal = false;

        egui::Window::new("Valve Anti-Cheat (VAC) Information")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(480.0, 240.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xDC, 0x26, 0x26)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    render_badge(ui, "VAC", Color32::from_rgb(0xDC, 0x26, 0x26));
                    ui.add_space(6.0);
                    ui.heading("Valve Anti-Cheat Protection");
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    RichText::new("This title is protected by Valve Anti-Cheat (VAC).")
                        .font(FontId::proportional(12.5))
                        .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                );

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "While simulating a process session in the background does not inject into game memory, you should never join VAC-secured multiplayer servers simultaneously or run external game modification tools while idling.",
                    )
                    .font(FontId::proportional(11.5))
                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                );

                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.0)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([80.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_vac_info_modal = false;
        }
    }

    fn render_ac_info_modal(&mut self, ctx: &egui::Context) {
        if !self.show_ac_info_modal {
            return;
        }

        let mut close_modal = false;

        egui::Window::new("Third-Party Anti-Cheat Information")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(500.0, 260.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xD9, 0x77, 0x06)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    render_badge(ui, "AC", Color32::from_rgb(0xD9, 0x77, 0x06));
                    ui.add_space(6.0);
                    ui.heading("Third-Party Anti-Cheat Protection");
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    RichText::new("This game appears to use a third-party anti-cheat system (such as Easy Anti-Cheat, BattlEye, or similar).\n\nNote: Unlike VAC (which is detected with 100% accuracy directly from Steam metadata), third-party anti-cheat detection relies on local heuristics and may not identify every title. Always avoid running idling sessions alongside active anti-cheat protected multiplayer games.")
                        .font(FontId::proportional(12.5))
                        .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                );

                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.0)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([80.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_ac_info_modal = false;
        }
    }

    fn render_edit_game_modal(&mut self, ctx: &egui::Context) {
        if !self.show_edit_game_modal {
            return;
        }

        let mut close_modal = false;
        let mut save_game = false;
        let mut reset_game = false;

        let title = if self.is_editing_custom {
            "Add Custom Game"
        } else {
            "Edit Game Executable"
        };

        if self.editing_game.is_some() && self.edit_exe_path.is_empty() {
            if let Some(ref g) = self.editing_game {
                self.edit_exe_path = g.executable_relative_path.clone();
                self.edit_game_name = g.name.clone();
                self.edit_app_id_str = g.app_id.to_string();
            }
        }

        egui::Window::new(title)
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(500.0, 340.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                if self.is_editing_custom {
                    ui.label("Game Title:");
                    ui.add_sized(
                        [ui.available_width(), 28.0],
                        egui::TextEdit::singleline(&mut self.edit_game_name),
                    );
                    ui.add_space(8.0);

                    ui.label("Steam AppID (number):");
                    ui.add_sized(
                        [ui.available_width(), 28.0],
                        egui::TextEdit::singleline(&mut self.edit_app_id_str),
                    );
                    ui.add_space(8.0);
                } else if let Some(ref g) = self.editing_game {
                    ui.label(
                        RichText::new(&g.name)
                            .font(FontId::proportional(15.0))
                            .color(Color32::WHITE)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("AppID: {}", g.app_id))
                            .font(FontId::proportional(12.0))
                            .color(Color32::from_rgb(0x8F, 0x98, 0xA0)),
                    );
                    ui.add_space(10.0);
                }

                ui.label("Relative Executable Path (e.g., cs2.exe or Binaries\\Win64\\Game.exe):");
                ui.add_sized(
                    [ui.available_width(), 28.0],
                    egui::TextEdit::singleline(&mut self.edit_exe_path),
                );

                if let Some(ref g) = self.editing_game {
                    if g.available_executables.len() > 1 {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Discovered Executables:").font(FontId::proportional(11.5)));
                        for exe in &g.available_executables {
                            if ui.button(exe).clicked() {
                                self.edit_exe_path = exe.clone();
                            }
                        }
                    }
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if !self.is_editing_custom {
                        let reset_btn = egui::Button::new(
                            RichText::new("Reset to Default")
                                .color(Color32::from_rgb(0xDC, 0x26, 0x26))
                                .font(FontId::proportional(12.0)),
                        )
                        .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x4A, 0x22, 0x22)))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([120.0, 32.0], reset_btn).on_hover_text("Revert to default Steam executable and remove custom override").clicked() {
                            reset_game = true;
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save_btn = egui::Button::new(
                            RichText::new("Save")
                                .color(Color32::WHITE)
                                .font(FontId::proportional(12.5))
                                .strong(),
                        )
                        .fill(Color32::from_rgb(0x5C, 0x7E, 0x10))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([80.0, 32.0], save_btn).clicked() {
                            save_game = true;
                        }

                        ui.add_space(6.0);

                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel")
                                .color(Color32::WHITE)
                                .font(FontId::proportional(12.0)),
                        )
                        .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([80.0, 32.0], cancel_btn).clicked() {
                            close_modal = true;
                        }
                    });
                });
            });

        if reset_game {
            if let Some(mut g) = self.editing_game.take() {
                remove_override(g.app_id);
                if !g.default_executable.is_empty() {
                    g.executable_relative_path = g.default_executable.clone();
                }
                if let Some(existing) = self.games.iter_mut().find(|x| x.app_id == g.app_id) {
                    existing.executable_relative_path = g.executable_relative_path.clone();
                }
                self.set_status(&format!("Reset {} to default executable.", g.name));
            }
            self.show_edit_game_modal = false;
            self.edit_exe_path.clear();
            self.edit_game_name.clear();
            self.edit_app_id_str.clear();
        }

        if save_game {
            if self.is_editing_custom {
                let app_id = self.edit_app_id_str.trim().parse::<u32>().unwrap_or(0);
                if app_id > 0 && !self.edit_game_name.trim().is_empty() {
                    let mut game = SteamGame::new(app_id, self.edit_game_name.trim().to_string());
                    game.executable_relative_path = self.edit_exe_path.trim().to_string();
                    game.default_executable = self.edit_exe_path.trim().to_string();
                    game.available_executables = vec![game.executable_relative_path.clone()];
                    game.is_custom = true;
                    save_or_update_override(&game);
                    self.custom_games.retain(|g| g.app_id != app_id);
                    self.custom_games.push(game);
                    self.set_status("Custom game added.");
                }
            } else if let Some(mut g) = self.editing_game.take() {
                g.executable_relative_path = self.edit_exe_path.trim().to_string();
                save_or_update_override(&g);
                if let Some(existing) = self.games.iter_mut().find(|x| x.app_id == g.app_id) {
                    existing.executable_relative_path = g.executable_relative_path.clone();
                }
                self.set_status(&format!("Updated executable for {}.", g.name));
            }
            self.show_edit_game_modal = false;
            self.edit_exe_path.clear();
            self.edit_game_name.clear();
            self.edit_app_id_str.clear();
        }

        if close_modal {
            self.show_edit_game_modal = false;
            self.editing_game = None;
            self.edit_exe_path.clear();
            self.edit_game_name.clear();
            self.edit_app_id_str.clear();
        }
    }

    fn render_vac_safety_modal(&mut self, ctx: &egui::Context) {
        if self.pending_vac_notice_game.is_none() {
            return;
        }

        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("VAC Safety Notice")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(440.0, 230.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xDC, 0x26, 0x26)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    render_badge(ui, "VAC", Color32::from_rgb(0xDC, 0x26, 0x26));
                    ui.add_space(6.0);
                    ui.heading("Valve Anti-Cheat Notice");
                });

                ui.add_space(10.0);
                ui.label(
                    "This game uses Valve Anti-Cheat (VAC). While background simulation does not modify game files, do not launch multiplayer matches while idling.",
                );

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    let confirm_btn = egui::Button::new(
                        RichText::new("I Understand, Launch")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.0))
                            .strong(),
                    )
                    .fill(Color32::from_rgb(0xDC, 0x26, 0x26))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([160.0, 32.0], confirm_btn).clicked() {
                        confirmed = true;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel")
                                .color(Color32::WHITE)
                                .font(FontId::proportional(12.0)),
                        )
                        .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([80.0, 32.0], cancel_btn).clicked() {
                            cancelled = true;
                        }
                    });
                });
            });

        if confirmed {
            if let Some(game) = self.pending_vac_notice_game.take() {
                self.execute_launch_game_with_duration(&game, None);
            }
        }
        if cancelled {
            self.pending_vac_notice_game = None;
        }
    }

    fn render_ac_safety_modal(&mut self, ctx: &egui::Context) {
        if self.pending_ac_notice_game.is_none() {
            return;
        }

        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("Anti-Cheat Safety Notice")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(440.0, 200.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xD9, 0x77, 0x06)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    render_badge(ui, "AC", Color32::from_rgb(0xD9, 0x77, 0x06));
                    ui.add_space(6.0);
                    ui.heading("Third-Party Anti-Cheat Notice");
                });

                ui.add_space(10.0);
                ui.label(
                    "This game uses third-party Anti-Cheat (EAC / BattlEye). Simulated processes run safely in background sandboxes without injecting code.",
                );

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    let confirm_btn = egui::Button::new(
                        RichText::new("I Understand, Launch")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.0))
                            .strong(),
                    )
                    .fill(Color32::from_rgb(0xD9, 0x77, 0x06))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([160.0, 32.0], confirm_btn).clicked() {
                        confirmed = true;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let cancel_btn = egui::Button::new(
                            RichText::new("Cancel")
                                .color(Color32::WHITE)
                                .font(FontId::proportional(12.0)),
                        )
                        .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                        .corner_radius(CornerRadius::same(6));

                        if ui.add_sized([80.0, 32.0], cancel_btn).clicked() {
                            cancelled = true;
                        }
                    });
                });
            });

        if confirmed {
            if let Some(game) = self.pending_ac_notice_game.take() {
                self.execute_launch_game_with_duration(&game, None);
            }
        }
        if cancelled {
            self.pending_ac_notice_game = None;
        }
    }

    fn render_history_modal(&mut self, ctx: &egui::Context) {
        if !self.show_history_modal {
            return;
        }

        let mut close_modal = false;
        egui::Window::new("Session History & Statistics")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(520.0, 440.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.heading("Idler Lifetime Summary");
                ui.add_space(8.0);

                let total_secs: u64 = self
                    .history_store
                    .games
                    .values()
                    .map(|g| g.total_idled_seconds)
                    .sum();
                let total_sessions: u32 = self
                    .history_store
                    .games
                    .values()
                    .map(|g| g.session_count)
                    .sum();
                let total_titles = self.history_store.games.len();

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Total Idled Time:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                    ui.label(
                        RichText::new(format_duration_human(total_secs))
                            .color(Color32::from_rgb(0x66, 0xC0, 0xF4))
                            .strong(),
                    );
                    ui.add_space(16.0);
                    ui.label(RichText::new("Sessions:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                    ui.label(RichText::new(total_sessions.to_string()).color(Color32::WHITE).strong());
                    ui.add_space(16.0);
                    ui.label(RichText::new("Games:").color(Color32::from_rgb(0x8F, 0x98, 0xA0)));
                    ui.label(RichText::new(total_titles.to_string()).color(Color32::WHITE).strong());
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.heading("Recent Sessions");
                ui.add_space(6.0);

                if self.history_store.recent_sessions.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("No session history recorded yet.")
                                .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                .font(FontId::proportional(13.0)),
                        );
                    });
                } else {
                    ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                        for session in &self.history_store.recent_sessions {
                            Frame::NONE
                                .fill(Color32::from_rgb(0x16, 0x22, 0x30))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x23, 0x38, 0x4D)))
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(&session.game_name)
                                                .color(Color32::from_rgb(0xE5, 0xEC, 0xEE))
                                                .font(FontId::proportional(13.0))
                                                .strong(),
                                        );
                                        ui.label(
                                            RichText::new(format!("(AppID {})", session.app_id))
                                                .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                                .font(FontId::proportional(11.0)),
                                        );
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(
                                                RichText::new(format_timestamp_human(session.timestamp_unix))
                                                    .color(Color32::from_rgb(0x8F, 0x98, 0xA0))
                                                    .font(FontId::proportional(11.0)),
                                            );
                                            ui.add_space(12.0);
                                            ui.label(
                                                RichText::new(format_duration_human(session.duration_seconds))
                                                    .color(Color32::from_rgb(0x66, 0xC0, 0xF4))
                                                    .font(FontId::proportional(12.0))
                                                    .strong(),
                                            );
                                        });
                                    });
                                });
                            ui.add_space(3.0);
                        }
                    });
                }

                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.5)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([100.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_history_modal = false;
        }
    }

    fn render_duplicate_process_modal(&mut self, ctx: &egui::Context) {
        if self.show_duplicate_process_modal.is_none() {
            return;
        }

        let mut close_modal = false;
        egui::Window::new("Game Already Running")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(440.0, 180.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.heading("Duplicate Game Detected");
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Cannot start idling because this game is already running on your computer. Please close the game first.")
                        .font(FontId::proportional(13.0))
                        .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                );
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.5)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([100.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_duplicate_process_modal = None;
        }
    }

    fn render_vac_shield_blocked_modal(&mut self, ctx: &egui::Context) {
        if self.show_vac_shield_blocked_modal.is_none() {
            return;
        }

        let mut close_modal = false;
        egui::Window::new("VAC / Anti-Cheat Shield Protection")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(460.0, 210.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xDC, 0x26, 0x26)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.heading("Launch Blocked by Safety Shield");
                ui.add_space(8.0);
                ui.label(
                    RichText::new("A protected online game or anti-cheat service is currently active on your system.\n\nFor the safety of your Steam account, launching VAC or anti-cheat protected titles is locked while anti-cheat systems are running.")
                        .font(FontId::proportional(13.0))
                        .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                );
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("Close")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.5)),
                    )
                    .fill(Color32::from_rgb(0x21, 0x2C, 0x3D))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([100.0, 32.0], close_btn).clicked() {
                        close_modal = true;
                    }
                });
            });

        if close_modal {
            self.show_vac_shield_blocked_modal = None;
        }
    }

    fn render_vac_shield_disable_warning_modal(&mut self, ctx: &egui::Context) {
        if !self.show_vac_shield_disable_warning_modal {
            return;
        }

        egui::Window::new("Disable VAC & Anti-Cheat Shield")
            .order(Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(480.0, 240.0))
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(0x1B, 0x28, 0x38))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xDC, 0x26, 0x26)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.heading("Disable Safety Protection?");
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Disabling the VAC & Anti-Cheat Safety Shield removes automatic protection against running VAC or anti-cheat protected idler sessions alongside active online games.\n\nRunning simulated processes alongside VAC or third-party anti-cheat systems may put your Steam account at risk.\n\nAre you sure you want to disable this protection?")
                        .font(FontId::proportional(12.5))
                        .color(Color32::from_rgb(0xE5, 0xEC, 0xEE)),
                );
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let keep_btn = egui::Button::new(
                        RichText::new("Keep Enabled")
                            .color(Color32::WHITE)
                            .font(FontId::proportional(12.5))
                            .strong(),
                    )
                    .fill(Color32::from_rgb(0x27, 0x6E, 0x3E))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x3B, 0x93, 0x54)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([120.0, 32.0], keep_btn).clicked() {
                        self.show_vac_shield_disable_warning_modal = false;
                    }

                    ui.add_space(10.0);

                    let disable_btn = egui::Button::new(
                        RichText::new("Disable Anyway")
                            .color(Color32::from_rgb(0xFF, 0xA0, 0xA0))
                            .font(FontId::proportional(12.5)),
                    )
                    .fill(Color32::from_rgb(0x3D, 0x1E, 0x1E))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x7A, 0x2A, 0x2A)))
                    .corner_radius(CornerRadius::same(6));

                    if ui.add_sized([130.0, 32.0], disable_btn).clicked() {
                        self.settings.vac_shield_enabled = false;
                        save_settings(&self.settings);
                        self.show_vac_shield_disable_warning_modal = false;
                        self.set_status("VAC & Anti-Cheat Shield disabled.");
                    }
                });
            });
    }
}

fn toggle_switch(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let desired_size = vec2(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let bg_color = if *on {
        Color32::from_rgb(0x5C, 0x7E, 0x10)
    } else {
        Color32::from_rgb(0x2A, 0x38, 0x4D)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(10), bg_color);

    let circle_x = if *on {
        rect.max.x - 10.0
    } else {
        rect.min.x + 10.0
    };

    let circle_center = pos2(circle_x, rect.center().y);
    ui.painter().circle_filled(circle_center, 8.0, Color32::WHITE);

    response
}

fn filter_pill(ui: &mut Ui, label: &str, active: &mut bool) -> egui::Response {
    let bg = if *active {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    } else {
        Color32::from_rgb(0x1B, 0x28, 0x38)
    };
    let stroke = Stroke::new(
        1.0_f32,
        if *active {
            Color32::from_rgb(0x66, 0xC0, 0xF4)
        } else {
            Color32::from_rgb(0x2A, 0x47, 0x5E)
        },
    );
    let text_color = if *active {
        Color32::WHITE
    } else {
        Color32::from_rgb(0x8F, 0x98, 0xA0)
    };

    let btn = egui::Button::new(
        RichText::new(label)
            .color(text_color)
            .font(FontId::proportional(12.0)),
    )
    .fill(bg)
    .stroke(stroke)
    .corner_radius(CornerRadius::same(6));

    let res = ui.add_sized([90.0, 28.0], btn);
    if res.clicked() {
        *active = !*active;
    }
    res
}

fn render_badge(ui: &mut Ui, text: &str, color: Color32) {
    let font_id = FontId::proportional(10.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, Color32::WHITE);
    let padding = vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    ui.painter().rect_filled(rect, CornerRadius::same(4), color);
    let text_pos = pos2(rect.min.x + padding.x, rect.min.y + padding.y);
    ui.painter().galley(text_pos, galley, Color32::WHITE);
}

fn render_clickable_badge(ui: &mut Ui, text: &str, color: Color32, hover_text: &str) -> egui::Response {
    let font_id = FontId::proportional(10.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, Color32::WHITE);
    let padding = vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let is_hovered = response.hovered();
    let bg_color = if is_hovered {
        let r = color.r().saturating_add(25);
        let g = color.g().saturating_add(25);
        let b = color.b().saturating_add(25);
        Color32::from_rgb(r, g, b)
    } else {
        color
    };

    ui.painter().rect_filled(rect, CornerRadius::same(4), bg_color);
    if is_hovered {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0_f32, Color32::WHITE),
            StrokeKind::Inside,
        );
    }
    let text_pos = pos2(rect.min.x + padding.x, rect.min.y + padding.y);
    ui.painter().galley(text_pos, galley, Color32::WHITE);

    response.on_hover_text(hover_text)
}

fn render_shield_badge(
    ui: &mut Ui,
    text: &str,
    bg_color: Color32,
    text_color: Color32,
    hover_text: &str,
    is_clickable: bool,
) -> egui::Response {
    let font_id = FontId::proportional(10.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, text_color);
    let padding = vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_clickable {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );

    let is_hovered = response.hovered();
    let effective_bg = if is_clickable && is_hovered {
        Color32::from_rgb(0xB9, 0x1C, 0x1C)
    } else {
        bg_color
    };

    ui.painter().rect_filled(rect, CornerRadius::same(4), effective_bg);
    if is_clickable && is_hovered {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0_f32, Color32::WHITE),
            StrokeKind::Inside,
        );
    }
    let text_pos = pos2(rect.min.x + padding.x, rect.min.y + padding.y);
    ui.painter().galley(text_pos, galley, text_color);

    response.on_hover_text(hover_text)
}

fn render_counter_pill(ui: &mut Ui, current: usize, total: usize) {
    let text = format!("{}/{}", current, total);
    let font_id = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text, font_id, Color32::from_rgb(0x66, 0xC0, 0xF4));
    let padding = vec2(10.0, 5.0);
    let size = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        Color32::from_rgb(0x1E, 0x2D, 0x3D),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)),
        StrokeKind::Inside,
    );
    let text_pos = pos2(rect.min.x + padding.x, rect.min.y + padding.y);
    ui.painter().galley(text_pos, galley, Color32::from_rgb(0x66, 0xC0, 0xF4));
}

fn render_elapsed_badge(ui: &mut Ui, text: &str) {
    let font_id = FontId::proportional(11.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, Color32::from_rgb(0x57, 0xCB, 0x58));
    let padding = vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    ui.painter().rect_filled(
        rect,
        CornerRadius::same(4),
        Color32::from_rgb(0x1B, 0x33, 0x22),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, Color32::from_rgb(0x2E, 0x59, 0x3A)),
        StrokeKind::Inside,
    );
    let text_pos = pos2(rect.min.x + padding.x, rect.min.y + padding.y);
    ui.painter().galley(text_pos, galley, Color32::from_rgb(0x57, 0xCB, 0x58));
}

fn render_tab_button(ui: &mut Ui, label: &str, is_active: bool) -> egui::Response {
    let bg = if is_active {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    } else {
        Color32::from_rgb(0x1B, 0x28, 0x38)
    };
    let text_color = if is_active {
        Color32::WHITE
    } else {
        Color32::from_rgb(0x8F, 0x98, 0xA0)
    };

    let btn = egui::Button::new(
        RichText::new(label)
            .color(text_color)
            .font(FontId::proportional(12.0))
            .strong(),
    )
    .fill(bg)
    .corner_radius(CornerRadius::same(6));

    ui.add_sized([100.0, 28.0], btn)
}

fn render_top_bar_button(ui: &mut Ui, label: &str, size: [f32; 2], is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(size[0], size[1]),
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else if is_enabled {
        Color32::from_rgb(0x32, 0x52, 0x70)
    } else {
        Color32::from_rgb(0x22, 0x30, 0x40)
    };
    let text_color = if is_enabled {
        if is_hovered {
            Color32::WHITE
        } else {
            Color32::from_rgb(0xE5, 0xEC, 0xEE)
        }
    } else {
        Color32::from_rgb(0x6B, 0x72, 0x80)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, stroke_color),
        StrokeKind::Inside,
    );

    let font_id = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font_id, text_color);
    let text_pos = pos2(
        rect.center().x - galley.size().x * 0.5_f32,
        rect.center().y - galley.size().y * 0.5_f32,
    );
    ui.painter().galley(text_pos, galley, text_color);

    response
}

fn render_launch_button(ui: &mut Ui, size: egui::Vec2, text: &str, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if !is_enabled {
        Color32::from_rgb(0x3B, 0x4D, 0x22)
    } else if is_hovered {
        Color32::from_rgb(0x68, 0x8F, 0x12)
    } else {
        Color32::from_rgb(0x5C, 0x7E, 0x10)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);

    let content_color = if is_enabled { Color32::WHITE } else { Color32::from_rgb(0x8F, 0x98, 0xA0) };
    let cy = rect.center().y;
    let icon_w = 7.0_f32;
    let tri_half_h = 4.5_f32;
    let gap = 6.0_f32;

    let font_id = FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, content_color);
    let total_group_w = icon_w + gap + galley.size().x;
    let start_x = rect.center().x - total_group_w * 0.5;

    let p1 = pos2(start_x, cy - tri_half_h);
    let p2 = pos2(start_x + icon_w, cy);
    let p3 = pos2(start_x, cy + tri_half_h);
    ui.painter().add(egui::Shape::convex_polygon(vec![p1, p2, p3], content_color, Stroke::NONE));

    let text_pos = pos2(start_x + icon_w + gap, cy - galley.size().y * 0.5);
    ui.painter().galley(text_pos, galley, content_color);

    response
}

fn render_stop_button(ui: &mut Ui, size: egui::Vec2, text: &str, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if !is_enabled {
        Color32::from_rgb(0x4A, 0x22, 0x22)
    } else if is_hovered {
        Color32::from_rgb(0xA3, 0x36, 0x36)
    } else {
        Color32::from_rgb(0x8A, 0x2E, 0x2E)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);

    let content_color = if is_enabled { Color32::WHITE } else { Color32::from_rgb(0x8F, 0x98, 0xA0) };
    let cy = rect.center().y;
    let sq_size = 8.0_f32;
    let gap = 6.0_f32;

    let font_id = FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, content_color);
    let total_group_w = sq_size + gap + galley.size().x;
    let start_x = rect.center().x - total_group_w * 0.5;

    let sq_rect = egui::Rect::from_min_size(pos2(start_x, cy - sq_size * 0.5), vec2(sq_size, sq_size));
    ui.painter().rect_filled(sq_rect, CornerRadius::same(1), content_color);

    let text_pos = pos2(start_x + sq_size + gap, cy - galley.size().y * 0.5);
    ui.painter().galley(text_pos, galley, content_color);

    response
}

fn render_edit_text_button(ui: &mut Ui, size: egui::Vec2, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x2A, 0x38, 0x4D)
    } else {
        Color32::from_rgb(0x21, 0x2C, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else if is_enabled {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    } else {
        Color32::from_rgb(0x1F, 0x2A, 0x36)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let content_color = if is_enabled {
        if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xE5, 0xEC, 0xEE) }
    } else {
        Color32::from_rgb(0x55, 0x67, 0x72)
    };

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Edit",
        FontId::proportional(11.5),
        content_color,
    );

    response
}

fn render_edit_icon_button(ui: &mut Ui, size: egui::Vec2, corner_radius: u8, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x28, 0x3D, 0x55)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else if is_enabled {
        Color32::from_rgb(0x32, 0x52, 0x70)
    } else {
        Color32::from_rgb(0x1F, 0x2A, 0x36)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(corner_radius), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(corner_radius), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let content_color = if is_enabled {
        if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xE5, 0xEC, 0xEE) }
    } else {
        Color32::from_rgb(0x55, 0x67, 0x72)
    };

    let c = rect.center();
    let p_tip = pos2(c.x - 4.2, c.y + 3.2);
    let p_mid = pos2(c.x - 1.8, c.y + 0.8);
    let p_top = pos2(c.x + 3.8, c.y - 4.8);

    ui.painter().line_segment(
        [p_mid, p_top],
        Stroke::new(2.4_f32, content_color),
    );
    ui.painter().line_segment(
        [p_tip, p_mid],
        Stroke::new(1.4_f32, content_color),
    );
    ui.painter().line_segment(
        [pos2(c.x - 4.5, c.y + 5.2), pos2(c.x + 4.5, c.y + 5.2)],
        Stroke::new(1.4_f32, content_color),
    );

    response.on_hover_text("Edit executable")
}

fn render_plus_icon_button(ui: &mut Ui, size: egui::Vec2, corner_radius: u8, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x28, 0x3D, 0x55)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else if is_enabled {
        Color32::from_rgb(0x32, 0x52, 0x70)
    } else {
        Color32::from_rgb(0x1F, 0x2A, 0x36)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(corner_radius), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(corner_radius), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let icon_color = if is_enabled {
        if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xE5, 0xEC, 0xEE) }
    } else {
        Color32::from_rgb(0x55, 0x67, 0x72)
    };

    let c = rect.center();
    let arm = 5.0_f32;
    ui.painter().line_segment(
        [pos2(c.x - arm, c.y), pos2(c.x + arm, c.y)],
        Stroke::new(1.8_f32, icon_color),
    );
    ui.painter().line_segment(
        [pos2(c.x, c.y - arm), pos2(c.x, c.y + arm)],
        Stroke::new(1.8_f32, icon_color),
    );

    response
}

fn render_cross_button(ui: &mut Ui, size: egui::Vec2, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x8A, 0x2E, 0x2E)
    } else {
        Color32::from_rgb(0x21, 0x2C, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0xDC, 0x26, 0x26)
    } else {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(4), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let icon_color = if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xC7, 0xD5, 0xE0) };
    let c = rect.center();
    let d = 3.5_f32;
    ui.painter().line_segment(
        [pos2(c.x - d, c.y - d), pos2(c.x + d, c.y + d)],
        Stroke::new(1.6_f32, icon_color),
    );
    ui.painter().line_segment(
        [pos2(c.x - d, c.y + d), pos2(c.x + d, c.y - d)],
        Stroke::new(1.6_f32, icon_color),
    );

    response
}

fn render_arrow_up_button(ui: &mut Ui, size: egui::Vec2, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x28, 0x3D, 0x55)
    } else {
        Color32::from_rgb(0x21, 0x2C, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(4), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let icon_color = if is_enabled {
        if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xC7, 0xD5, 0xE0) }
    } else {
        Color32::from_rgb(0x4A, 0x5A, 0x6A)
    };
    let c = rect.center();
    let p1 = pos2(c.x, c.y - 3.5);
    let p2 = pos2(c.x - 3.5, c.y + 2.5);
    let p3 = pos2(c.x + 3.5, c.y + 2.5);
    ui.painter().add(egui::Shape::convex_polygon(vec![p1, p2, p3], icon_color, Stroke::NONE));

    response
}

fn render_arrow_down_button(ui: &mut Ui, size: egui::Vec2, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x28, 0x3D, 0x55)
    } else {
        Color32::from_rgb(0x21, 0x2C, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(4), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let icon_color = if is_enabled {
        if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xC7, 0xD5, 0xE0) }
    } else {
        Color32::from_rgb(0x4A, 0x5A, 0x6A)
    };
    let c = rect.center();
    let p1 = pos2(c.x, c.y + 3.5);
    let p2 = pos2(c.x - 3.5, c.y - 2.5);
    let p3 = pos2(c.x + 3.5, c.y - 2.5);
    ui.painter().add(egui::Shape::convex_polygon(vec![p1, p2, p3], icon_color, Stroke::NONE));

    response
}

fn render_panel_collapse_button(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
    let is_hovered = response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x2A, 0x38, 0x4D)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else {
        Color32::from_rgb(0x32, 0x52, 0x70)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);
    ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let icon_color = if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xC7, 0xD5, 0xE0) };
    let c = rect.center();
    ui.painter().line_segment(
        [pos2(c.x - 2.5, c.y - 4.5), pos2(c.x + 2.0, c.y)],
        Stroke::new(1.8_f32, icon_color),
    );
    ui.painter().line_segment(
        [pos2(c.x + 2.0, c.y), pos2(c.x - 2.5, c.y + 4.5)],
        Stroke::new(1.8_f32, icon_color),
    );

    response.on_hover_text("Hide Panel")
}

fn render_panel_expand_handle(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 32.0), egui::Sense::click());
    let is_hovered = response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x2A, 0x38, 0x4D)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else {
        Color32::from_rgb(0x32, 0x52, 0x70)
    };

    let corner_radius = CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 };
    ui.painter().rect_filled(rect, corner_radius, bg_color);
    ui.painter().rect_stroke(rect, corner_radius, Stroke::new(1.0_f32, stroke_color), StrokeKind::Inside);

    let icon_color = if is_hovered { Color32::WHITE } else { Color32::from_rgb(0xC7, 0xD5, 0xE0) };
    let c = rect.center();
    ui.painter().line_segment(
        [pos2(c.x + 2.0, c.y - 4.5), pos2(c.x - 2.5, c.y)],
        Stroke::new(1.8_f32, icon_color),
    );
    ui.painter().line_segment(
        [pos2(c.x - 2.5, c.y), pos2(c.x + 2.0, c.y + 4.5)],
        Stroke::new(1.8_f32, icon_color),
    );

    response.on_hover_text("Show Side Panel")
}

fn render_gear_button(ui: &mut Ui, is_open: bool, is_enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(34.0, 34.0),
        if is_enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let is_hovered = is_enabled && response.hovered();
    let is_active = is_open;

    let bg_color = if is_active || is_hovered {
        Color32::from_rgb(0x2A, 0x47, 0x5E)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_active || is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else if is_enabled {
        Color32::from_rgb(0x32, 0x52, 0x70)
    } else {
        Color32::from_rgb(0x22, 0x30, 0x40)
    };
    let gear_color = if is_enabled {
        if is_active || is_hovered {
            Color32::WHITE
        } else {
            Color32::from_rgb(0xE5, 0xEC, 0xEE)
        }
    } else {
        Color32::from_rgb(0x6B, 0x72, 0x80)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, stroke_color),
        StrokeKind::Inside,
    );

    let center = rect.center();
    let outer_radius = 8.5_f32;
    let inner_radius = 6.0_f32;
    let hole_radius = 3.0_f32;
    let teeth = 8;

    for i in 0..teeth {
        let angle = (i as f32) * (std::f32::consts::TAU / (teeth as f32));
        let tooth_w = 0.22_f32;
        let p1 = pos2(center.x + (angle - tooth_w).cos() * inner_radius, center.y + (angle - tooth_w).sin() * inner_radius);
        let p2 = pos2(center.x + (angle - tooth_w * 0.7).cos() * outer_radius, center.y + (angle - tooth_w * 0.7).sin() * outer_radius);
        let p3 = pos2(center.x + (angle + tooth_w * 0.7).cos() * outer_radius, center.y + (angle + tooth_w * 0.7).sin() * outer_radius);
        let p4 = pos2(center.x + (angle + tooth_w).cos() * inner_radius, center.y + (angle + tooth_w).sin() * inner_radius);

        ui.painter().add(egui::Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            gear_color,
            Stroke::NONE,
        ));
    }

    ui.painter().circle_filled(center, inner_radius, gear_color);
    ui.painter().circle_filled(center, hole_radius, bg_color);

    response.on_hover_text("Settings")
}

fn render_view_mode_buttons(ui: &mut Ui, mode: &mut ViewMode) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(68.0, 34.0), egui::Sense::click());
    let left_rect = egui::Rect::from_min_max(rect.min, pos2(rect.center().x, rect.max.y));
    let right_rect = egui::Rect::from_min_max(pos2(rect.center().x, rect.min.y), rect.max);

    let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
    let clicked = ui.ctx().input(|i| i.pointer.primary_clicked());

    if let Some(pos) = pointer_pos {
        if clicked {
            if left_rect.contains(pos) && *mode != ViewMode::List {
                *mode = ViewMode::List;
                response.mark_changed();
            } else if right_rect.contains(pos) && *mode != ViewMode::Grid {
                *mode = ViewMode::Grid;
                response.mark_changed();
            }
        }
    }

    ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(0x1E, 0x2D, 0x3D));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, Color32::from_rgb(0x32, 0x52, 0x70)),
        StrokeKind::Inside,
    );

    let is_list = *mode == ViewMode::List;
    if is_list {
        ui.painter().rect_filled(
            left_rect,
            CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 },
            Color32::from_rgb(0x2A, 0x47, 0x5E),
        );
    } else {
        ui.painter().rect_filled(
            right_rect,
            CornerRadius { nw: 0, sw: 0, ne: 6, se: 6 },
            Color32::from_rgb(0x2A, 0x47, 0x5E),
        );
    }

    let list_color = if is_list { Color32::WHITE } else { Color32::from_rgb(0x8F, 0x98, 0xA0) };
    let lc = left_rect.center();
    ui.painter().line_segment([pos2(lc.x - 7.0, lc.y - 4.5), pos2(lc.x + 7.0, lc.y - 4.5)], Stroke::new(1.8_f32, list_color));
    ui.painter().line_segment([pos2(lc.x - 7.0, lc.y), pos2(lc.x + 7.0, lc.y)], Stroke::new(1.8_f32, list_color));
    ui.painter().line_segment([pos2(lc.x - 7.0, lc.y + 4.5), pos2(lc.x + 7.0, lc.y + 4.5)], Stroke::new(1.8_f32, list_color));

    let grid_color = if !is_list { Color32::WHITE } else { Color32::from_rgb(0x8F, 0x98, 0xA0) };
    let rc = right_rect.center();
    let sq_size = 4.5_f32;
    let gap = 2.0_f32;
    ui.painter().rect_filled(egui::Rect::from_min_size(pos2(rc.x - sq_size - gap * 0.5, rc.y - sq_size - gap * 0.5), vec2(sq_size, sq_size)), CornerRadius::ZERO, grid_color);
    ui.painter().rect_filled(egui::Rect::from_min_size(pos2(rc.x + gap * 0.5, rc.y - sq_size - gap * 0.5), vec2(sq_size, sq_size)), CornerRadius::ZERO, grid_color);
    ui.painter().rect_filled(egui::Rect::from_min_size(pos2(rc.x - sq_size - gap * 0.5, rc.y + gap * 0.5), vec2(sq_size, sq_size)), CornerRadius::ZERO, grid_color);
    ui.painter().rect_filled(egui::Rect::from_min_size(pos2(rc.x + gap * 0.5, rc.y + gap * 0.5), vec2(sq_size, sq_size)), CornerRadius::ZERO, grid_color);

    response.on_hover_text(if is_list { "List View (Click right for Grid)" } else { "Grid View (Click left for List)" })
}

fn render_icon_placeholder(ui: &mut Ui, size: egui::Vec2, title: &str) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        Color32::from_rgb(0x2A, 0x47, 0x5E),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, Color32::from_rgb(0x3B, 0x64, 0x8C)),
        StrokeKind::Inside,
    );

    let initials = extract_initials(title);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initials,
        FontId::proportional(14.0),
        Color32::WHITE,
    );
}

fn render_banner_placeholder_at(ui: &mut Ui, rect: egui::Rect, title: &str) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        Color32::from_rgb(0x18, 0x24, 0x33),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x47, 0x5E)),
        StrokeKind::Inside,
    );

    let center = rect.center();
    let circle_center = pos2(center.x, center.y - 10.0);
    let radius = 22.0_f32;

    ui.painter().circle_filled(
        circle_center,
        radius,
        Color32::from_rgb(0x24, 0x38, 0x4E),
    );
    ui.painter().circle_stroke(
        circle_center,
        radius,
        Stroke::new(1.5_f32, Color32::from_rgb(0x66, 0xC0, 0xF4)),
    );

    let initials = extract_initials(title);
    ui.painter().text(
        circle_center,
        Align2::CENTER_CENTER,
        initials,
        FontId::proportional(15.0),
        Color32::WHITE,
    );

    let title_cut = if title.len() > 26 {
        format!("{}...", &title[..24])
    } else {
        title.to_string()
    };

    ui.painter().text(
        pos2(center.x, center.y + 24.0),
        Align2::CENTER_CENTER,
        title_cut,
        FontId::proportional(11.5),
        Color32::from_rgb(0xC7, 0xD5, 0xE0),
    );
}

fn extract_initials(title: &str) -> String {
    let clean: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let words: Vec<&str> = clean.split_whitespace().collect();
    if words.is_empty() {
        return "SP".to_string();
    }
    if words.len() >= 2 {
        let first = words[0].chars().next().unwrap_or('?').to_ascii_uppercase();
        let second = words[1].chars().next().unwrap_or('?').to_ascii_uppercase();
        format!("{}{}", first, second)
    } else {
        let mut chars = words[0].chars();
        let first = chars.next().unwrap_or('?').to_ascii_uppercase();
        let second = chars.next().unwrap_or(' ').to_ascii_uppercase();
        if second != ' ' {
            format!("{}{}", first, second)
        } else {
            format!("{}", first)
        }
    }
}

fn render_empty_sessions_icon(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), egui::Sense::hover());
    let center = rect.center();
    let stroke_color = Color32::from_rgb(0x3B, 0x4E, 0x63);
    let painter = ui.painter();

    let body_rect = egui::Rect::from_center_size(center, vec2(32.0, 20.0));
    painter.rect_stroke(
        body_rect,
        CornerRadius::same(6),
        Stroke::new(1.5_f32, stroke_color),
        StrokeKind::Inside,
    );

    let dpad_cx = center.x - 7.5;
    let dpad_cy = center.y;
    painter.line_segment(
        [pos2(dpad_cx - 4.0, dpad_cy), pos2(dpad_cx + 4.0, dpad_cy)],
        Stroke::new(1.5_f32, stroke_color),
    );
    painter.line_segment(
        [pos2(dpad_cx, dpad_cy - 4.0), pos2(dpad_cx, dpad_cy + 4.0)],
        Stroke::new(1.5_f32, stroke_color),
    );

    let btn_cx = center.x + 7.5;
    let btn_cy = center.y;
    painter.circle_filled(pos2(btn_cx - 3.0, btn_cy), 1.2, stroke_color);
    painter.circle_filled(pos2(btn_cx + 3.0, btn_cy), 1.2, stroke_color);
    painter.circle_filled(pos2(btn_cx, btn_cy - 3.0), 1.2, stroke_color);
    painter.circle_filled(pos2(btn_cx, btn_cy + 3.0), 1.2, stroke_color);
}

fn load_ico_best_frame(bytes: &[u8]) -> Option<image::DynamicImage> {
    if bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if count == 0 {
        return None;
    }

    let mut best_index = 0;
    let mut best_score = 0u32;
    let mut best_offset = 0usize;
    let mut best_size = 0usize;

    for i in 0..count {
        let offset = 6 + i * 16;
        if offset + 16 > bytes.len() {
            break;
        }

        let w_raw = bytes[offset] as u32;
        let h_raw = bytes[offset + 1] as u32;
        let width = if w_raw == 0 { 256 } else { w_raw };
        let height = if h_raw == 0 { 256 } else { h_raw };
        let bpp = u16::from_le_bytes([bytes[offset + 6], bytes[offset + 7]]) as u32;

        let img_size = u32::from_le_bytes([
            bytes[offset + 8],
            bytes[offset + 9],
            bytes[offset + 10],
            bytes[offset + 11],
        ]) as usize;
        let img_offset = u32::from_le_bytes([
            bytes[offset + 12],
            bytes[offset + 13],
            bytes[offset + 14],
            bytes[offset + 15],
        ]) as usize;

        let score = (width * height) + (bpp * 10);
        if score > best_score {
            best_score = score;
            best_index = i;
            best_offset = img_offset;
            best_size = img_size;
        }
    }

    if best_size > 0 && best_offset + best_size <= bytes.len() {
        let slice = &bytes[best_offset..best_offset + best_size];
        if let Ok(img) = image::load_from_memory_with_format(slice, image::ImageFormat::Png) {
            return Some(img);
        }

        let mut single_ico = Vec::with_capacity(6 + 16 + best_size);
        single_ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
        let dir_entry_offset = 6 + best_index * 16;
        if dir_entry_offset + 16 <= bytes.len() {
            let mut entry = bytes[dir_entry_offset..dir_entry_offset + 16].to_vec();
            let new_offset = 22u32.to_le_bytes();
            entry[12] = new_offset[0];
            entry[13] = new_offset[1];
            entry[14] = new_offset[2];
            entry[15] = new_offset[3];
            single_ico.extend_from_slice(&entry);
            single_ico.extend_from_slice(slice);

            if let Ok(img) = image::load_from_memory_with_format(&single_ico, image::ImageFormat::Ico) {
                return Some(img);
            }
        }
    }

    None
}

fn load_image_to_color_image(path: &Path) -> Option<ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let is_ico = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case("ico"));

    let dynamic_img = if is_ico {
        load_ico_best_frame(&bytes).or_else(|| image::load_from_memory(&bytes).ok())?
    } else {
        image::load_from_memory(&bytes).ok()?
    };

    let rgba = dynamic_img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    Some(ColorImage::from_rgba_unmultiplied(size, &pixels))
}

fn generate_synthetic_banner_image(icon_path: &Path) -> Option<ColorImage> {
    let bytes = std::fs::read(icon_path).ok()?;
    let is_ico = icon_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or(false, |ext| ext.eq_ignore_ascii_case("ico"));

    let dynamic_img = if is_ico {
        load_ico_best_frame(&bytes).or_else(|| image::load_from_memory(&bytes).ok())?
    } else {
        image::load_from_memory(&bytes).ok()?
    };

    let img = dynamic_img.to_rgba8();
    if img.width() == 0 || img.height() == 0 {
        return None;
    }

    let bg_resized = image::imageops::resize(&img, 460, 215, image::imageops::FilterType::Triangle);
    let mut bg = image::imageops::blur(&bg_resized, 14.0);

    for pixel in bg.pixels_mut() {
        pixel[0] = ((pixel[0] as f32) * 0.55) as u8;
        pixel[1] = ((pixel[1] as f32) * 0.55) as u8;
        pixel[2] = ((pixel[2] as f32) * 0.55) as u8;
        pixel[3] = 255;
    }

    let mut sharp = image::imageops::resize(&img, 116, 116, image::imageops::FilterType::Lanczos3);
    let r = 14.0_f32;
    for y in 0..116 {
        for x in 0..116 {
            let px = x as f32;
            let py = y as f32;
            let cx = if px < r { r } else if px >= 116.0 - r { 116.0 - r } else { px };
            let cy = if py < r { r } else if py >= 116.0 - r { 116.0 - r } else { py };
            let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            if dist > r {
                sharp.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            } else if dist >= r - 1.2 || x == 0 || y == 0 || x == 115 || y == 115 {
                sharp.put_pixel(x, y, image::Rgba([0x66, 0xC0, 0xF4, 0xDD]));
            }
        }
    }

    image::imageops::overlay(&mut bg, &sharp, 172, 49);
    let raw = bg.into_raw();
    Some(ColorImage::from_rgba_unmultiplied([460, 215], &raw))
}

fn load_embedded_app_icon(ctx: &egui::Context) -> Option<TextureHandle> {
    let icon_bytes = include_bytes!("../assets/icon_256.png");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let pixels = rgba.into_raw();
        let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);
        Some(ctx.load_texture("app_icon", color_image, TextureOptions::LINEAR))
    } else {
        None
    }
}

fn render_favorite_star_button(ui: &mut Ui, is_favorite: bool, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let is_hovered = response.hovered();

    let bg_color = if is_hovered {
        Color32::from_rgb(0x28, 0x3D, 0x55)
    } else {
        Color32::from_rgb(0x1E, 0x2D, 0x3D)
    };
    let stroke_color = if is_hovered {
        Color32::from_rgb(0x66, 0xC0, 0xF4)
    } else {
        Color32::from_rgb(0x32, 0x52, 0x70)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(6), bg_color);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, stroke_color),
        StrokeKind::Inside,
    );

    let scale = size / 32.0_f32;
    let center_x = rect.center().x;
    let center_y = rect.center().y + 0.5_f32 * scale;
    let center = pos2(center_x, center_y);
    let outer_radius = 8.0_f32 * scale;
    let inner_radius = outer_radius * 0.382_f32;
    let num_points = 10;
    let mut points = Vec::with_capacity(num_points);

    for i in 0..num_points {
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * (std::f32::consts::PI / 5.0_f32);
        let r = if i % 2 == 0 { outer_radius } else { inner_radius };
        points.push(pos2(center_x + angle.cos() * r, center_y + angle.sin() * r));
    }

    let star_color = if is_favorite {
        if is_hovered {
            Color32::from_rgb(0xFC, 0xD3, 0x4D)
        } else {
            Color32::from_rgb(0xFB, 0xBF, 0x24)
        }
    } else {
        if is_hovered {
            Color32::from_rgb(0x9C, 0xA3, 0xAF)
        } else {
            Color32::from_rgb(0x4B, 0x55, 0x63)
        }
    };

    let mut mesh = egui::epaint::Mesh::default();
    mesh.reserve_vertices(11);
    mesh.reserve_triangles(10);
    mesh.colored_vertex(center, star_color);

    for i in 0..num_points {
        mesh.colored_vertex(points[i], star_color);
    }

    for i in 0..10_u32 {
        let next = (i + 1) % 10;
        mesh.add_triangle(0, i + 1, next + 1);
    }

    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().add(egui::Shape::closed_line(
        points,
        Stroke::new(1.1_f32, star_color),
    ));

    response.on_hover_text(if is_favorite {
        "Remove from Favorites"
    } else {
        "Add to Favorites"
    })
}
