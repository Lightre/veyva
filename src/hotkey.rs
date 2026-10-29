//! A thread-owned Windows hotkey with bounded delivery and no polling by an idle UI.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
        },
        WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY},
    },
};

pub struct Hotkey {
    stop: Arc<AtomicBool>,
    pressed: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Hotkey {
    pub fn register(
        modifiers: u32,
        key: u32,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        if !crate::settings::valid_shortcut(modifiers, key) {
            return Err("Choose a modifier and a supported key".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let pressed = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let delivered = pressed.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("veyva-shortcut".into())
            .spawn(move || unsafe {
                if let Err(e) = RegisterHotKey(
                    HWND::default(),
                    1,
                    HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT.0),
                    key,
                ) {
                    let _ = tx.send(Err(format!(
                        "Could not register {}: {e}",
                        label(modifiers, key)
                    )));
                    return;
                }
                let _ = tx.send(Ok(()));
                while !stopping.load(Ordering::Acquire) {
                    let mut message = MSG::default();
                    while PeekMessageW(
                        &mut message,
                        HWND::default(),
                        WM_HOTKEY,
                        WM_HOTKEY,
                        PM_REMOVE,
                    )
                    .as_bool()
                    {
                        delivered.store(true, Ordering::Release);
                        wake();
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
                let _ = UnregisterHotKey(HWND::default(), 1);
            })
            .map_err(|e| e.to_string())?;
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                pressed,
                worker: Some(worker),
            }),
            result => {
                stop.store(true, Ordering::Release);
                if worker.is_finished() {
                    let _ = worker.join();
                }
                Err(match result {
                    Ok(Err(e)) => e,
                    _ => "Shortcut registration timed out".into(),
                })
            }
        }
    }
    pub fn take_pressed(&self) -> bool {
        self.pressed.swap(false, Ordering::AcqRel)
    }
}
impl Drop for Hotkey {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
pub fn label(modifiers: u32, key: u32) -> String {
    let mut parts = Vec::new();
    for (flag, name) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
        if modifiers & flag != 0 {
            parts.push(name.to_string());
        }
    }
    parts.push(match key {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(key).unwrap().to_string(),
        0x70..=0x87 => format!("F{}", key - 0x70 + 1),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x20 => "Space".into(),
        0x21 => "Page Up".into(),
        0x22 => "Page Down".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        _ => format!("Key 0x{key:02X}"),
    });
    parts.join("+")
}
