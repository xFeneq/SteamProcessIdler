using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using Microsoft.Win32;
using SteamProcessIdler.Models;
using ValveKeyValue;

namespace SteamProcessIdler.Services;

public class SteamLibraryScanner
{
    private const uint MagicAppInfo26 = 0x07564426;
    private const uint MagicAppInfo27 = 0x07564427;
    private const uint MagicAppInfo28 = 0x07564428;
    private const uint MagicAppInfo29 = 0x07564429;

    private static readonly HttpClient HttpClientInstance = new()
    {
        Timeout = TimeSpan.FromSeconds(15)
    };

    public List<SteamGame> ScanGames(string? customSteamPath = null)
    {
        var steamPath = customSteamPath ?? GetSteamInstallationPath();
        if (string.IsNullOrWhiteSpace(steamPath) || !Directory.Exists(steamPath))
        {
            return new List<SteamGame>();
        }

        var ownedAppIds = GetOwnedAppIds(steamPath);

        var appInfoPath = Path.Combine(steamPath, "appcache", "appinfo.vdf");
        if (!File.Exists(appInfoPath))
        {
            return new List<SteamGame>();
        }

        var games = ParseAppInfoVdf(appInfoPath, steamPath, ownedAppIds);

        return games
            .GroupBy(g => g.AppId)
            .Select(g => g.First())
            .OrderBy(g => g.Name, StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    public static List<SteamGame> Scan(string? customSteamPath = null)
    {
        return new SteamLibraryScanner().ScanGames(customSteamPath);
    }

    public static string? GetSteamInstallationPath()
    {
        try
        {
            if (OperatingSystem.IsWindows())
            {
                using var hkcuKey = Registry.CurrentUser.OpenSubKey(@"Software\Valve\Steam");
                if (hkcuKey?.GetValue("SteamPath") is string steamPath && !string.IsNullOrWhiteSpace(steamPath))
                {
                    var normalized = steamPath.Replace('/', Path.DirectorySeparatorChar);
                    if (Directory.Exists(normalized))
                    {
                        return normalized;
                    }
                }

                using var hklmKey = Registry.LocalMachine.OpenSubKey(@"SOFTWARE\WOW6432Node\Valve\Steam")
                                 ?? Registry.LocalMachine.OpenSubKey(@"SOFTWARE\Valve\Steam");
                if (hklmKey?.GetValue("InstallPath") is string installPath && !string.IsNullOrWhiteSpace(installPath))
                {
                    var normalized = installPath.Replace('/', Path.DirectorySeparatorChar);
                    if (Directory.Exists(normalized))
                    {
                        return normalized;
                    }
                }
            }
        }
        catch
        {
        }

        var defaultPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86), "Steam");
        return Directory.Exists(defaultPath) ? defaultPath : null;
    }

    public static HashSet<uint> GetOwnedAppIds(string steamPath)
    {
        var ownedAppIds = new HashSet<uint>();

        var userDataDir = Path.Combine(steamPath, "userdata");
        if (Directory.Exists(userDataDir))
        {
            try
            {
                foreach (var userDir in Directory.EnumerateDirectories(userDataDir))
                {
                    var localConfigPath = Path.Combine(userDir, "config", "localconfig.vdf");
                    if (File.Exists(localConfigPath))
                    {
                        ExtractAppIdsFromConfig(localConfigPath, ownedAppIds);
                    }

                    try
                    {
                        foreach (var sharedConfigPath in Directory.EnumerateFiles(userDir, "*sharedconfig.vdf", SearchOption.AllDirectories))
                        {
                            ExtractAppIdsFromConfig(sharedConfigPath, ownedAppIds);
                        }
                    }
                    catch
                    {
                    }
                }
            }
            catch
            {
            }
        }

        var libraryFoldersPath = Path.Combine(steamPath, "steamapps", "libraryfolders.vdf");
        if (!File.Exists(libraryFoldersPath))
        {
            libraryFoldersPath = Path.Combine(steamPath, "config", "libraryfolders.vdf");
        }

        if (File.Exists(libraryFoldersPath))
        {
            ExtractAppIdsFromLibraryFolders(libraryFoldersPath, ownedAppIds);
        }

        return ownedAppIds;
    }

