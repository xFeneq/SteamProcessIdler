using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
using System.Windows;
using SteamProcessIdler.Models;

namespace SteamProcessIdler.Services;

public class ProcessStubManager
{
    private readonly object _lock = new();
    private readonly HashSet<uint> _stoppingAppIds = new();

    public static ProcessStubManager Instance { get; } = new();

    public ObservableCollection<ActiveSession> ActiveSessions { get; } = new();

    public bool IsSteamApiAvailable => EnsureSteamApiDll();

    public static void ValidateRelativePath(string? path, string paramName)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        if (Path.IsPathRooted(path))
        {
            throw new InvalidOperationException($"The parameter '{paramName}' must be a relative path, but a rooted path was provided: '{path}'.");
        }

        if (path.Contains(".."))
        {
            throw new InvalidOperationException($"The parameter '{paramName}' cannot contain parent directory traversal ('..'): '{path}'.");
        }
    }

    public bool EnsureSteamApiDll()
    {
        var baseDir = GetAppBinDirectory();
        var targetDll = Path.Combine(baseDir, "steam_api64.dll");
        if (File.Exists(targetDll))
        {
            return true;
        }

        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var appDataDll = Path.Combine(localAppData, "SteamProcessIdler", "steam_api64.dll");
        if (File.Exists(appDataDll))
        {
            try
            {
                File.Copy(appDataDll, targetDll, overwrite: true);
                return true;
            }
            catch
            {
                return true;
            }
        }

        var steamPath = SteamLibraryScanner.GetSteamInstallationPath();
        if (string.IsNullOrWhiteSpace(steamPath) || !Directory.Exists(steamPath))
        {
            return false;
        }

        string? foundSource = null;

        var candidatePaths = new[]
        {
            Path.Combine(steamPath, "steam_api64.dll"),
            Path.Combine(steamPath, "bin", "steam_api64.dll")
        };

        foreach (var candidate in candidatePaths)
        {
            if (File.Exists(candidate))
            {
                foundSource = candidate;
                break;
            }
        }

        if (foundSource == null)
        {
            var libraryFoldersPath = Path.Combine(steamPath, "steamapps", "libraryfolders.vdf");
            if (!File.Exists(libraryFoldersPath))
            {
                libraryFoldersPath = Path.Combine(steamPath, "config", "libraryfolders.vdf");
            }

            if (File.Exists(libraryFoldersPath))
            {
                try
                {
                    var content = File.ReadAllText(libraryFoldersPath);
                    var matches = Regex.Matches(content, @"""path""\s*""([^""]+)""");
                    foreach (Match match in matches)
                    {
                        if (match.Success)
                        {
                            var libPath = match.Groups[1].Value.Replace(@"\\", @"\");
                            var commonDir = Path.Combine(libPath, "steamapps", "common");
                            if (Directory.Exists(commonDir))
                            {
                                var foundDll = Directory.EnumerateFiles(commonDir, "steam_api64.dll", new EnumerationOptions
                                {
                                    RecurseSubdirectories = true,
                                    MaxRecursionDepth = 5,
                                    IgnoreInaccessible = true
                                }).FirstOrDefault();

                                if (foundDll != null && File.Exists(foundDll))
                                {
                                    foundSource = foundDll;
                                    break;
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

        if (foundSource != null)
        {
            bool copiedToBase = false;
            try
            {
                File.Copy(foundSource, targetDll, overwrite: true);
                copiedToBase = true;
            }
            catch
            {
            }

            try
            {
                var appDataDir = Path.GetDirectoryName(appDataDll)!;
                Directory.CreateDirectory(appDataDir);
                File.Copy(foundSource, appDataDll, overwrite: true);
            }
            catch
            {
            }

            return copiedToBase || File.Exists(appDataDll);
        }

        return false;
    }

    public ActiveSession? StartGameSession(SteamGame game)
    {
        ArgumentNullException.ThrowIfNull(game);

        lock (_lock)
        {
            if (ActiveSessions.Any(s => s.Game.AppId == game.AppId))
            {
                Debug.WriteLine($"[ProcessStubManager] AppId {game.AppId} ({game.Name}) is already active. Ignoring launch request.");
                return null;
            }
        }

        ValidateRelativePath(game.InstallDir, nameof(game.InstallDir));
        ValidateRelativePath(game.ExecutableRelativePath, nameof(game.ExecutableRelativePath));

        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var sessionsRootDir = Path.GetFullPath(Path.Combine(localAppData, "SteamProcessIdler", "Sessions"));
        var sessionRootDir = Path.GetFullPath(Path.Combine(sessionsRootDir, game.AppId.ToString()));
        var sessionRootWithSlash = sessionRootDir.EndsWith(Path.DirectorySeparatorChar)
            ? sessionRootDir
            : sessionRootDir + Path.DirectorySeparatorChar;

        var exeSubDir = Path.GetDirectoryName(game.ExecutableRelativePath) ?? string.Empty;
        ValidateRelativePath(exeSubDir, nameof(exeSubDir));

        var targetExeDir = !string.IsNullOrWhiteSpace(game.InstallDir)
            ? Path.Combine(sessionRootDir, game.InstallDir, exeSubDir)
            : Path.Combine(sessionRootDir, exeSubDir);

        var fullTargetExeDir = Path.GetFullPath(targetExeDir);

        if (!fullTargetExeDir.StartsWith(sessionRootWithSlash, StringComparison.OrdinalIgnoreCase) &&
            !string.Equals(fullTargetExeDir, sessionRootDir, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException($"The target directory '{fullTargetExeDir}' escapes the session sandbox '{sessionRootDir}'.");
        }

        Directory.CreateDirectory(fullTargetExeDir);

        var hostExePath = GetHostExecutablePath();
        var exeFileName = Path.GetFileName(game.ExecutableRelativePath);
        if (string.IsNullOrWhiteSpace(exeFileName))
        {
            exeFileName = "game.exe";
        }
        if (!exeFileName.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
        {
            exeFileName += ".exe";
        }

        var targetExePath = Path.Combine(fullTargetExeDir, exeFileName);
        var fullTargetExePath = Path.GetFullPath(targetExePath);

        if (!fullTargetExePath.StartsWith(sessionRootWithSlash, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException($"The executable path '{fullTargetExePath}' escapes the session sandbox '{sessionRootDir}'.");
        }

        File.Copy(hostExePath, fullTargetExePath, overwrite: true);

        var baseDir = GetAppBinDirectory();
        foreach (var file in Directory.EnumerateFiles(baseDir))
        {
            var ext = Path.GetExtension(file);
            if (ext.Equals(".dll", StringComparison.OrdinalIgnoreCase) ||
                ext.Equals(".json", StringComparison.OrdinalIgnoreCase))
            {
                var destFile = Path.Combine(fullTargetExeDir, Path.GetFileName(file));
                File.Copy(file, destFile, overwrite: true);
            }
        }

        EnsureSteamApiDll();
        var steamApiSource = Path.Combine(baseDir, "steam_api64.dll");
        if (!File.Exists(steamApiSource))
        {
            var appDataDll = Path.Combine(localAppData, "SteamProcessIdler", "steam_api64.dll");
            if (File.Exists(appDataDll))
            {
                steamApiSource = appDataDll;
            }
        }

        if (File.Exists(steamApiSource))
        {
            var targetSteamApi = Path.Combine(fullTargetExeDir, "steam_api64.dll");
            File.Copy(steamApiSource, targetSteamApi, overwrite: true);
        }

        var appIdFile = Path.Combine(fullTargetExeDir, "steam_appid.txt");
        File.WriteAllText(appIdFile, game.AppId.ToString());

        var escapedName = (game.Name ?? string.Empty).Replace("\"", "\\\"");
        var startInfo = new ProcessStartInfo
        {
            FileName = fullTargetExePath,
            Arguments = $"--stub {game.AppId} --name \"{escapedName}\"",
            WorkingDirectory = fullTargetExeDir,
            UseShellExecute = false,
            CreateNoWindow = true
        };

        var process = new Process
        {
            StartInfo = startInfo,
            EnableRaisingEvents = true
        };

        if (!process.Start())
        {
            SafeDeleteDirectory(sessionRootDir);
            throw new InvalidOperationException($"Failed to start process: {fullTargetExePath}");
        }

        var session = new ActiveSession
        {
            Game = game,
            Process = process,
            StartTime = DateTime.Now,
            TempDirectory = sessionRootDir,
            ProcessName = exeFileName
        };

        RunOnUIThread(() =>
        {
            game.IsRunning = true;
            ActiveSessions.Add(session);
        });

        process.Exited += (_, _) =>
        {
            StopSession(session);
        };

        return session;
    }

    public void StopSession(ActiveSession? session)
    {
        if (session == null)
        {
            return;
        }

        lock (_lock)
        {
            if (!_stoppingAppIds.Add(session.Game.AppId))
            {
                return;
            }
        }

        try
        {
            RunOnUIThread(() =>
            {
                session.Game.IsRunning = false;
                ActiveSessions.Remove(session);
            });

            try
            {
                if (!session.Process.HasExited)
                {
                    session.Process.Kill(entireProcessTree: true);
                    session.Process.WaitForExit(2000);
                }
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"[ProcessStubManager] Error terminating process for {session.Game.Name}: {ex.Message}");
            }

            SafeDeleteDirectory(session.TempDirectory);

            try
            {
                session.Process.Dispose();
            }
            catch
            {
            }
        }
        finally
        {
            lock (_lock)
            {
                _stoppingAppIds.Remove(session.Game.AppId);
            }
        }
    }

    public void StopAllSessions()
    {
        List<ActiveSession> sessionsToStop;
        lock (_lock)
        {
            sessionsToStop = ActiveSessions.ToList();
        }

        foreach (var session in sessionsToStop)
        {
            StopSession(session);
        }
    }

    public void CleanupOrphanedSessions()
    {
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var sessionsDir = Path.Combine(localAppData, "SteamProcessIdler", "Sessions");
        if (!Directory.Exists(sessionsDir))
        {
            return;
        }

        var runningProcessPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        try
        {
            foreach (var proc in Process.GetProcesses())
            {
                try
                {
                    var mod = proc.MainModule?.FileName;
                    if (!string.IsNullOrWhiteSpace(mod))
                    {
                        runningProcessPaths.Add(Path.GetFullPath(mod));
                    }
                }
                catch
                {
                }
                finally
                {
                    proc.Dispose();
                }
            }
        }
        catch
        {
        }

        foreach (var subDir in Directory.EnumerateDirectories(sessionsDir))
        {
            var fullSubDir = Path.GetFullPath(subDir);
            var subDirWithSlash = fullSubDir.EndsWith(Path.DirectorySeparatorChar)
                ? fullSubDir
                : fullSubDir + Path.DirectorySeparatorChar;

            bool hasRunningProcess = runningProcessPaths.Any(p => p.StartsWith(subDirWithSlash, StringComparison.OrdinalIgnoreCase));
            if (!hasRunningProcess)
            {
                try
                {
                    SafeDeleteDirectory(fullSubDir);
                }
                catch
                {
                }
            }
        }
    }

    private static string GetHostExecutablePath()
    {
        var procPath = Environment.ProcessPath;
        if (!string.IsNullOrWhiteSpace(procPath) &&
            File.Exists(procPath) &&
            procPath.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) &&
            !Path.GetFileName(procPath).Equals("dotnet.exe", StringComparison.OrdinalIgnoreCase) &&
            !Path.GetFileName(procPath).Equals("pwsh.exe", StringComparison.OrdinalIgnoreCase) &&
            !Path.GetFileName(procPath).Equals("powershell.exe", StringComparison.OrdinalIgnoreCase))
        {
            return procPath;
        }

        try
        {
            using var currentProc = Process.GetCurrentProcess();
            var mainModuleFile = currentProc.MainModule?.FileName;
            if (!string.IsNullOrWhiteSpace(mainModuleFile) &&
                File.Exists(mainModuleFile) &&
                mainModuleFile.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(mainModuleFile).Equals("dotnet.exe", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(mainModuleFile).Equals("pwsh.exe", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(mainModuleFile).Equals("powershell.exe", StringComparison.OrdinalIgnoreCase))
            {
                return mainModuleFile;
            }
        }
        catch
        {
        }

        var baseDir = GetAppBinDirectory();
        var appName = AppDomain.CurrentDomain.FriendlyName;
        var appExeInBaseDir = Path.Combine(baseDir, $"{appName}.exe");
        if (File.Exists(appExeInBaseDir))
        {
            return appExeInBaseDir;
        }

        var defaultExe = Path.Combine(baseDir, "SteamProcessIdler.exe");
        if (File.Exists(defaultExe))
        {
            return defaultExe;
        }

        throw new FileNotFoundException("Cannot locate SteamProcessIdler host executable (.exe).");
    }

    [System.Diagnostics.CodeAnalysis.UnconditionalSuppressMessage("SingleFile", "IL3000", Justification = "Fallback for external test environments")]
    private static string GetAppBinDirectory()
    {
        var baseDir = AppContext.BaseDirectory;
        if (File.Exists(Path.Combine(baseDir, "SteamProcessIdler.exe")) || File.Exists(Path.Combine(baseDir, "SteamProcessIdler.dll")))
        {
            return baseDir;
        }

        var asmLocation = typeof(ProcessStubManager).Assembly.Location;
        if (!string.IsNullOrWhiteSpace(asmLocation))
        {
            var asmDir = Path.GetDirectoryName(asmLocation);
            if (!string.IsNullOrWhiteSpace(asmDir) && Directory.Exists(asmDir))
            {
                return asmDir;
            }
        }

        return baseDir;
    }

    private static void SafeDeleteDirectory(string dirPath, int maxRetries = 5, int delayMs = 300)
    {
        if (string.IsNullOrWhiteSpace(dirPath) || !Directory.Exists(dirPath))
        {
            return;
        }

        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var sessionsRootDir = Path.GetFullPath(Path.Combine(localAppData, "SteamProcessIdler", "Sessions"));
        var sessionsRootWithSlash = sessionsRootDir.EndsWith(Path.DirectorySeparatorChar)
            ? sessionsRootDir
            : sessionsRootDir + Path.DirectorySeparatorChar;

        var fullTargetDir = Path.GetFullPath(dirPath);

        if (string.Equals(fullTargetDir, sessionsRootDir, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException($"Refusing to delete the root Sessions directory: '{fullTargetDir}'.");
        }

        if (!fullTargetDir.StartsWith(sessionsRootWithSlash, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException($"Refusing to delete directory outside the Sessions sandbox: '{fullTargetDir}'.");
        }

        RemoveReadOnlyAttributes(fullTargetDir);

        for (int i = 0; i < maxRetries; i++)
        {
            try
            {
                Directory.Delete(fullTargetDir, recursive: true);
                return;
            }
            catch (DirectoryNotFoundException)
            {
                return;
            }
            catch (Exception ex) when (i < maxRetries - 1 && (ex is IOException || ex is UnauthorizedAccessException))
            {
                Thread.Sleep(delayMs);
                RemoveReadOnlyAttributes(fullTargetDir);
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"[ProcessStubManager] Failed to delete directory '{fullTargetDir}': {ex.Message}");
                break;
            }
        }
    }

    private static void RemoveReadOnlyAttributes(string dirPath)
    {
        try
        {
            var dirInfo = new DirectoryInfo(dirPath);
            if (dirInfo.Exists)
            {
                dirInfo.Attributes &= ~FileAttributes.ReadOnly;
                foreach (var file in dirInfo.EnumerateFiles("*", SearchOption.AllDirectories))
                {
                    file.Attributes &= ~FileAttributes.ReadOnly;
                }
            }
        }
        catch
        {
        }
    }

    private static void RunOnUIThread(Action action)
    {
        var app = Application.Current;
        if (app?.Dispatcher != null && !app.Dispatcher.CheckAccess())
        {
            app.Dispatcher.Invoke(action);
        }
        else
        {
            action();
        }
    }
}
