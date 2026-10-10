use super::{
    character_from_utf16, ActivitySink, FailureSink, InputBackend, InputEvent, InputListener,
    KeyCode,
};
use std::{cell::RefCell, ptr, sync::mpsc, thread, time::Duration};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx},
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW,
            GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
            TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
            PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

thread_local! { static ACTIVITY: RefCell<Option<ActivitySink>> = RefCell::new(None); }
thread_local! { static CAPS_LOCK: RefCell<bool> = const { RefCell::new(false) }; }

unsafe fn event_character(event: &KBDLLHOOKSTRUCT) -> Option<char> {
    if event.vkCode == 0x14 {
        if unsafe { GetAsyncKeyState(0x14) } >= 0 {
            CAPS_LOCK.with(|caps| *caps.borrow_mut() ^= true);
        }
        return None;
    }
    let mut state = [0_u8; 256];
    // The low-level callback runs before this event updates asynchronous state.
    // Other held modifiers are already visible; mark the current key explicitly.
    for vk in [
        0x10, 0x11, 0x12, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c,
    ] {
        if unsafe { GetAsyncKeyState(vk) } < 0 {
            state[vk as usize] = 0x80;
        }
    }
    if let Some(slot) = state.get_mut(event.vkCode as usize) {
        *slot |= 0x80;
    }
    state[0x14] = CAPS_LOCK.with(|caps| u8::from(*caps.borrow()));
    // Ignore control-only and Windows shortcuts; Ctrl+Alt permits AltGr layouts.
    if state[0x5b] != 0 || state[0x5c] != 0 || ((state[0x11] != 0) != (state[0x12] != 0)) {
        return None;
    }
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return None;
    }
    let thread = unsafe { GetWindowThreadProcessId(foreground, ptr::null_mut()) };
    let layout = unsafe { GetKeyboardLayout(thread) };
    let mut text = [0_u16; 8];
    // Bit 2 leaves the OS dead-key state untouched (Windows 10 1607+, including 11).
    let count = unsafe {
        ToUnicodeEx(
            event.vkCode,
            event.scanCode,
            state.as_ptr(),
            text.as_mut_ptr(),
            text.len() as i32,
            4,
            layout,
        )
    };
    if count <= 0 {
        return None;
    }
    text.get(..count as usize).and_then(character_from_utf16)
}

unsafe extern "system" fn callback(code: i32, message: WPARAM, payload: LPARAM) -> LRESULT {
    if code >= 0
        && matches!(
            message as u32,
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
        )
    {
        if matches!(message as u32, WM_KEYUP | WM_SYSKEYUP) {
            if payload != 0 {
                let event = unsafe { &*(payload as *const KBDLLHOOKSTRUCT) };
                let id = event.scanCode
                    | if event.flags & LLKHF_EXTENDED != 0 {
                        0xe000
                    } else {
                        0
                    };
                ACTIVITY.with(|slot| {
                    if let Some(sink) = slot.borrow().as_ref() {
                        sink(InputEvent::KeyUp { id });
                    }
                });
            }
            return unsafe { CallNextHookEx(ptr::null_mut(), code, message, payload) };
        }
        // Translation is transient; never retain text or alter OS keyboard state.
        let (key, character) = if payload == 0 {
            (None, None)
        } else {
            let event = unsafe { &*(payload as *const KBDLLHOOKSTRUCT) };
            (
                KeyCode::from_windows(event.scanCode, event.flags & LLKHF_EXTENDED != 0),
                unsafe { event_character(event) },
            )
        };
        ACTIVITY.with(|slot| {
            if let Some(sink) = slot.borrow().as_ref() {
                if payload != 0 {
                    let event = unsafe { &*(payload as *const KBDLLHOOKSTRUCT) };
                    let id = event.scanCode
                        | if event.flags & LLKHF_EXTENDED != 0 {
                            0xe000
                        } else {
                            0
                        };
                    // The core's down-key set detects repeats; no text is retained here.
                    sink(InputEvent::KeyDown {
                        id,
                        key,
                        character,
                        repeat: false,
                    });
                }
            }
        });
    }
    // A listen-only hook must always pass input to the next hook.
    unsafe { CallNextHookEx(ptr::null_mut(), code, message, payload) }
}

pub struct WindowsInput;
struct Listener {
    thread_id: u32,
    worker: Option<thread::JoinHandle<()>>,
}
impl InputListener for Listener {}
impl Drop for Listener {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl InputBackend for WindowsInput {
    fn start(
        &self,
        activity: ActivitySink,
        failure: FailureSink,
    ) -> Result<Box<dyn InputListener>, String> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (run_tx, run_rx) = mpsc::sync_channel(0);
        let worker = thread::Builder::new().name("tapkin-key-hook".into()).spawn(move || unsafe {
            let mut message: MSG = std::mem::zeroed();
            // Create the thread message queue before exposing its ID to Drop.
            PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_NOREMOVE);
            ACTIVITY.with(|slot| *slot.borrow_mut() = Some(activity));
            CAPS_LOCK.with(|caps| *caps.borrow_mut() = GetKeyState(0x14) & 1 != 0);
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(callback), GetModuleHandleW(ptr::null()), 0);
            if hook.is_null() {
                let _ = ready_tx.send(Err(format!("Windows could not start the global keyboard hook: {}. Try Retry listener.", std::io::Error::last_os_error())));
                ACTIVITY.with(|slot| *slot.borrow_mut() = None);
                return;
            }
            if ready_tx.send(Ok(GetCurrentThreadId())).is_ok() && run_rx.recv().is_ok() {
                loop {
                    match GetMessageW(&mut message, ptr::null_mut(), 0, 0) {
                        -1 => { failure("The Windows keyboard listener stopped. Use Retry listener in Settings.".into()); break; },
                        0 => break,
                        _ => { TranslateMessage(&message); DispatchMessageW(&message); },
                    }
                }
            }
            UnhookWindowsHookEx(hook);
            ACTIVITY.with(|slot| *slot.borrow_mut() = None);
        }).map_err(|e| format!("Could not create keyboard listener thread: {e}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(thread_id)) => {
                run_tx
                    .send(())
                    .map_err(|_| "Windows listener stopped during startup".to_string())?;
                Ok(Box::new(Listener {
                    thread_id,
                    worker: Some(worker),
                }))
            }
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => Err("Windows keyboard hook did not become ready. Try Retry listener.".into()),
        }
    }
}
