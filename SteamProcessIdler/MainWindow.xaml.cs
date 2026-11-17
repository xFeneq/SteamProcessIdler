using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Threading;
using Microsoft.Win32;
using SteamProcessIdler.Models;
using SteamProcessIdler.Services;

namespace SteamProcessIdler;

public partial class MainWindow : Window
{
    private readonly ObservableCollection<SteamGame> _allGames = new();
    private readonly List<SteamGame> _customGames = new();
    private ICollectionView? _gamesView;
    private readonly DispatcherTimer _timer;

    private static readonly SolidColorBrush StatusGreen = new((Color)ColorConverter.ConvertFromString("#57CB58"));
    private static readonly SolidColorBrush StatusRed = new((Color)ColorConverter.ConvertFromString("#E05252"));

    public MainWindow()
    {
        InitializeComponent();

        ActiveSessionsListBox.ItemsSource = ProcessStubManager.Instance.ActiveSessions;
        ProcessStubManager.Instance.ActiveSessions.CollectionChanged += (_, _) =>
        {
            UpdateActiveSessionsUi();
        };

        _timer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(1)
        };
        _timer.Tick += Timer_Tick;
        _timer.Start();

        SourceInitialized += (_, _) => WindowThemeHelper.ApplySystemTheme(this);
        SystemEvents.UserPreferenceChanged += SystemEvents_UserPreferenceChanged;

