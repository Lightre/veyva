//! Event-driven native tray host. Capture remains owned by the desktop session.
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
};
use windows::{
    core::w,
    Win32::{
        Foundation::*,
        System::{LibraryLoader::GetModuleHandleW, Registry::*, Threading::CreateMutexW},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub struct BackgroundPreferences {
    pub close_to_tray: bool,
    pub start_with_windows: bool,
}
impl BackgroundPreferences {
    fn path() -> Result<PathBuf, String> {
        Ok(crate::settings::Settings::default_path()?.with_file_name("background.conf"))
    }
    pub fn load() -> Result<Self, String> {
        let text = match std::fs::read_to_string(Self::path()?) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let mut value = Self::default();
        for line in text.lines() {
            match line.split_once('=') {
                Some(("close_to_tray", v)) => {
                    value.close_to_tray = v.parse().map_err(|_| "Invalid close_to_tray")?
                }
                Some(("start_with_windows", v)) => {
                    value.start_with_windows =
                        v.parse().map_err(|_| "Invalid start_with_windows")?
                }
                _ => return Err("Invalid background preference".into()),
            }
        }
        Ok(value)
    }
    pub fn save(&self) -> Result<(), String> {
        let previous = Self::load()?;
        let path = Self::path()?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temporary = path.with_extension("conf.tmp");
        std::fs::write(
            &temporary,
            format!(
                "close_to_tray={}\nstart_with_windows={}\n",
                self.close_to_tray, self.start_with_windows
            ),
        )
        .map_err(|e| e.to_string())?;
        if let Err(error) = configure_startup(self.start_with_windows) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            let rollback = configure_startup(previous.start_with_windows);
            let _ = std::fs::remove_file(&temporary);
            return Err(format!(
                "Could not save background preferences: {error}; startup rollback: {rollback:?}"
            ));
        }
        Ok(())
    }
}
fn configure_startup(enabled: bool) -> Result<(), String> {
    unsafe {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()
        .map_err(|e| e.to_string())?;
        let result = if enabled {
            use std::os::windows::ffi::OsStrExt;
            let mut quoted = std::ffi::OsString::from("\"");
            quoted.push(exe.as_os_str());
            quoted.push("\" gui --background");
            let mut command: Vec<u16> = quoted.encode_wide().collect();
            command.push(0);
            let bytes =
                std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), command.len() * 2);
            RegSetValueExW(key, w!("Veyva"), 0, REG_SZ, Some(bytes)).ok()
        } else {
            let status = RegDeleteValueW(key, w!("Veyva"));
            if status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                status.ok()
            }
        };
        let _ = RegCloseKey(key);
        result.map_err(|e| e.to_string())
    }
}

