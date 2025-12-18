use std::{
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use smart_switcher_shared_types::config::ForbiddenContextsConfig;
use smart_switcher_shared_types::KeyboardEvent;
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::{
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, GetKeyboardLayoutList, MapVirtualKeyExW, SendInput, ToUnicodeEx, INPUT,
        INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, VK_BACK,
    },
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        PostMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
        WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    },
};

fn is_cyrillic_char(ch: char) -> bool {
    matches!(ch, '\u{0400}'..='\u{04FF}')
}

pub fn is_active_layout_cyrillic() -> anyhow::Result<bool> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Ok(false);
    }

    let mut pid: u32 = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread_id == 0 {
        return Ok(false);
    }

    let hkl = unsafe { GetKeyboardLayout(thread_id) };

    // Пробуем понять "какая это раскладка" по выводу ToUnicodeEx для VK_G.
    // 0x47 = 'G'
    let vk_g: u32 = 0x47;
    let scan = unsafe { MapVirtualKeyExW(vk_g, 0, hkl) };

    // Важно: не используем GetKeyboardState (он привязан к потоку) — это может врать.
    // Нам нужна базовая буква без модификаторов.
    let state = [0u8; 256];

    let mut out = [0u16; 8];
    let rc = unsafe {
        ToUnicodeEx(
            vk_g,
            scan,
            state.as_ptr(),
            out.as_mut_ptr(),
            out.len() as i32,
            0,
            hkl,
        )
    };

    if rc <= 0 {
        return Ok(false);
    }

    let ch = char::from_u32(out[0] as u32).unwrap_or('\0');
    Ok(is_cyrillic_char(ch))
}

static KEY_TX: Mutex<Option<mpsc::Sender<KeyboardEvent>>> = Mutex::new(None);

const ACTIVE_WINDOW_CACHE_TTL: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct ActiveWindowCache {
    hwnd_key: usize,
    info: ActiveWindowInfo,
    updated_at: Instant,
}

static ACTIVE_WINDOW_CACHE: Mutex<Option<ActiveWindowCache>> = Mutex::new(None);

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

#[derive(Debug, Clone)]
pub struct ActiveWindowInfo {
    pub title: String,
    pub process_name: Option<String>,
}

fn contains_any(haystack: &str, needles: &[String]) -> bool {
    let haystack = haystack.to_lowercase();
    needles
        .iter()
        .map(|s| s.to_lowercase())
        .any(|needle| !needle.is_empty() && haystack.contains(&needle))
}

fn is_forbidden(info: &ActiveWindowInfo, forbidden: &ForbiddenContextsConfig) -> bool {
    if contains_any(&info.title, &forbidden.blocked_windows) {
        return true;
    }

    if let Some(proc_name) = info.process_name.as_ref() {
        if contains_any(proc_name, &forbidden.blocked_processes) {
            return true;
        }
    }

    false
}

fn get_window_title(hwnd: *mut core::ffi::c_void) -> anyhow::Result<String> {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return Ok(String::new());
    }

    let mut buf = vec![0u16; (len as usize) + 1];
    let written = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if written <= 0 {
        return Ok(String::new());
    }

    Ok(String::from_utf16_lossy(&buf[..written as usize]))
}

fn get_process_name(hwnd: *mut core::ffi::c_void) -> Option<String> {
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut buf = vec![0u16; 1024];
    let mut size: u32 = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size)
    };
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 || size == 0 {
        return None;
    }

    let full = String::from_utf16_lossy(&buf[..size as usize]);
    let name = std::path::Path::new(&full)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());

    name
}

pub fn get_active_window_info() -> anyhow::Result<ActiveWindowInfo> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Err(anyhow::anyhow!("GetForegroundWindow returned null"));
    }

    get_window_info_cached(hwnd)
}

