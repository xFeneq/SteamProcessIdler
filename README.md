<div align="center">

<img src="assets/logo.png" width="96" height="96" alt="SteamProcessIdler Logo" />

# SteamProcessIdler

A lightweight, native Windows desktop application to simulate running Steam games in the background without downloading game files.

[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011%20(x64)-0078D6?style=flat)](#requirements)
[![Language](https://img.shields.io/badge/Language-Rust%202024-DEA584?style=flat)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/Version-1.3.0-66C0F4?style=flat)](https://github.com/xFeneq/SteamProcessIdler/releases)
[![License](https://img.shields.io/badge/License-MIT-0284C7?style=flat)](LICENSE)

</div>

---

### How It Works

SteamProcessIdler spawns a lightweight local worker matching the target game executable, creates an off-screen Win32 window, and initializes the Steamworks API (`steam_api64.dll`) inside an isolated session sandbox. 

Because the process runs locally with an authentic window handle and valid Steam client integration, both **Steam** and **Discord** recognize the title as actively running without ever downloading full game packages.

---

### Key Features

#### 1. Core Simulation & Library Scanning
- **Fast binary scanner:** Parses `appinfo.vdf`, `libraryfolders.vdf`, and `localconfig.vdf` directly from your local Steam installation with seamless support for **multi-account setups** and **Family Sharing**.
- **Discord Quests & Activity compatibility:** Automatically detects and tags games matching the Discord detectable catalog (`DC` badge) for effortless quest completion.
- **Executable path overrides:** Custom executable paths can be edited per game for titles with external launchers, with one-click **Reset to Default** to restore original Steam metadata.
- **Custom game entries:** Easily register and idle non-library games or custom AppIDs via `+ Add Custom`.

#### 2. VAC & Anti-Cheat Safety Shield
- **Background system scanner:** Lightweight, periodic Win32 process monitor checks active Windows processes with near-zero CPU overhead.
- **Anti-Cheat safety shield:** Automatically identifies **VAC-protected games** and third-party anti-cheat services (Easy Anti-Cheat, BattlEye, Vanguard, Ricochet, GameGuard, and more).
- **Proactive session protection:** Instantly locks protected games and automatically halts running sessions when online anti-cheat protected titles are active.
- **Duplicate process protection:** Prevents starting an idler session if the real game executable is already running on your PC.

#### 3. Session Queue, Auto-Stop & History
- **Automated session queue:** Queue multiple games for sequential execution with fixed or randomized session intervals and cooldowns.
- **Session auto-stop:** Automatically terminates running sessions after a configured duration, perfect for timed Discord quests.
- **Favorites system:** Mark favorite titles with a crisp golden star; filter by `Favorites` or sort library using `Favorites First`.
- **Persistent lifetime statistics:** Optional local tracking (`history.json`) recording total idled time, session counts, and timestamp logs with a dedicated `History` modal.

#### 4. Lightweight Native Rust Design
- **Single portable executable:** Zero external dependencies (no .NET runtime or Visual C++ redistributable required), statically linked x64 binary under ~6 MB.
- **Minimal resource usage:** Background simulation stub processes consume only ~2 to 4 MB of RAM each.
- **Dual view modes:** Toggle between high-density **List View** and an artwork-rich **Grid View** with local banner caching.
- **In-app updater:** Checks for new releases directly from GitHub and provides seamless self-updating.

---

### Quick Start

1. **Download:** Grab the latest `SteamProcessIdler.exe` from [GitHub Releases](https://github.com/xFeneq/SteamProcessIdler/releases).
2. **Launch:** Run the application while the official Steam desktop client is open and logged in.
3. **Idle:** Select any game from your library, click **Launch** to simulate playtime immediately, or click **+** to add it to your automated queue.

---

### Requirements

- **Operating System:** Windows 10 or Windows 11 (64-bit).
- **Steam Client:** Official Steam desktop client installed, running, and logged into your account.

---

### Safety & Disclaimer

- **No credentials needed:** SteamProcessIdler never requests login credentials, passwords, or Steam Guard tokens. It hooks strictly into the local Steam client.
- **Clean sandbox isolation:** All temporary session shims are isolated inside `%LOCALAPPDATA%\SteamProcessIdler\Sessions\` and cleaned up automatically.
- **Use responsibly:** Avoid running simulated sessions simultaneously alongside active competitive online multiplayer games protected by anti-cheat. SteamProcessIdler is an independent open-source utility and is not affiliated with Valve Corporation or Discord Inc.
