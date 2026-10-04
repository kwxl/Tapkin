use super::{ActivitySink, FailureSink, InputBackend, InputEvent, InputListener};
use std::{cell::RefCell, ptr, sync::mpsc, thread, time::Duration};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, MSG, PM_NOREMOVE, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
    },
};

thread_local! { static ACTIVITY: RefCell<Option<ActivitySink>> = RefCell::new(None); }

unsafe extern "system" fn callback(code: i32, message: WPARAM, payload: LPARAM) -> LRESULT {
    if code >= 0 && matches!(message as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        // Deliberately never dereference KBDLLHOOKSTRUCT / payload.
        ACTIVITY.with(|slot| {
            if let Some(sink) = slot.borrow().as_ref() {
                sink(InputEvent::AnyKeyPressed);
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
