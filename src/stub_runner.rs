use libloading::Library;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, ShowWindow, TranslateMessage, MSG, SW_SHOWNOACTIVATE,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW,
};

static RUNNING: AtomicBool = AtomicBool::new(true);

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        0x0010 => {
            RUNNING.store(false, Ordering::SeqCst);
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        0x0002 => {
            RUNNING.store(false, Ordering::SeqCst);
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn run(app_id: u32, game_title: Option<String>) {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let app_id_file = current_dir.join("steam_appid.txt");
    let _ = fs::write(app_id_file, app_id.to_string());

    let dll_path = current_dir.join("steam_api64.dll");
    let steam_lib = if dll_path.exists() {
        unsafe { Library::new(&dll_path).ok() }
    } else {
        unsafe { Library::new("steam_api64.dll").ok() }
    };

    let steam_initialized = if let Some(ref lib) = steam_lib {
        unsafe {
            if let Ok(init_fn) = lib.get::<unsafe extern "C" fn() -> bool>(b"SteamAPI_Init") {
                init_fn()
            } else {
                false
            }
        }
    } else {
        false
    };

    let title = game_title.unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| app_id.to_string())
    });

    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name: Vec<u16> = "SteamProcessIdlerStubClass\0".encode_utf16().collect();
        let title_wide: Vec<u16> = format!("{}\0", title).encode_utf16().collect();

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        let _ = RegisterClassW(&wnd_class);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            -32000,
            -32000,
            1280,
            720,
            None,
            None,
            instance,
            None,
        );

        if let Ok(valid_hwnd) = hwnd {
            let _ = ShowWindow(valid_hwnd, SW_SHOWNOACTIVATE);
        }

        let mut msg = MSG::default();
        while RUNNING.load(Ordering::SeqCst) && GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    if steam_initialized {
        if let Some(ref lib) = steam_lib {
            unsafe {
                if let Ok(shutdown_fn) = lib.get::<unsafe extern "C" fn()>(b"SteamAPI_Shutdown") {
                    shutdown_fn();
                }
            }
        }
    }
}
