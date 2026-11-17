using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.CompilerServices;

namespace SteamProcessIdler.Models;

public class ActiveSession : INotifyPropertyChanged
{
    private SteamGame _game = null!;
    private Process _process = null!;
    private DateTime _startTime;
    private string _tempDirectory = string.Empty;
    private string _processName = string.Empty;

    public event PropertyChangedEventHandler? PropertyChanged;

    public SteamGame Game
    {
        get => _game;
        set => SetField(ref _game, value);
    }

    public Process Process
    {
        get => _process;
        set => SetField(ref _process, value);
    }

    public DateTime StartTime
    {
        get => _startTime;
        set
        {
            if (SetField(ref _startTime, value))
            {
                OnPropertyChanged(nameof(Elapsed));
                OnPropertyChanged(nameof(ElapsedFormatted));
            }
        }
    }

    public string TempDirectory
    {
        get => _tempDirectory;
        set => SetField(ref _tempDirectory, value);
    }

    public string ProcessName
    {
        get => _processName;
        set => SetField(ref _processName, value);
    }

    public TimeSpan Elapsed => DateTime.Now >= StartTime ? DateTime.Now - StartTime : TimeSpan.Zero;

    public string ElapsedFormatted
    {
        get
        {
            var elapsed = Elapsed;
            if (elapsed.TotalHours >= 24)
            {
                return $"{(int)elapsed.TotalHours:D2}:{elapsed:mm\\:ss}";
            }

            return elapsed.ToString(@"hh\:mm\:ss");
        }
    }

    public void RefreshElapsed()
    {
        OnPropertyChanged(nameof(Elapsed));
        OnPropertyChanged(nameof(ElapsedFormatted));
    }

    protected virtual void OnPropertyChanged([CallerMemberName] string? propertyName = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }

    protected bool SetField<T>(ref T field, T value, [CallerMemberName] string? propertyName = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value))
        {
            return false;
        }

        field = value;
        OnPropertyChanged(propertyName);
        return true;
    }
}