    private static void ExtractAppIdsFromConfig(string configPath, HashSet<uint> appIds)
    {
        bool parsedSuccessfully = false;
        try
        {
            var textSerializer = KVSerializer.Create(KVSerializationFormat.KeyValues1Text);
            using var stream = File.OpenRead(configPath);
            var kv = textSerializer.Deserialize(stream);
            if (kv != null)
            {
                var appsNode = kv["Software"]?["Valve"]?["Steam"]?["apps"]
                            ?? kv["Software"]?["Valve"]?["Steam"]?["Apps"]
                            ?? kv["UserRoamingConfigStore"]?["Software"]?["Valve"]?["Steam"]?["apps"];

                if (appsNode is IEnumerable<KVObject> appsList)
                {
                    foreach (var app in appsList)
                    {
                        if (uint.TryParse(app.Name, out var id) && id > 0)
                        {
                            appIds.Add(id);
                        }
                    }
                }

                var userAppConfigNode = kv["UserAppConfig"];
                if (userAppConfigNode is IEnumerable<KVObject> configList)
                {
                    foreach (var app in configList)
                    {
                        if (uint.TryParse(app.Name, out var id) && id > 0)
                        {
                            appIds.Add(id);
                        }
                    }
                }

                parsedSuccessfully = true;
            }
        }
        catch
        {
            parsedSuccessfully = false;
        }

        if (!parsedSuccessfully)
        {
            try
            {
                var content = File.ReadAllText(configPath);
                var blockRegex = new Regex(@"""(?:apps|UserAppConfig)""\s*\{([\s\S]*?)\n\t{1,4}\}", RegexOptions.Multiline);
                var idRegex = new Regex(@"^\s*""(\d+)""", RegexOptions.Multiline);

                foreach (Match blockMatch in blockRegex.Matches(content))
                {
                    if (blockMatch.Success)
                    {
                        var blockContent = blockMatch.Groups[1].Value;
                        foreach (Match idMatch in idRegex.Matches(blockContent))
                        {
                            if (uint.TryParse(idMatch.Groups[1].Value, out var id) && id > 0)
                            {
                                appIds.Add(id);
                            }
                        }
                    }
                }
            }
            catch
            {
            }
        }
    }

    private static void ExtractAppIdsFromLibraryFolders(string libraryFoldersPath, HashSet<uint> appIds)
    {
        bool parsedSuccessfully = false;
        try
        {
            var textSerializer = KVSerializer.Create(KVSerializationFormat.KeyValues1Text);
            using var stream = File.OpenRead(libraryFoldersPath);
            var kv = textSerializer.Deserialize(stream);
            if (kv != null && kv.Children != null)
            {
                foreach (var folder in kv.Children)
                {
                    var apps = folder["apps"];
                    if (apps is IEnumerable<KVObject> appsList)
                    {
                        foreach (var app in appsList)
                        {
                            if (uint.TryParse(app.Name, out var id) && id > 0)
                            {
                                appIds.Add(id);
                            }
                        }
                    }

                    var folderPath = folder["path"]?.ToString(null);
                    if (!string.IsNullOrWhiteSpace(folderPath) && Directory.Exists(folderPath))
                    {
                        ScanAppManifests(folderPath, appIds);
                    }
                }
                parsedSuccessfully = true;
            }
        }
        catch
        {
            parsedSuccessfully = false;
        }

        if (!parsedSuccessfully)
        {
            try
            {
                var content = File.ReadAllText(libraryFoldersPath);
                var blockRegex = new Regex(@"""apps""\s*\{([^}]*)\}", RegexOptions.Multiline);
                var idRegex = new Regex(@"^\s*""(\d+)""", RegexOptions.Multiline);

                foreach (Match blockMatch in blockRegex.Matches(content))
                {
                    var blockContent = blockMatch.Groups[1].Value;
                    foreach (Match idMatch in idRegex.Matches(blockContent))
                    {
                        if (uint.TryParse(idMatch.Groups[1].Value, out var id) && id > 0)
                        {
                            appIds.Add(id);
                        }
                    }
                }
            }
            catch
            {
            }
        }
    }

