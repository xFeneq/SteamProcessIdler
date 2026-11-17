using System;
using System.Windows;
using SteamProcessIdler.Services;

namespace SteamProcessIdler;

public partial class App : Application
{
    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        var (stubAppId, gameTitle) = ParseStubArguments(e.Args);
        if (stubAppId.HasValue)
        {
            SteamworksStubRunner.Run(stubAppId.Value, gameTitle);
            Shutdown();
            return;
        }

        Exit += (_, _) =>
        {
            ProcessStubManager.Instance.StopAllSessions();
        };

        var mainWindow = new MainWindow();
        mainWindow.Show();
    }

    private static (uint? AppId, string? Name) ParseStubArguments(string[]? args)
    {
        if (args == null || args.Length == 0)
        {
            return (null, null);
        }

        uint? appId = null;
        string? name = null;

        for (int i = 0; i < args.Length; i++)
        {
            var arg = args[i];
            if (string.Equals(arg, "--stub", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length && uint.TryParse(args[i + 1], out var nextId))
                {
                    appId = nextId;
                }
            }
            else if (arg.StartsWith("--stub=", StringComparison.OrdinalIgnoreCase))
            {
                var val = arg.Substring("--stub=".Length);
                if (uint.TryParse(val, out var id))
                {
                    appId = id;
                }
            }
            else if (arg.StartsWith("--stub:", StringComparison.OrdinalIgnoreCase))
            {
                var val = arg.Substring("--stub:".Length);
                if (uint.TryParse(val, out var id))
                {
                    appId = id;
                }
            }
            else if (string.Equals(arg, "--name", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length)
                {
                    name = args[i + 1];
                }
            }
            else if (arg.StartsWith("--name=", StringComparison.OrdinalIgnoreCase))
            {
                name = arg.Substring("--name=".Length);
            }
            else if (arg.StartsWith("--name:", StringComparison.OrdinalIgnoreCase))
            {
                name = arg.Substring("--name:".Length);
            }
        }

        return (appId, name);
    }
}
