use std::{
    sync::{mpsc, Mutex},
    thread,
};

use anyhow::Context;
use smart_switcher_shared_types::KeyboardEvent;
use windows_sys::Win32::{
    Foundation::{GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION,
        KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

static KEY_TX: Mutex<Option<mpsc::Sender<KeyboardEvent>>> = Mutex::new(None);

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let msg = wparam as u32;
        let is_key_down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
        let is_key_up = matches!(msg, WM_KEYUP | WM_SYSKEYUP);

        if is_key_down || is_key_up {
            let kb = unsafe { *(lparam as *const KBDLLHOOKSTRUCT) };
            if let Ok(guard) = KEY_TX.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.send(KeyboardEvent {
                        vk_code: kb.vkCode,
                        scan_code: kb.scanCode,
                        flags: kb.flags,
                        is_key_down,
                    });
                }
            }
        }
    }

    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

pub struct KeyboardHookController {
    thread_id: u32,
    join: Option<thread::JoinHandle<()>>,
}

impl KeyboardHookController {
    pub fn stop(mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for KeyboardHookController {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub struct KeyboardHook {
    controller: KeyboardHookController,
    events: mpsc::Receiver<KeyboardEvent>,
}

impl KeyboardHook {
    pub fn into_parts(self) -> (KeyboardHookController, mpsc::Receiver<KeyboardEvent>) {
        (self.controller, self.events)
    }
}

pub fn start_keyboard_hook() -> anyhow::Result<KeyboardHook> {
    let (events_tx, events_rx) = mpsc::channel::<KeyboardEvent>();
    let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<u32>>();

    let join = thread::spawn(move || {
        {
            let mut guard = KEY_TX.lock().expect("keyboard hook sender lock");
            *guard = Some(events_tx);
        }

        let thread_id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };

        let hmod: HINSTANCE = unsafe { GetModuleHandleW(std::ptr::null()) };
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) };
        if hook.is_null() {
            let err = unsafe { GetLastError() };
            let mut guard = KEY_TX.lock().expect("keyboard hook sender lock");
            *guard = None;
            let _ = ready_tx.send(Err(anyhow::anyhow!(
                "SetWindowsHookExW(WH_KEYBOARD_LL) failed, GetLastError={err}"
            )));
            return;
        }

        let _ = ready_tx.send(Ok(thread_id));

        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if ret <= 0 {
                break;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        unsafe {
            UnhookWindowsHookEx(hook);
        }

        let mut guard = KEY_TX.lock().expect("keyboard hook sender lock");
        *guard = None;
    });

    let thread_id = ready_rx
        .recv()
        .context("keyboard hook thread did not report status")??;

    Ok(KeyboardHook {
        controller: KeyboardHookController {
            thread_id,
            join: Some(join),
        },
        events: events_rx,
    })
}