    private static void ScanAppManifests(string libraryFolder, HashSet<uint> appIds)
    {
        try
        {
            var steamappsDir = Path.Combine(libraryFolder, "steamapps");
            if (Directory.Exists(steamappsDir))
            {
                foreach (var acfFile in Directory.EnumerateFiles(steamappsDir, "appmanifest_*.acf"))
                {
                    var fileName = Path.GetFileName(acfFile);
                    var match = Regex.Match(fileName, @"appmanifest_(\d+)\.acf", RegexOptions.IgnoreCase);
                    if (match.Success && uint.TryParse(match.Groups[1].Value, out var id) && id > 0)
                    {
                        appIds.Add(id);
                    }
                }
            }
        }
        catch
        {
        }
    }

    private static List<SteamGame> ParseAppInfoVdf(string appInfoPath, string steamPath, HashSet<uint> ownedAppIds)
    {
        var games = new List<SteamGame>();

        using var fs = File.Open(appInfoPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        using var reader = new BinaryReader(fs);

        if (fs.Length < 16)
        {
            return games;
        }

        uint magic = reader.ReadUInt32();
        if (magic != MagicAppInfo26 && magic != MagicAppInfo27 && magic != MagicAppInfo28 && magic != MagicAppInfo29)
        {
            return games;
        }

        uint universe = reader.ReadUInt32();

        long stringTableOffset = 0;
        StringTable? stringTable = null;

        if (magic >= MagicAppInfo28)
        {
            stringTableOffset = reader.ReadInt64();
            if (stringTableOffset > 0 && stringTableOffset < fs.Length)
            {
                long savedPos = fs.Position;
                try
                {
                    fs.Position = stringTableOffset;
                    int stringCount = reader.ReadInt32();
                    var strings = new List<string>(stringCount);
                    var byteBuffer = new List<byte>(64);

                    for (int i = 0; i < stringCount; i++)
                    {
                        byteBuffer.Clear();
                        byte b;
                        while ((b = reader.ReadByte()) != 0)
                        {
                            byteBuffer.Add(b);
                        }
                        strings.Add(Encoding.UTF8.GetString(byteBuffer.ToArray()));
                    }

                    stringTable = new StringTable(strings);
                }
                finally
                {
                    fs.Position = savedPos;
                }
            }
        }

        var serializerOptions = new KVSerializerOptions();
        if (stringTable != null)
        {
            serializerOptions.StringTable = stringTable;
        }

        var binarySerializer = KVSerializer.Create(KVSerializationFormat.KeyValues1Binary);
        long appRecordsEnd = (magic >= MagicAppInfo28 && stringTableOffset > 0) ? stringTableOffset : fs.Length;

        while (fs.Position < appRecordsEnd)
        {
            if (fs.Position + 8 > appRecordsEnd)
            {
                break;
            }

            uint appId = reader.ReadUInt32();
            if (appId == 0)
            {
                break;
            }

            uint entrySize = reader.ReadUInt32();
            long nextEntryPos = fs.Position + entrySize;
            if (nextEntryPos > fs.Length || nextEntryPos <= fs.Position)
            {
                break;
            }

            int entryHeaderBytes = 4 + 4 + 8 + 20 + 4 + (magic >= MagicAppInfo28 ? 20 : 0);
            if (fs.Position + entryHeaderBytes >= nextEntryPos)
            {
                fs.Position = nextEntryPos;
                continue;
            }

            fs.Position += entryHeaderBytes;

            try
            {
                var kv = binarySerializer.Deserialize(fs, serializerOptions);
                if (kv != null)
                {
                    var game = ExtractSteamGame(appId, kv, steamPath);
                    if (game != null)
                    {
                        games.Add(game);
                    }
                }
            }
            catch
            {
            }
            finally
            {
                fs.Position = nextEntryPos;
            }
        }

        return games;
    }

    private static SteamGame? ExtractSteamGame(uint appId, KVObject kv, string steamPath)
    {
        var common = kv["common"];
        if (common == null)
        {
            return null;
        }

        var name = common["name"]?.ToString(null);
        if (string.IsNullOrWhiteSpace(name))
        {
            return null;
        }

        var appType = common["type"]?.ToString(null);
        if (!string.Equals(appType, "game", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        var config = kv["config"];
        string? installDir = config?["installdir"]?.ToString(null);
        if (string.IsNullOrWhiteSpace(installDir))
        {
            installDir = common["installdir"]?.ToString(null);
        }
        if (string.IsNullOrWhiteSpace(installDir))
        {
            installDir = kv["installdir"]?.ToString(null);
        }
        installDir ??= string.Empty;

        var candidateExes = new List<string>();

        if (config != null && config["launch"] != null)
        {
            var launch = config["launch"];

            if (launch is IEnumerable<KVObject> launchItems)
            {
                foreach (var item in launchItems)
                {
                    var candidate = item["executable"]?.ToString(null);
                    if (IsValidExe(candidate))
                    {
                        var osList = item["config"]?["oslist"]?.ToString(null);
                        if (string.IsNullOrEmpty(osList) || osList.Contains("windows", StringComparison.OrdinalIgnoreCase))
                        {
                            var clean = CleanExecutablePath(candidate!);
                            if (!candidateExes.Any(e => string.Equals(e, clean, StringComparison.OrdinalIgnoreCase)))
                            {
                                candidateExes.Add(clean);
                            }
                        }
                    }
                }
            }

            if (candidateExes.Count == 0)
            {
                var launch0 = launch["0"];
                if (launch0 != null && launch0["executable"] != null)
                {
                    var candidate0 = launch0["executable"].ToString(null);
                    if (IsValidExe(candidate0))
                    {
                        candidateExes.Add(CleanExecutablePath(candidate0!));
                    }
                }
            }
        }

        if (candidateExes.Count == 0)
        {
            return null;
        }

        var scoredExes = candidateExes
            .Select(exe => new { Exe = exe, Score = ScoreExecutable(exe, name, installDir) })
            .OrderByDescending(x => x.Score)
            .Select(x => x.Exe)
            .ToList();

        var primaryExe = scoredExes[0];

        var clientIcon = common["clienticon"]?.ToString(null);
        var iconPath = ResolveGameIcon(appId, clientIcon, steamPath);

        return new SteamGame
        {
            AppId = appId,
            Name = name,
            InstallDir = installDir,
            ExecutableRelativePath = primaryExe,
            AvailableExecutables = scoredExes,
            IconPath = iconPath,
            ClientIconHash = clientIcon,
            IsCustom = false
        };
    }

    private static int ScoreExecutable(string exePath, string gameName, string installDir)
    {
        int score = 100;
        var fileName = Path.GetFileName(exePath);
        var fileNameWithoutExt = Path.GetFileNameWithoutExtension(fileName);
        var lowerFile = fileName.ToLowerInvariant();

        if (lowerFile.Contains("launcher")) score -= 50;
        if (lowerFile.Contains("updater")) score -= 50;
        if (lowerFile.Contains("crash")) score -= 60;
        if (lowerFile.Contains("report")) score -= 60;
        if (lowerFile.Contains("setup")) score -= 50;
        if (lowerFile.Contains("install")) score -= 50;
        if (lowerFile.Contains("config") || lowerFile.Contains("cfg") || lowerFile.Contains("settings")) score -= 40;
        if (lowerFile.Contains("redist")) score -= 50;
        if (lowerFile.Contains("benchmark")) score -= 40;
        if (lowerFile.Contains("server")) score -= 40;
        if (lowerFile.Contains("editor") || lowerFile.Contains("tool")) score -= 40;
        if (lowerFile.Contains("unins")) score -= 60;
        if (lowerFile.Contains("eac") || lowerFile.Contains("anticheat") || lowerFile.Contains("battleye")) score -= 30;

        if (lowerFile.Contains("shipping")) score += 60;
        if (lowerFile.Contains("win64") || lowerFile.Contains("x64")) score += 20;

        if (!string.IsNullOrWhiteSpace(installDir))
        {
            var cleanDir = installDir.ToLowerInvariant().Replace(" ", "").Replace("-", "").Replace("_", "");
            var cleanName = fileNameWithoutExt.ToLowerInvariant().Replace(" ", "").Replace("-", "").Replace("_", "");
            if (cleanDir.Length >= 3 && cleanName.Contains(cleanDir)) score += 40;
        }

        if (!string.IsNullOrWhiteSpace(gameName))
        {
            var cleanTitle = gameName.ToLowerInvariant().Replace(" ", "").Replace("-", "").Replace("_", "").Replace(":", "");
            var cleanName = fileNameWithoutExt.ToLowerInvariant().Replace(" ", "").Replace("-", "").Replace("_", "");
            if (cleanTitle.Length >= 3 && (cleanName.Contains(cleanTitle) || cleanTitle.Contains(cleanName))) score += 30;
        }

        return score;
    }

    private static string? ResolveGameIcon(uint appId, string? clientIcon, string steamPath)
    {
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var iconCacheDir = Path.Combine(localAppData, "SteamProcessIdler", "IconCache");

        var cachedAppIco = Path.Combine(iconCacheDir, $"{appId}.ico");
        if (File.Exists(cachedAppIco))
        {
            return cachedAppIco;
        }

        var cachedAppJpg = Path.Combine(iconCacheDir, $"{appId}.jpg");
        if (File.Exists(cachedAppJpg))
        {
            return cachedAppJpg;
        }

        var cachedAppPng = Path.Combine(iconCacheDir, $"{appId}.png");
        if (File.Exists(cachedAppPng))
        {
            return cachedAppPng;
        }

        var appCacheDir = Path.Combine(steamPath, "appcache", "librarycache", appId.ToString());

        if (!string.IsNullOrWhiteSpace(clientIcon))
        {
            var primaryIco = Path.Combine(steamPath, "steam", "games", $"{clientIcon}.ico");
            if (File.Exists(primaryIco))
            {
                return primaryIco;
            }

            var appCacheJpg = Path.Combine(appCacheDir, $"{clientIcon}.jpg");
            if (File.Exists(appCacheJpg))
            {
                return appCacheJpg;
            }

            var appCachePng = Path.Combine(appCacheDir, $"{clientIcon}.png");
            if (File.Exists(appCachePng))
            {
                return appCachePng;
            }

            var appCacheIco = Path.Combine(appCacheDir, $"{clientIcon}.ico");
            if (File.Exists(appCacheIco))
            {
                return appCacheIco;
            }
        }

        var appCacheAppIdIconJpg = Path.Combine(steamPath, "appcache", "librarycache", $"{appId}_icon.jpg");
        if (File.Exists(appCacheAppIdIconJpg))
        {
            return appCacheAppIdIconJpg;
        }

        var appCacheAppIdIconPng = Path.Combine(steamPath, "appcache", "librarycache", $"{appId}_icon.png");
        if (File.Exists(appCacheAppIdIconPng))
        {
            return appCacheAppIdIconPng;
        }

        var iconJpg = Path.Combine(appCacheDir, "icon.jpg");
        if (File.Exists(iconJpg))
        {
            return iconJpg;
        }

        var iconPng = Path.Combine(appCacheDir, "icon.png");
        if (File.Exists(iconPng))
        {
            return iconPng;
        }

        var iconIco = Path.Combine(appCacheDir, "icon.ico");
        if (File.Exists(iconIco))
        {
            return iconIco;
        }

        return null;
    }

    public static void FetchMissingIconsAsync(IEnumerable<SteamGame> games)
    {
        var gamesNeedingIcons = games
            .Where(g => string.IsNullOrWhiteSpace(g.IconPath) && !string.IsNullOrWhiteSpace(g.ClientIconHash))
            .ToList();

        if (gamesNeedingIcons.Count == 0)
        {
            return;
        }

        Task.Run(async () =>
        {
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            var iconCacheDir = Path.Combine(localAppData, "SteamProcessIdler", "IconCache");
            Directory.CreateDirectory(iconCacheDir);

            using var semaphore = new SemaphoreSlim(6, 6);
            var tasks = gamesNeedingIcons.Select(async game =>
            {
                await semaphore.WaitAsync();
                try
                {
                    var icoPath = Path.Combine(iconCacheDir, $"{game.AppId}.ico");
                    var jpgPath = Path.Combine(iconCacheDir, $"{game.AppId}.jpg");

                    if (File.Exists(icoPath))
                    {
                        UpdateGameIconOnUI(game, icoPath);
                        return;
                    }

                    if (File.Exists(jpgPath))
                    {
                        UpdateGameIconOnUI(game, jpgPath);
                        return;
                    }

                    var icoUrl = $"https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{game.AppId}/{game.ClientIconHash}.ico";
                    var downloaded = await TryDownloadIconAsync(icoUrl, icoPath);
                    if (downloaded)
                    {
                        UpdateGameIconOnUI(game, icoPath);
                        return;
                    }

                    var jpgUrl = $"https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{game.AppId}/{game.ClientIconHash}.jpg";
                    downloaded = await TryDownloadIconAsync(jpgUrl, jpgPath);
                    if (downloaded)
                    {
                        UpdateGameIconOnUI(game, jpgPath);
                    }
                }
                catch
                {
                }
                finally
                {
                    semaphore.Release();
                }
            });

            await Task.WhenAll(tasks);
        });
    }

    private static async Task<bool> TryDownloadIconAsync(string url, string targetPath)
    {
        try
        {
            using var response = await HttpClientInstance.GetAsync(url, HttpCompletionOption.ResponseHeadersRead);
            if (response.IsSuccessStatusCode)
            {
                var bytes = await response.Content.ReadAsByteArrayAsync();
                if (bytes.Length > 0)
                {
                    await File.WriteAllBytesAsync(targetPath, bytes);
                    return true;
                }
            }
        }
        catch
        {
        }

        return false;
    }

    private static void UpdateGameIconOnUI(SteamGame game, string iconPath)
    {
        var app = Application.Current;
        if (app?.Dispatcher != null && !app.Dispatcher.CheckAccess())
        {
            app.Dispatcher.InvokeAsync(() => game.IconPath = iconPath);
        }
        else
        {
            game.IconPath = iconPath;
        }
    }

    private static bool IsValidExe(string? raw)
    {
        if (string.IsNullOrWhiteSpace(raw))
        {
            return false;
        }

        var cleaned = raw.Trim('\"', '\'', ' ');
        if (cleaned.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        int exeIdx = cleaned.IndexOf(".exe", StringComparison.OrdinalIgnoreCase);
        if (exeIdx >= 0)
        {
            int afterExe = exeIdx + 4;
            if (afterExe == cleaned.Length || char.IsWhiteSpace(cleaned[afterExe]))
            {
                return true;
            }
        }

        return false;
    }

    private static string CleanExecutablePath(string raw)
    {
        var cleaned = raw.Trim('\"', '\'', ' ');
        int exeIdx = cleaned.IndexOf(".exe", StringComparison.OrdinalIgnoreCase);
        if (exeIdx >= 0 && exeIdx + 4 < cleaned.Length)
        {
            cleaned = cleaned.Substring(0, exeIdx + 4);
        }

        return cleaned.Replace('/', '\\');
    }
}
