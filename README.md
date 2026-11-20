<div align="center">

<img src="assets/logo.png" width="96" height="96" alt="SteamProcessIdler Logo" />

# SteamProcessIdler

A lightweight, native Windows desktop application to simulate running Steam games in the background without downloading game files.

[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011%20(x64)-0078D6?style=flat)](#requirements)
[![Language](https://img.shields.io/badge/Language-Rust%202024-DEA584?style=flat)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/Version-1.3.0-66C0F4?style=flat)](https://github.com/xFeneq/SteamProcessIdler/releases)
[![License](https://img.shields.io/badge/License-GPL--3.0-0284C7?style=flat)](LICENSE)

</div>

---

### What is SteamProcessIdler?

**SteamProcessIdler** allows you to accumulate Steam playtime hours, farm Steam Trading Cards, and complete **Discord Quests** (with active *Rich Presence* status) **without downloading or installing dozens of gigabytes of game files** on your drive.

> **Important:** SteamProcessIdler **requires you to own the game** on your Steam account (or have access to it via Steam Family Sharing or Free-to-Play titles). It does not unlock, pirate, or bypass licenses for games you do not own.

---

### How It Works

SteamProcessIdler operates entirely on your local machine using authentic Steamworks client hooks and lightweight background workers:

1. **Scans your Steam library locally:** Automatically scans your local Steam files (`appinfo.vdf` and `localconfig.vdf`) to discover all games assigned to your account — including uninstalled titles and family shared libraries.
2. **Spawns an empty background process:** When you click **Launch**, it creates a lightweight, empty process (~2–4 MB RAM) matching the exact `.exe` file name of the chosen game and opens an invisible, off-screen Win32 window (which allows **Discord Activity** to detect the game).
3. **Connects to your active Steam client:** In an isolated temporary session sandbox, it hooks the local `steam_api64.dll` from your active Steam client. Steam instantly registers you as "In-Game" and accrues playtime and trading card drops normally.
4. **Guards your sessions automatically:** An integrated background monitor constantly tracks active system processes. If you launch a real game protected by VAC or third-party anti-cheat, the shield immediately pauses and locks protected idling sessions.

---

### Common Use Cases

- **Complete Discord Quests:** Fulfill playtime requirements for Discord Quests without spending hours downloading 50+ GB game packages.
- **Farm Steam Trading Cards:** Idle eligible titles in the background to earn trading card drops effortlessly.
- **Track & Accumulate Playtime:** Accrue playtime hours across owned games and family shared titles.
- **Save Storage & Bandwidth:** Conserve hundreds of gigabytes of SSD space, avoid unnecessary drive write wear, and eliminate huge download quotas.

---

### Key Features

#### 1. Smart Library & Discord Detection
- **Fast binary library scanning:** Parses `appinfo.vdf`, `libraryfolders.vdf`, and `localconfig.vdf` directly from your local Steam directory with multi-account support.
- **Discord Quests & Activity compatibility:** Automatically tags games matching Discord's detectable application list (`DC` badge) for guaranteed quest completion.
- **Executable path overrides:** Customize relative executable paths per title for games that use custom launchers, with one-click **Reset to Default**.
- **Custom game registration:** Easily register and idle non-library games or custom AppIDs using `+ Add Custom`.

#### 2. VAC & Anti-Cheat Safety Shield
- **Continuous background monitor:** Periodic Win32 process snapshot checks active system processes with near-zero CPU usage.
- **Heuristic anti-cheat detection:** Automatically recognizes **VAC-protected titles** and third-party anti-cheat services (Easy Anti-Cheat, BattlEye, Vanguard, Ricochet, GameGuard, Xigncode, and more).
- **Proactive session locking:** Instantly locks protected games and automatically halts running sessions when online anti-cheat protected titles are active.
- **Duplicate process protection:** Prevents starting an idler session if the real game executable is already running on your PC.

#### 3. Session Queue, Auto-Stop & History
- **Automated session queue:** Queue multiple games for sequential idling with configurable fixed or randomized durations and cooldowns.
- **Session auto-stop:** Automatically stops manually launched sessions after a configured duration (ideal for timed Discord quests).
- **Favorites system:** Mark favorite titles with a crisp golden star; filter by `Favorites` or sort your library using `Favorites First`.
- **Persistent lifetime statistics:** Optional local tracking (`history.json`) recording total idled time, session count, and timestamps with a dedicated `History` modal.

#### 4. Native Rust Architecture
- **Single standalone executable:** Zero external runtimes (no .NET runtime, Python, or Visual C++ redistributable required), statically linked x64 binary under ~6 MB.
- **Ultra-low resource usage:** Background simulation stub processes consume only about 2 to 4 MB of RAM each.
- **Dual view modes:** Toggle between high-density **List View** and visual **Grid View** with locally cached Steam banners.
- **Built-in updater:** Automatically checks GitHub Releases for new updates with in-app download and hot-replacement.

---

### Quick Start

1. **Download:** Get the latest `SteamProcessIdler.exe` from [GitHub Releases](https://github.com/xFeneq/SteamProcessIdler/releases).
2. **Launch:** Run the application while your official Steam desktop client is open and logged in.
3. **Idle:** Select any game from your library, click **Launch** to simulate playtime immediately, or click **+** to add it to your automated queue.

---

### Frequently Asked Questions (FAQ)

#### Do I need to own the game on Steam?
**Yes.** You must own the game on your Steam account, have access to it via Steam Family Sharing, or have it added as a Free-to-Play game. SteamProcessIdler communicates with your genuine Steam client and does not bypass Steam licensing or pirate games.

#### Do I need to install the game first?
**No.** SteamProcessIdler creates lightweight dummy worker processes with valid Win32 window handles and Steamworks API hooks. You do not need to download or install any game files on your drive.

#### Does it ask for my Steam login or password?
**Never.** The application operates 100% locally on your computer and attaches directly to your running Steam desktop client through the standard Steamworks DLL. It never asks for, handles, or transmits account credentials, passwords, or Steam Guard codes.

#### Is it safe to use with VAC / Anti-Cheat games?
SteamProcessIdler never modifies, patches, or injects into game memory or system files. It runs a standalone stub that calls standard Steamworks API functions (`SteamAPI_Init` / `SteamAPI_Shutdown`). Furthermore, the built-in **VAC & Anti-Cheat Safety Shield** proactively locks protected titles if active anti-cheat processes (such as EAC, BattlEye, or Vanguard) are detected in the system. As a general rule, you should avoid running simulated sessions alongside active competitive multiplayer games.

---

### Requirements

- **Operating System:** Windows 10 or Windows 11 (64-bit).
- **Steam Client:** Official Steam desktop client installed, running, and logged into your account.

---

### Safety & Disclaimer

- **Isolated sandbox:** All temporary session files reside strictly in `%LOCALAPPDATA%\SteamProcessIdler\Sessions\` and are cleaned up automatically.
- **Independent project:** SteamProcessIdler is an independent open-source utility licensed under the GNU General Public License v3.0 (GPL-3.0) and is not affiliated with, endorsed by, or connected to Valve Corporation or Discord Inc.
