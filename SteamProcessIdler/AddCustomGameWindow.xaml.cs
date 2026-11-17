using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Windows;
using Microsoft.Win32;
using SteamProcessIdler.Models;
using SteamProcessIdler.Services;

namespace SteamProcessIdler;

public partial class AddCustomGameWindow : Window
{
    private readonly SteamGame? _existingGame;

    public SteamGame? CreatedGame { get; private set; }

    public AddCustomGameWindow()
    {
        InitializeComponent();
        SourceInitialized += (_, _) => WindowThemeHelper.ApplySystemTheme(this);
        SystemEvents.UserPreferenceChanged += SystemEvents_UserPreferenceChanged;
        Loaded += (_, _) => AppIdTextBox.Focus();
    }

    public AddCustomGameWindow(SteamGame existingGame) : this()
    {
        _existingGame = existingGame;

        Title = $"Edit Path — {existingGame.Name}";
        HeaderTitleText.Text = "Edit Executable Path";
        HeaderSubtitleText.Text = $"Customize executable path and install directory for {existingGame.Name}.";
        SaveButton.Content = "Save Changes";

        AppIdTextBox.Text = existingGame.AppId.ToString();
        AppIdTextBox.IsEnabled = false;

        GameNameTextBox.Text = existingGame.Name;
        InstallDirTextBox.Text = existingGame.InstallDir;
        ExecutableTextBox.Text = existingGame.ExecutableRelativePath;

        Loaded += (_, _) => ExecutableTextBox.Focus();
    }

    private void SaveButton_Click(object sender, RoutedEventArgs e)
    {
        ErrorTextBlock.Visibility = Visibility.Collapsed;
        ErrorTextBlock.Text = string.Empty;

        var appIdText = AppIdTextBox.Text?.Trim();
        if (string.IsNullOrWhiteSpace(appIdText) || !uint.TryParse(appIdText, out var appId) || appId == 0)
        {
            ShowError("Please enter a valid numeric Steam AppID (greater than 0).");
            AppIdTextBox.Focus();
            return;
        }

        var gameName = GameNameTextBox.Text?.Trim();
        if (string.IsNullOrWhiteSpace(gameName))
        {
            ShowError("Game title cannot be empty.");
            GameNameTextBox.Focus();
            return;
        }

        var installDir = InstallDirTextBox.Text?.Trim() ?? string.Empty;
        if (!string.IsNullOrWhiteSpace(installDir))
        {
            if (Path.IsPathRooted(installDir))
            {
                ShowError("Install directory must be a relative directory name, not a rooted path.");
                InstallDirTextBox.Focus();
                return;
            }

            if (installDir.Contains(".."))
            {
                ShowError("Install directory cannot contain parent directory traversal ('..').");
                InstallDirTextBox.Focus();
                return;
            }
        }

        var executable = ExecutableTextBox.Text?.Trim();
        if (string.IsNullOrWhiteSpace(executable))
        {
            ShowError("Please specify the executable filename (e.g., game.exe).");
            ExecutableTextBox.Focus();
            return;
        }

        if (Path.IsPathRooted(executable))
        {
            ShowError("Executable path must be relative to the install directory, not a rooted path.");
            ExecutableTextBox.Focus();
            return;
        }

        if (executable.Contains(".."))
        {
            ShowError("Executable path cannot contain parent directory traversal ('..').");
            ExecutableTextBox.Focus();
            return;
        }

        if (!executable.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
        {
            executable += ".exe";
        }

        var cleanExe = executable.Replace('/', Path.DirectorySeparatorChar);

        if (_existingGame != null)
        {
            _existingGame.InstallDir = installDir;
            _existingGame.ExecutableRelativePath = cleanExe;
            if (_existingGame.IsCustom)
            {
                _existingGame.Name = gameName;
            }

            if (!_existingGame.AvailableExecutables.Contains(cleanExe, StringComparer.OrdinalIgnoreCase))
            {
                _existingGame.AvailableExecutables.Insert(0, cleanExe);
            }

            GameOverrideManager.SaveOrUpdateOverride(_existingGame);
            DialogResult = true;
            Close();
            return;
        }

        CreatedGame = new SteamGame
        {
            AppId = appId,
            Name = gameName,
            InstallDir = installDir,
            ExecutableRelativePath = cleanExe,
            AvailableExecutables = new List<string> { cleanExe },
            IsCustom = true
        };

        GameOverrideManager.SaveOrUpdateOverride(CreatedGame);
        DialogResult = true;
        Close();
    }

    private void CancelButton_Click(object sender, RoutedEventArgs e)
    {
        DialogResult = false;
        Close();
    }

    private void ShowError(string message)
    {
        ErrorTextBlock.Text = message;
        ErrorTextBlock.Visibility = Visibility.Visible;
    }

    private void SystemEvents_UserPreferenceChanged(object sender, UserPreferenceChangedEventArgs e)
    {
        Dispatcher.Invoke(() => WindowThemeHelper.ApplySystemTheme(this));
    }

    protected override void OnClosed(EventArgs e)
    {
        SystemEvents.UserPreferenceChanged -= SystemEvents_UserPreferenceChanged;
        base.OnClosed(e);
    }
}
