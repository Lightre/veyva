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
        if modifiers == 0 || modifiers > 15 || !(0x70..=0x7B).contains(&key) {
            return Err("Choose a modifier and F1-F12".into());
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
    parts.push(format!("F{}", key.saturating_sub(0x70) + 1));
    parts.join("+")
}
