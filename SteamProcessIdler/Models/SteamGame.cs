using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace SteamProcessIdler.Models;

public class SteamGame : INotifyPropertyChanged
{
    private string _executableRelativePath = string.Empty;
    private string? _iconPath;
    private string? _clientIconHash;
    private bool _isRunning;

    public event PropertyChangedEventHandler? PropertyChanged;

    public uint AppId { get; set; }

    public string Name { get; set; } = string.Empty;

    public string InstallDir { get; set; } = string.Empty;

    public string ExecutableRelativePath
    {
        get => _executableRelativePath;
        set => SetField(ref _executableRelativePath, value);
    }

    public List<string> AvailableExecutables { get; set; } = new();

    public bool HasMultipleExecutables => AvailableExecutables != null && AvailableExecutables.Count > 1;

    public string? IconPath
    {
        get => _iconPath;
        set => SetField(ref _iconPath, value);
    }

    public string? ClientIconHash
    {
        get => _clientIconHash;
        set => SetField(ref _clientIconHash, value);
    }

    public bool IsRunning
    {
        get => _isRunning;
        set => SetField(ref _isRunning, value);
    }

    public bool IsCustom { get; set; }

    public override string ToString() => $"{Name} ({AppId})";

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
