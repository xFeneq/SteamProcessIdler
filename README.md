<div align="center">

<img src="SteamProcessIdler/Assets/icon_256.png" width="96" height="96" alt="SteamProcessIdler Icon" />

# SteamProcessIdler

**Play any Steam game in the background without downloading gigabytes of game files.**

Simulate official Steam game processes to complete Discord Quests, farm Steam trading cards, and track playtime with zero performance impact.

[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011-0078D6?logo=windows&logoColor=white)](https://www.microsoft.com/windows)
[![.NET 9.0](https://img.shields.io/badge/.NET-9.0%20WPF-512BD4?logo=dotnet&logoColor=white)](https://dotnet.microsoft.com/download/dotnet/9.0)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

---

## What is SteamProcessIdler?

**SteamProcessIdler** tells Steam and Discord that you are actively playing a game, **without requiring you to download or install the actual 50–100 GB game files**.

It creates a tiny, safe background simulator (stub process) that communicates with your local Steam client. To Windows, Steam, and Discord, your game looks and acts 100% real — while taking virtually **0% CPU** and less than **15 MB of RAM**.

---

## Why use it?

### 🎮 Complete Discord Quests & Activity
Many Discord Quests reward you with exclusive avatar decorations, badges, or in-game loot just for playing a game for 15 minutes. With SteamProcessIdler, you can finish these quests in minutes without wasting hours downloading huge games, and **without connecting third-party bots or sharing your Discord account token**.

### 🃏 Farm Steam Trading Cards & Hours
Effortlessly drop card sets from newly bought games, accumulate playtime, or keep active hours on your profile across games in your library (including Free-to-Play titles and shared Steam Family libraries).

### 💾 Save Disk Space & Internet Bandwidth
Never delete your favorite games just to free up temporary space for an event or promo. Keep your SSD free and avoid downloading hundreds of gigabytes of unwanted data.

---

## How to Use (Step-by-Step)

### 1. Download & Launch
1. Download the latest `SteamProcessIdler.exe` from the **[Releases](https://github.com/xFeneq/SteamProcessIdler/releases)** page.
2. Make sure your official **Steam** desktop app is open and logged in.
3. Run `SteamProcessIdler.exe`.
   *(Requires the free [.NET 9.0 Desktop Runtime](https://dotnet.microsoft.com/download/dotnet/9.0) if you don't already have it installed).*

### 2. Pick Your Game
- Scroll through your Steam library in the left column or use the instant search bar to find a game by name, AppID, or `.exe` file.
- Don't see a game you want? Click **+ Add Custom** to add any Steam game by entering its numeric AppID.

### 3. Click Launch
- Click the green **▶ Launch** button next to your game.
- The game will appear in the **Active Sessions** list on the right.
- Steam will switch your status to **In-Game**, and Discord will show the game in your **Playing** activity status.
- When you are finished, simply click **⏹ Stop** (or **Stop All Sessions**).

---

> [!TIP]
> ### Games with Launchers (e.g., Delta Force, MMOs)
> Some games on Steam launch an external launcher first (e.g. `launcher.exe`), but Discord expects the actual main client executable (such as `DeltaForceClient-Win64-Shipping.exe` or `Game\Binaries\Win64\Shipping.exe`).
> 
> If a game has multiple `.exe` options, SteamProcessIdler lets you select the target binary from a dropdown. You can also click the edit button (**✎**) next to any game to type the exact relative `.exe` path. The app remembers your custom choices permanently!

---

## Safety & Privacy

- **100% Local & Offline:** SteamProcessIdler connects only to your local Steam client. It has zero telemetry, zero background servers, and never asks for your Steam password or Discord token.
- **Never Touches Real Game Files:** All simulation files are strictly quarantined inside a temporary Windows folder (`%LOCALAPPDATA%\SteamProcessIdler\Sessions\`). Your existing Steam library files (`steamapps\common`) are never altered or written to.
- **Automatic Cleanup:** When a session is stopped or the application closes, temporary session files are automatically erased.

---

## FAQ (Frequently Asked Questions)

#### Q: Why does Steam temporarily show the game as "Installed / Ready to Play" while it's running?
**A:** When Steam detects an active game process, it keeps that status in your computer's RAM during the session so game overlays and features work properly. It **does not download any game files** to your hard drive. As soon as you stop the session or restart Steam, it returns to normal.

#### Q: Where does `steam_api64.dll` come from? Do I need to find it?
**A:** In 99% of cases, you don't need to do anything! When you start the app, SteamProcessIdler automatically finds `steam_api64.dll` in your local Steam client folder. You can see the green status dot at the top right of the app confirming it is ready.

#### Q: Can I run multiple games at the same time?
**A:** Yes! You can launch multiple games from the list. Each game runs independently in its own isolated background process.

---

<details>
<summary><b>🛠️ Building from Source (For Developers)</b></summary>

<br />

If you prefer to compile SteamProcessIdler yourself, you only need the [.NET 9.0 SDK](https://dotnet.microsoft.com/download/dotnet/9.0):

```powershell
# Clone the repository
git clone https://github.com/xFeneq/SteamProcessIdler.git
cd SteamProcessIdler

# Build in Debug mode
dotnet build SteamProcessIdler/SteamProcessIdler.csproj

# Publish an optimized single-file Release executable
dotnet publish SteamProcessIdler/SteamProcessIdler.csproj -c Release -r win-x64 --self-contained false -p:PublishSingleFile=true
```

The resulting standalone `.exe` will be located in:
`SteamProcessIdler/bin/Release/net9.0-windows/win-x64/publish/SteamProcessIdler.exe`

</details>

---

## Disclaimer

**SteamProcessIdler** is an independent, community-driven open-source project and is **not affiliated, associated, authorized, endorsed by, or in any way officially connected** with Valve Corporation, Steam, Discord Inc., or any of their subsidiaries or affiliates.

All game titles, trademarks, and registered marks belong to their respective owners.

---

## License

This project is licensed under the [MIT License](LICENSE).
