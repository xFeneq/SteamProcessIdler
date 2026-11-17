using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using Microsoft.Win32;

namespace SteamProcessIdler.Services;

public static class WindowThemeHelper
{
    private const int DWMWA_USE_IMMERSIVE_DARK_MODE = 20;
    private const int DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1 = 19;

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attr, ref int attrValue, int attrSize);

    public static bool IsDarkThemeEnabled()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            var value = key?.GetValue("AppsUseLightTheme");
            if (value is int intValue)
            {
                return intValue == 0;
            }
        }
        catch
        {
        }

        return true;
    }

    public static void ApplySystemTheme(IntPtr handle)
    {
        if (handle == IntPtr.Zero)
        {
            return;
        }

        int useDarkMode = IsDarkThemeEnabled() ? 1 : 0;
        if (DwmSetWindowAttribute(handle, DWMWA_USE_IMMERSIVE_DARK_MODE, ref useDarkMode, sizeof(int)) != 0)
        {
            DwmSetWindowAttribute(handle, DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1, ref useDarkMode, sizeof(int));
        }
    }

    public static void ApplySystemTheme(Window window)
    {
        var handle = new WindowInteropHelper(window).Handle;
        ApplySystemTheme(handle);
    }
}
