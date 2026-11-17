using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Threading;

namespace SteamProcessIdler.Services;

public static class SteamworksStubRunner
{
    private const string SteamApiDll = "steam_api64.dll";

    [DllImport(SteamApiDll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "SteamAPI_Init")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool SteamAPI_Init();

    [DllImport(SteamApiDll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "SteamAPI_Shutdown")]
    private static extern void SteamAPI_Shutdown();

    private static readonly ManualResetEvent ExitEvent = new(false);
    private static int _shutdownCalled;
    private static bool _initialized;
    private static DispatcherFrame? _frame;
    private static Dispatcher? _dispatcher;
    private static Window? _stubWindow;

    public static void Run(uint appId, string? gameTitle = null)
    {
        try
        {
            var baseDirectory = AppContext.BaseDirectory;
            var appIdFile = Path.Combine(baseDirectory, "steam_appid.txt");
            File.WriteAllText(appIdFile, appId.ToString());

            var localDllPath = Path.Combine(baseDirectory, SteamApiDll);
            if (!File.Exists(localDllPath) && !NativeLibrary.TryLoad(SteamApiDll, typeof(SteamworksStubRunner).Assembly, null, out _))
            {
                Console.Error.WriteLine($"[SteamworksStubRunner] '{SteamApiDll}' not found. Cannot initialize Steamworks for AppId {appId}.");
                return;
            }

            try
            {
                _initialized = SteamAPI_Init();
                if (!_initialized)
                {
                    Console.Error.WriteLine($"[SteamworksStubRunner] SteamAPI_Init() returned false for AppId {appId}.");
                }
            }
            catch (DllNotFoundException ex)
            {
                Console.Error.WriteLine($"[SteamworksStubRunner] Failed to load '{SteamApiDll}': {ex.Message}");
                return;
            }
            catch (EntryPointNotFoundException ex)
            {
                Console.Error.WriteLine($"[SteamworksStubRunner] SteamAPI_Init entry point not found in '{SteamApiDll}': {ex.Message}");
                return;
            }
            catch (BadImageFormatException ex)
            {
                Console.Error.WriteLine($"[SteamworksStubRunner] Incompatible '{SteamApiDll}' architecture: {ex.Message}");
                return;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"[SteamworksStubRunner] Error initializing SteamAPI: {ex.Message}");
                return;
            }

            var title = !string.IsNullOrWhiteSpace(gameTitle)
                ? gameTitle
                : (Path.GetFileNameWithoutExtension(Environment.ProcessPath) ?? appId.ToString());

            _stubWindow = new Window
            {
                Title = title,
                WindowStyle = WindowStyle.SingleBorderWindow,
                AllowsTransparency = false,
                Opacity = 1,
                Width = 1280,
                Height = 720,
                ShowInTaskbar = false,
                ShowActivated = false,
                WindowStartupLocation = WindowStartupLocation.Manual,
                Left = -32000,
                Top = -32000
            };
            _stubWindow.Show();
            new WindowInteropHelper(_stubWindow).EnsureHandle();

            _dispatcher = Dispatcher.CurrentDispatcher;
            _frame = new DispatcherFrame();

            AppDomain.CurrentDomain.ProcessExit += OnProcessExit;
            Console.CancelKeyPress += OnCancelKeyPress;

            Dispatcher.PushFrame(_frame);
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"[SteamworksStubRunner] Unexpected error during stub execution: {ex}");
        }
        finally
        {
            PerformShutdown();
        }
    }

    public static void Stop()
    {
        ExitEvent.Set();
        if (_frame != null)
        {
            _frame.Continue = false;
        }
        if (_dispatcher != null && !_dispatcher.HasShutdownStarted)
        {
            _dispatcher.InvokeShutdown();
        }
    }

    private static void OnProcessExit(object? sender, EventArgs e)
    {
        ExitEvent.Set();
        if (_frame != null)
        {
            _frame.Continue = false;
        }
        PerformShutdown();
    }

    private static void OnCancelKeyPress(object? sender, ConsoleCancelEventArgs e)
    {
        e.Cancel = true;
        ExitEvent.Set();
        if (_frame != null)
        {
            _frame.Continue = false;
        }
        PerformShutdown();
    }

    private static void PerformShutdown()
    {
        if (Interlocked.Exchange(ref _shutdownCalled, 1) != 0)
        {
            return;
        }

        try
        {
            if (_stubWindow != null && !_stubWindow.Dispatcher.HasShutdownStarted)
            {
                _stubWindow.Dispatcher.Invoke(() =>
                {
                    try
                    {
                        _stubWindow.Close();
                    }
                    catch
                    {
                    }
                });
            }
        }
        catch
        {
        }

        if (!_initialized)
        {
            return;
        }

        try
        {
            SteamAPI_Shutdown();
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"[SteamworksStubRunner] Error during SteamAPI_Shutdown: {ex.Message}");
        }
    }
}
