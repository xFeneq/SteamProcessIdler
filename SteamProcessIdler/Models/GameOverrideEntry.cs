namespace SteamProcessIdler.Models;

public class GameOverrideEntry
{
    public uint AppId { get; set; }

    public string Name { get; set; } = string.Empty;

    public string InstallDir { get; set; } = string.Empty;

    public string ExecutableRelativePath { get; set; } = string.Empty;

    public bool IsCustom { get; set; }
}
