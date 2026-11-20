<div align="center">

<img src="assets/icon_256.png" width="96" height="96" alt="SteamProcessIdler Icon" />

# SteamProcessIdler

A lightweight, native Windows desktop application to simulate running Steam games in the background without downloading game files.

[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011%20(x64)-0078D6?style=flat)](#requirements)
[![Rust](https://img.shields.io/badge/Language-Rust%202024-DEA584?style=flat)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/Version-1.3.0-66C0F4?style=flat)](https://github.com/xFeneq/SteamProcessIdler/releases)
[![License](https://img.shields.io/badge/License-MIT-0284C7?style=flat)](LICENSE)

</div>

---

### How it works

SteamProcessIdler creates a lightweight local process matching the executable name of your chosen game, spawns an off-screen Win32 window, and initializes the Steamworks SDK (`steam_api64.dll`) for that process session.

Because the process runs locally with an authentic window handle and valid Steam client integration, both Steam and Discord recognize the game as actively running, even when game files are not downloaded on your computer.

---

### Key Features in Version 1.3.0

- **Favorites System:** Mark favorite games with a crisp, vector-rendered golden star directly on cards (List and Grid view) and in Game Details. Quickly filter by `Favorites` or sort using `Favorites First`.
- **Session History and Lifetime Statistics:** Optional persistent session tracking stored in `%LOCALAPPDATA%\SteamProcessIdler\history.json`. Review total idled time, session count, and last idled dates per game in Game Details and inspect all recent activity in the dedicated `History` modal.
- **Process Guard and VAC / Anti-Cheat Safety Shield:** Real-time background scanner that detects active system processes and anti-cheat services. Prevents duplicate simulated processes when a real game is running, blocks and automatically pauses VAC/AC protected idler sessions if online anti-cheat systems are active, and includes an optional toggle to stop all idling when any real Steam game starts.
- **Dual View Modes (List and Grid):** Switch smoothly between the detailed List view and a SAM-inspired Grid view featuring Steam horizontal banners cached locally in `%LOCALAPPDATA%\SteamProcessIdler\BannerCache\`, with synthetic banner fallbacks for games without official horizontal art.
- **Session Queue and Planner:** Queue up multiple games for automated sequential idling. Features include one-click safe batch queuing for Discord-detectable games, reordering, progress tracking, configurable duration and cooldown intervals with flexible units (ms, sec, min, hour).
- **Session Auto-Stop:** Set automatic shutdown timers for running sessions, using either fixed durations or randomized intervals with a safety buffer.
- **Vector Icons and Clean Interface:** Fully vector-drawn icons for launch, stop, edit, queue, panel toggle, favorites, settings, and navigation, completely avoiding missing font glyphs or Unicode artifacts.
- **Interactive Safety and DC Badges:** Clickable `DC`, `VAC`, and `AC` badges with comprehensive explanations for Discord quests, Valve Anti-Cheat policies, and third-party anti-cheat guidelines.
- **Executable Overrides with Reset to Default:** Custom executable paths can be edited per game and restored to default Steam values with a single click.
- **Library Sorting:** Quickly sort games by Name (A to Z), Least Played, Most Played, Discord Detectable First, or Favorites First.
- **Collapsible and Resizable Sidebar:** Adjust the right sidebar width from 280 px to 520 px, collapse it directly using the compact toggle button on the panel header, or restore it via the sleek edge handle button on the right edge of the window.
- **Zero Runtime Dependencies:** Built in native Rust with static C runtime linkage. A single executable runs out of the box on clean Windows 10 and 11 installations.
- **Dual Architecture:** Operates either as an interactive desktop UI or as an ultra-lightweight background stub process consuming only about 2 to 4 MB of RAM per running session.
- **Full Library Scanner:** Fast binary parser for `appinfo.vdf`, `libraryfolders.vdf`, `localconfig.vdf`, and `sharedconfig.vdf`, supporting multi-account setups and accurate playtime statistics.
- **Session Sandbox:** All temporary simulation shims reside strictly in `%LOCALAPPDATA%\SteamProcessIdler\Sessions\`, leaving actual Steam game directories untouched.
- **Automatic Updates:** Integrated GitHub Releases check with direct in-app download and seamless hot-replacement.

---

### Requirements

- **OS:** Windows 10 or Windows 11 (64-bit).
- **Steam:** An active local Steam desktop client.

---

### How to use

1. Download `SteamProcessIdler.exe` from the [Releases](https://github.com/xFeneq/SteamProcessIdler/releases) page and run it.
2. Select any title from your game library in either List or Grid view, or add a custom game using `+ Add Custom`.
3. Click `Launch` to begin simulating playtime, or `+` to add the game to your automated queue.
4. If a title uses a custom launcher, click the `Edit` button next to the game to adjust the relative executable path, or click `Reset to Default` to restore the original Steam configuration.
5. In the right panel, monitor active sessions or manage your sequential queue in the `Queue` tab.

---

### Use Cases

- **Discord Quests and Rich Presence:** Complete Discord quests and display your gaming status without downloading huge game packages.
- **Steam Trading Cards and Playtime:** Farm trading cards and keep track of playtime across owned games and family shared titles.
- **Storage and Bandwidth Savings:** Conserve disk space and network bandwidth by avoiding massive game installations.

---

### Safety and Disclaimer

- **No account credentials required:** The application never asks for login credentials or API tokens, nor does it intercept network traffic.
- **Isolated sandbox:** All temporary files are confined to `%LOCALAPPDATA%\SteamProcessIdler\Sessions\` and cleaned up automatically when sessions finish.
- **Use responsibly:** This software is an independent open-source project and is not affiliated with, endorsed by, or connected to Valve Corporation or Discord Inc.