pub struct SingleInstance(HANDLE);
impl SingleInstance {
    pub fn acquire() -> Result<Option<Self>, String> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\VeyvaDesktopInstance"))
                .map_err(|e| e.to_string())?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                for _ in 0..40 {
                    if let Ok(window) = FindWindowW(w!("VeyvaTrayHost"), None) {
                        let _ = PostMessageW(window, WM_APP + 2, WPARAM(0), LPARAM(0));
                        return Ok(None);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                return Err("Veyva is already starting. Try opening it again shortly.".into());
            }
            Ok(Some(Self(handle)))
        }
    }
}
impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    Show,
    SaveReplay,
    Quit,
}
struct State {
    events: Arc<AtomicU32>,
    available: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
    icon: HICON,
    restart: u32,
}
impl State {
    fn event(&self, bit: u32) {
        self.events.fetch_or(bit, Ordering::Release);
        (self.wake)();
        wake_desktop();
    }
}
pub struct Tray {
    hwnd: usize,
    events: Arc<AtomicU32>,
    available: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Tray {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        let events = Arc::new(AtomicU32::new(0));
        let available = Arc::new(AtomicBool::new(false));
        let state_events = events.clone();
        let state_available = available.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("veyva-tray".into())
            .spawn(move || unsafe {
                let result = (|| -> Result<(), String> {
                    let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                    let class = WNDCLASSW {
                        lpfnWndProc: Some(window_proc),
                        hInstance: instance.into(),
                        lpszClassName: w!("VeyvaTrayHost"),
                        ..Default::default()
                    };
                    if RegisterClassW(&class) == 0 {
                        return Err("Could not register tray window".into());
                    }
                    let icon_data = include_bytes!("../assets/veyva.ico");
                    let size = u32::from_le_bytes(icon_data[14..18].try_into().unwrap()) as usize;
                    let offset = u32::from_le_bytes(icon_data[18..22].try_into().unwrap()) as usize;
                    let icon = CreateIconFromResourceEx(
                        &icon_data[offset..offset + size],
                        true,
                        0x30000,
                        32,
                        32,
                        LR_DEFAULTCOLOR,
                    )
                    .map_err(|e| e.to_string())?;
                    let mut state = Box::new(State {
                        events: state_events,
                        available: state_available,
                        wake,
                        icon,
                        restart: RegisterWindowMessageW(w!("TaskbarCreated")),
                    });
                    let window = match CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        w!("VeyvaTrayHost"),
                        w!("Veyva"),
                        WINDOW_STYLE::default(),
                        0,
                        0,
                        0,
                        0,
                        None,
                        None,
                        instance,
                        None,
                    ) {
                        Ok(w) => w,
                        Err(e) => {
                            let _ = DestroyIcon(icon);
                            return Err(e.to_string());
                        }
                    };
                    SetWindowLongPtrW(window, GWLP_USERDATA, (&mut *state as *mut State) as isize);
                    if !add_icon(window, icon) {
                        let _ = DestroyWindow(window);
                        let _ = DestroyIcon(icon);
                        return Err("Could not create notification-area icon".into());
                    }
                    state.available.store(true, Ordering::Release);
                    let _ = tx.send(Ok(window.0 as usize));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    state.available.store(false, Ordering::Release);
                    let data = NOTIFYICONDATAW {
                        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                        hWnd: window,
                        uID: 1,
                        ..Default::default()
                    };
                    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                    let _ = DestroyWindow(window);
                    let _ = DestroyIcon(icon);
                    Ok(())
                })();
                if let Err(e) = result {
                    let _ = tx.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        match rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                events,
                available,
                worker: Some(worker),
            }),
            other => {
                let _ = worker.join();
                Err(match other {
                    Ok(Err(e)) => e,
                    _ => "Tray initialization failed".into(),
                })
            }
        }
    }
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
    pub fn take_events(&self) -> Vec<TrayEvent> {
        let bits = self.events.swap(0, Ordering::AcqRel);
        [
            (1, TrayEvent::Show),
            (2, TrayEvent::SaveReplay),
            (4, TrayEvent::Quit),
        ]
        .into_iter()
        .filter_map(|(b, e)| (bits & b != 0).then_some(e))
        .collect()
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(HWND(self.hwnd as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
unsafe fn add_icon(window: HWND, icon: HICON) -> bool {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_APP + 1,
        hIcon: icon,
        ..Default::default()
    };
    for (dest, src) in data.szTip.iter_mut().zip("Veyva".encode_utf16()) {
        *dest = src;
    }
    Shell_NotifyIconW(NIM_ADD, &data).as_bool()
}
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if !pointer.is_null() {
        let state = &*pointer;
        if message == state.restart {
            state
                .available
                .store(add_icon(window, state.icon), Ordering::Release);
            state.event(1);
            return LRESULT(0);
        }
        if message == WM_APP + 2 {
            state.event(1);
            return LRESULT(0);
        }
        if message == WM_APP + 1 {
            if matches!(lparam.0 as u32, WM_LBUTTONUP | WM_LBUTTONDBLCLK) {
                state.event(1);
            }
            if lparam.0 as u32 == WM_RBUTTONUP {
                if let Ok(menu) = CreatePopupMenu() {
                    let _ = AppendMenuW(menu, MF_STRING, 1, w!("Show Veyva"));
                    let _ = AppendMenuW(menu, MF_STRING, 2, w!("Save Replay"));
                    let _ = AppendMenuW(menu, MF_STRING, 3, w!("Quit"));
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    let _ = SetForegroundWindow(window);
                    let command = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_RIGHTBUTTON,
                        point.x,
                        point.y,
                        0,
                        window,
                        None,
                    )
                    .0;
                    match command {
                        1 => state.event(1),
                        2 => state.event(2),
                        3 => state.event(4),
                        _ => {}
                    }
                    let _ = DestroyMenu(menu);
                    let _ = PostMessageW(window, WM_NULL, WPARAM(0), LPARAM(0));
                }
            }
            return LRESULT(0);
        }
    }
    if message == WM_CLOSE {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    DefWindowProcW(window, message, wparam, lparam)
}

/// Windows may suppress invalidation-based redraw requests for a hidden HWND.
/// Post one paint message to the owning UI thread so it can drain queued commands
/// without showing the window or rendering continuously in the background.
pub fn wake_desktop() {
    unsafe extern "system" fn visit(window: HWND, _: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid != std::process::id() {
                return BOOL(1);
            }
            // GetWindowTextW sends WM_GETTEXT for windows in our own process.
            // Calling it from this worker can deadlock against the UI joining
            // the tray thread at shutdown. Class metadata needs no UI callback.
            let mut class = [0u16; 64];
            let len = GetClassNameW(window, &mut class);
            // The pinned winit 0.30 backend uses this default desktop class.
            if len <= 0 || String::from_utf16_lossy(&class[..len as usize]) != "Window Class" {
                return BOOL(1);
            }
            let _ = PostMessageW(window, WM_PAINT, WPARAM(0), LPARAM(0));
            BOOL(0)
        }
    }
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(0));
    }
}

/// Fit the restored window to its nearest monitor's usable desktop, including
/// taskbar insets. Called once on the UI thread, never during user resizing.
pub fn fit_desktop_to_work_area() {
    unsafe extern "system" fn visit(window: HWND, _: LPARAM) -> BOOL {
        use windows::Win32::Graphics::Gdi::*;
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid != std::process::id() {
                return BOOL(1);
            }
            let mut class = [0u16; 64];
            let len = GetClassNameW(window, &mut class);
            if len <= 0 || String::from_utf16_lossy(&class[..len as usize]) != "Window Class" {
                return BOOL(1);
            }
            if IsZoomed(window).as_bool() {
                return BOOL(0);
            }
            let mut rect = RECT::default();
            let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetWindowRect(window, &mut rect).is_ok()
                && GetMonitorInfoW(monitor, &mut info).as_bool()
            {
                let work = info.rcWork;
                let width = (rect.right - rect.left).min(work.right - work.left).max(1);
                let height = (rect.bottom - rect.top).min(work.bottom - work.top).max(1);
                let x = rect.left.clamp(work.left, work.right - width);
                let y = rect.top.clamp(work.top, work.bottom - height);
                let _ = SetWindowPos(
                    window,
                    None,
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            BOOL(0)
        }
    }
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(0));
    }
}