        Loaded += MainWindow_Loaded;
    }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        ProcessStubManager.Instance.CleanupOrphanedSessions();

        UpdateSteamConnectionStatus();
        UpdateSteamApiDllStatus();
        UpdateActiveSessionsUi();

        await LoadLibraryAsync();
    }

    private void Timer_Tick(object? sender, EventArgs e)
    {
        var sessions = ProcessStubManager.Instance.ActiveSessions;
        for (int i = 0; i < sessions.Count; i++)
        {
            sessions[i].RefreshElapsed();
        }
    }

    private void UpdateSteamConnectionStatus()
    {
        var steamPath = SteamLibraryScanner.GetSteamInstallationPath();
        bool isConnected = !string.IsNullOrWhiteSpace(steamPath) && Directory.Exists(steamPath);

        if (isConnected)
        {
            SteamStatusDot.Fill = StatusGreen;
            SteamStatusText.Text = "Steam: Detected";
            SteamStatusBorder.ToolTip = $"Steam Installation Directory:\n{steamPath}";
        }
        else
        {
            SteamStatusDot.Fill = StatusRed;
            SteamStatusText.Text = "Steam: Not detected";
            SteamStatusBorder.ToolTip = "Steam client installation was not detected in Windows Registry.";
        }
    }

    private void UpdateSteamApiDllStatus()
    {
        bool isAvailable = ProcessStubManager.Instance.IsSteamApiAvailable;

        if (isAvailable)
        {
            DllStatusDot.Fill = StatusGreen;
            DllStatusText.Text = "steam_api64.dll: Ready";
            SteamApiStatusButton.ToolTip = "steam_api64.dll is present and ready for use.";
        }
        else
        {
            DllStatusDot.Fill = StatusRed;
            DllStatusText.Text = "steam_api64.dll: Missing (click)";
            SteamApiStatusButton.ToolTip = "Click to learn how to acquire steam_api64.dll.";
        }
    }

    private void SteamApiStatusButton_Click(object sender, RoutedEventArgs e)
    {
        bool wasAvailable = ProcessStubManager.Instance.IsSteamApiAvailable;
        if (!wasAvailable)
        {
            if (ProcessStubManager.Instance.EnsureSteamApiDll())
            {
                UpdateSteamApiDllStatus();
                MessageBox.Show(
                    "Successfully located and copied steam_api64.dll from local Steam files!",
                    "steam_api64.dll Located",
                    MessageBoxButton.OK,
                    MessageBoxImage.Information);
                return;
            }
        }

        var appDir = AppContext.BaseDirectory;
        var message = wasAvailable
            ? $"steam_api64.dll is installed and ready for use.\n\nApplication directory:\n{appDir}"
            : "A 64-bit Steamworks library (steam_api64.dll) is required to simulate Steam game status in the background.\n\n" +
              "Where to obtain steam_api64.dll:\n" +
              "1. Copy steam_api64.dll from any installed 64-bit Steam game on your PC (e.g., CS2, Apex Legends, Satisfactory, Astroneer, etc.), or\n" +
              "2. Download the official Steamworks SDK from Valve (partner.steamgames.com) in 'redistributable_bin/win64/'.\n\n" +
              $"Place steam_api64.dll into the application folder:\n{appDir}";

        MessageBox.Show(message, "steam_api64.dll Status", MessageBoxButton.OK,
            wasAvailable ? MessageBoxImage.Information : MessageBoxImage.Warning);

        UpdateSteamApiDllStatus();
    }

    private async Task LoadLibraryAsync()
    {
        LoadingOverlay.Visibility = Visibility.Visible;
        RefreshButton.IsEnabled = false;

        try
        {
            var scannedGames = await Task.Run(() => SteamLibraryScanner.Scan());

            _allGames.Clear();
            _customGames.Clear();

            GameOverrideManager.ApplyOverrides(scannedGames, _customGames);

            foreach (var game in scannedGames)
            {
                _allGames.Add(game);
            }

            foreach (var customGame in _customGames)
            {
                if (!_allGames.Any(g => g.AppId == customGame.AppId))
                {
                    _allGames.Add(customGame);
                }
            }

            foreach (var session in ProcessStubManager.Instance.ActiveSessions)
            {
                var matchingGame = _allGames.FirstOrDefault(g => g.AppId == session.Game.AppId);
                if (matchingGame != null)
                {
                    matchingGame.IsRunning = true;
                }
            }

            _gamesView = CollectionViewSource.GetDefaultView(_allGames);
            _gamesView.Filter = FilterGamePredicate;
            _gamesView.SortDescriptions.Clear();
            _gamesView.SortDescriptions.Add(new SortDescription(nameof(SteamGame.Name), ListSortDirection.Ascending));

            GamesListBox.ItemsSource = _gamesView;

            UpdateGamesCount();
            UpdateFilteredCount();

            SteamLibraryScanner.FetchMissingIconsAsync(_allGames);
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Error scanning Steam library: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
        finally
        {
            LoadingOverlay.Visibility = Visibility.Collapsed;
            RefreshButton.IsEnabled = true;
        }
    }

    private bool FilterGamePredicate(object item)
    {
        if (item is not SteamGame game)
        {
            return false;
        }

        var filter = SearchBox.Text?.Trim();
        if (string.IsNullOrWhiteSpace(filter))
        {
            return true;
        }

        if (game.Name.Contains(filter, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        if (game.AppId.ToString().Contains(filter))
        {
            return true;
        }

        if (!string.IsNullOrEmpty(game.ExecutableRelativePath) &&
            game.ExecutableRelativePath.Contains(filter, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        if (game.AvailableExecutables != null &&
            game.AvailableExecutables.Any(e => e.Contains(filter, StringComparison.OrdinalIgnoreCase)))
        {
            return true;
        }

        if (!string.IsNullOrEmpty(game.InstallDir) &&
            game.InstallDir.Contains(filter, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        return false;
    }

    private void SearchBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        _gamesView?.Refresh();
        UpdateFilteredCount();
    }

    private void UpdateGamesCount()
    {
        int total = _allGames.Count;
        GamesCountBadge.Text = $"{total} {(total == 1 ? "game" : "games")}";
    }

    private void UpdateFilteredCount()
    {
        if (_gamesView is null)
        {
            FilteredCountText.Text = $"Visible: {_allGames.Count}";
            return;
        }

        int visibleCount = _gamesView.Cast<object>().Count();
        FilteredCountText.Text = $"Visible: {visibleCount} / {_allGames.Count}";
    }

    private async void RefreshButton_Click(object sender, RoutedEventArgs e)
    {
        UpdateSteamConnectionStatus();
        UpdateSteamApiDllStatus();
        await LoadLibraryAsync();
    }

    private void EditGameButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button btn || btn.Tag is not SteamGame game)
        {
            return;
        }

        var dialog = new AddCustomGameWindow(game)
        {
            Owner = this
        };

        if (dialog.ShowDialog() == true)
        {
            _gamesView?.Refresh();
        }
    }

    private void AddCustomButton_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new AddCustomGameWindow
        {
            Owner = this
        };

        if (dialog.ShowDialog() == true && dialog.CreatedGame != null)
        {
            var newGame = dialog.CreatedGame;

            var existing = _allGames.FirstOrDefault(g => g.AppId == newGame.AppId);
            if (existing != null)
            {
                MessageBox.Show($"A game with AppID {newGame.AppId} ({existing.Name}) already exists in the library.",
                    "Game Already Exists", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            _customGames.Add(newGame);
            _allGames.Add(newGame);

            _gamesView?.Refresh();
            UpdateGamesCount();
            UpdateFilteredCount();

            GamesListBox.SelectedItem = newGame;
            GamesListBox.ScrollIntoView(newGame);

            SteamLibraryScanner.FetchMissingIconsAsync(new[] { newGame });
        }
    }

    private void StartGameButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button btn || btn.Tag is not SteamGame game)
        {
            return;
        }

        if (ProcessStubManager.Instance.ActiveSessions.Any(s => s.Game.AppId == game.AppId))
        {
            MessageBox.Show($"{game.Name} (AppID: {game.AppId}) is already running.",
                "Session Active", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        try
        {
            var session = ProcessStubManager.Instance.StartGameSession(game);
            if (session == null)
            {
                MessageBox.Show($"Failed to start session for {game.Name}.", "Launch Error", MessageBoxButton.OK, MessageBoxImage.Warning);
            }
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Error starting game session: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private void StopGameFromListButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button btn || btn.Tag is not SteamGame game)
        {
            return;
        }

        var session = ProcessStubManager.Instance.ActiveSessions.FirstOrDefault(s => s.Game.AppId == game.AppId);
        if (session != null)
        {
            ProcessStubManager.Instance.StopSession(session);
        }
        else
        {
            game.IsRunning = false;
        }
    }

    private void StopSessionButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button btn || btn.Tag is not ActiveSession session)
        {
            return;
        }

        ProcessStubManager.Instance.StopSession(session);
    }

    private void StopAllButton_Click(object sender, RoutedEventArgs e)
    {
        ProcessStubManager.Instance.StopAllSessions();
    }

    private void UpdateActiveSessionsUi()
    {
        int count = ProcessStubManager.Instance.ActiveSessions.Count;
        ActiveSessionsHeaderCount.Text = $"({count})";
        EmptySessionsPlaceholder.Visibility = count == 0 ? Visibility.Visible : Visibility.Collapsed;
        StopAllButton.IsEnabled = count > 0;
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