fn get_window_info_cached(hwnd: *mut core::ffi::c_void) -> anyhow::Result<ActiveWindowInfo> {
    let hwnd_key = hwnd as usize;
    if let Ok(guard) = ACTIVE_WINDOW_CACHE.lock() {
        if let Some(entry) = guard.as_ref() {
            if entry.hwnd_key == hwnd_key
                && entry.updated_at.elapsed() <= ACTIVE_WINDOW_CACHE_TTL
            {
                return Ok(entry.info.clone());
            }
        }
    }

    let info = ActiveWindowInfo {
        title: get_window_title(hwnd)?,
        process_name: get_process_name(hwnd),
    };

    if let Ok(mut guard) = ACTIVE_WINDOW_CACHE.lock() {
        *guard = Some(ActiveWindowCache {
            hwnd_key,
            info: info.clone(),
            updated_at: Instant::now(),
        });
    }

    Ok(info)
}

pub fn switch_to_next_layout(forbidden: &ForbiddenContextsConfig) -> anyhow::Result<bool> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Ok(false);
    }

    let info = get_window_info_cached(hwnd)?;
    if is_forbidden(&info, forbidden) {
        return Ok(false);
    }

    let mut pid: u32 = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread_id == 0 {
        return Ok(false);
    }

    let current = unsafe { GetKeyboardLayout(thread_id) };
    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    if count <= 0 {
        return Ok(false);
    }

    let mut layouts: Vec<*mut core::ffi::c_void> = vec![std::ptr::null_mut(); count as usize];
    let filled = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
    if filled <= 0 {
        return Ok(false);
    }
    layouts.truncate(filled as usize);

    let next = match layouts.iter().position(|&hkl| hkl == current) {
        Some(idx) => layouts[(idx + 1) % layouts.len()],
        None => layouts[0],
    };

    let ok = unsafe { PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, 0, next as isize) };

    Ok(ok != 0)
}

pub fn is_forbidden_context(forbidden: &ForbiddenContextsConfig) -> anyhow::Result<bool> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Ok(true);
    }

    let info = get_window_info_cached(hwnd)?;
    Ok(is_forbidden(&info, forbidden))
}

fn lo_word(value: isize) -> u16 {
    (value as usize & 0xFFFF) as u16
}

pub fn get_active_lang_id() -> anyhow::Result<u16> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Err(anyhow::anyhow!("GetForegroundWindow returned null"));
    }

    let mut pid: u32 = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread_id == 0 {
        return Err(anyhow::anyhow!("GetWindowThreadProcessId returned 0"));
    }

    let hkl = unsafe { GetKeyboardLayout(thread_id) };
    Ok(lo_word(hkl as isize))
}

pub fn set_layout_by_lang_id(
    forbidden: &ForbiddenContextsConfig,
    lang_id: u16,
) -> anyhow::Result<bool> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Ok(false);
    }

    let info = get_window_info_cached(hwnd)?;
    if is_forbidden(&info, forbidden) {
        return Ok(false);
    }

    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    if count <= 0 {
        return Ok(false);
    }

    let mut layouts: Vec<*mut core::ffi::c_void> = vec![std::ptr::null_mut(); count as usize];
    let filled = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
    if filled <= 0 {
        return Ok(false);
    }
    layouts.truncate(filled as usize);

    let target = layouts
        .into_iter()
        .find(|&hkl| lo_word(hkl as isize) == lang_id);

    let Some(target) = target else {
        return Ok(false);
    };

    let ok = unsafe { PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, 0, target as isize) };
    Ok(ok != 0)
}

pub fn send_backspaces(forbidden: &ForbiddenContextsConfig, count: usize) -> anyhow::Result<bool> {
    let info = get_active_window_info()?;
    if is_forbidden(&info, forbidden) {
        return Ok(false);
    }

    if count == 0 {
        return Ok(true);
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(count * 2);
    for _ in 0..count {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_BACK as u16,
                    wScan: 0,
                    dwFlags: 0,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_BACK as u16,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        inputs.push(down);
        inputs.push(up);
    }

    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    Ok(sent == inputs.len() as u32)
}

pub fn send_unicode_text(
    forbidden: &ForbiddenContextsConfig,
    text: &str,
) -> anyhow::Result<bool> {
    let info = get_active_window_info()?;
    if is_forbidden(&info, forbidden) {
        return Ok(false);
    }

    if text.is_empty() {
        return Ok(true);
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(text.encode_utf16().count() * 2);
    for ch in text.encode_utf16() {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: ch,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: ch,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        inputs.push(down);
        inputs.push(up);
    }

    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    Ok(sent == inputs.len() as u32)
}
