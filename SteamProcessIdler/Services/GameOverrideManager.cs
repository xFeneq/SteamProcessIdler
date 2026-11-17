using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using SteamProcessIdler.Models;

namespace SteamProcessIdler.Services;

public static class GameOverrideManager
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNameCaseInsensitive = true
    };

    private static string GetStorageFilePath()
    {
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var dir = Path.Combine(localAppData, "SteamProcessIdler");
        Directory.CreateDirectory(dir);
        return Path.Combine(dir, "custom_games.json");
    }

    public static List<GameOverrideEntry> LoadOverrides()
    {
        var filePath = GetStorageFilePath();
        if (!File.Exists(filePath))
        {
            return new List<GameOverrideEntry>();
        }

        try
        {
            var json = File.ReadAllText(filePath);
            var entries = JsonSerializer.Deserialize<List<GameOverrideEntry>>(json, JsonOptions);
            return entries ?? new List<GameOverrideEntry>();
        }
        catch
        {
            return new List<GameOverrideEntry>();
        }
    }

    public static void SaveOverrides(List<GameOverrideEntry> entries)
    {
        var filePath = GetStorageFilePath();
        try
        {
            var json = JsonSerializer.Serialize(entries, JsonOptions);
            File.WriteAllText(filePath, json);
        }
        catch
        {
        }
    }

    public static void SaveOrUpdateOverride(SteamGame game)
    {
        var entries = LoadOverrides();
        entries.RemoveAll(e => e.AppId == game.AppId);

        entries.Add(new GameOverrideEntry
        {
            AppId = game.AppId,
            Name = game.Name,
            InstallDir = game.InstallDir,
            ExecutableRelativePath = game.ExecutableRelativePath,
            IsCustom = game.IsCustom
        });

        SaveOverrides(entries);
    }

    public static void RemoveOverride(uint appId)
    {
        var entries = LoadOverrides();
        int removed = entries.RemoveAll(e => e.AppId == appId);
        if (removed > 0)
        {
            SaveOverrides(entries);
        }
    }

    public static void ApplyOverrides(IList<SteamGame> games, IList<SteamGame> customGames)
    {
        var overrides = LoadOverrides();
        if (overrides.Count == 0)
        {
            return;
        }

        foreach (var entry in overrides)
        {
            if (entry.IsCustom)
            {
                var existingCustom = customGames.FirstOrDefault(g => g.AppId == entry.AppId);
                if (existingCustom == null)
                {
                    customGames.Add(new SteamGame
                    {
                        AppId = entry.AppId,
                        Name = entry.Name,
                        InstallDir = entry.InstallDir,
                        ExecutableRelativePath = entry.ExecutableRelativePath,
                        AvailableExecutables = new List<string> { entry.ExecutableRelativePath },
                        IsCustom = true
                    });
                }
                else
                {
                    existingCustom.Name = entry.Name;
                    existingCustom.InstallDir = entry.InstallDir;
                    existingCustom.ExecutableRelativePath = entry.ExecutableRelativePath;
                    if (!existingCustom.AvailableExecutables.Contains(entry.ExecutableRelativePath, StringComparer.OrdinalIgnoreCase))
                    {
                        existingCustom.AvailableExecutables.Insert(0, entry.ExecutableRelativePath);
                    }
                }
            }
            else
            {
                var existingGame = games.FirstOrDefault(g => g.AppId == entry.AppId);
                if (existingGame != null)
                {
                    if (!string.IsNullOrWhiteSpace(entry.InstallDir))
                    {
                        existingGame.InstallDir = entry.InstallDir;
                    }

                    if (!string.IsNullOrWhiteSpace(entry.ExecutableRelativePath))
                    {
                        existingGame.ExecutableRelativePath = entry.ExecutableRelativePath;
                        if (!existingGame.AvailableExecutables.Contains(entry.ExecutableRelativePath, StringComparer.OrdinalIgnoreCase))
                        {
                            existingGame.AvailableExecutables.Insert(0, entry.ExecutableRelativePath);
                        }
                    }
                }
            }
        }
    }
}
