use super::{ActivitySink, FailureSink, InputBackend, InputEvent, InputListener};
use std::{
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

type CfRef = *const c_void;
type TapRef = *mut c_void;
type EventRef = *mut c_void;
type Callback = unsafe extern "C" fn(*mut c_void, u32, EventRef, *mut c_void) -> EventRef;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: Callback,
        info: *mut c_void,
    ) -> TapRef;
    fn CGEventTapEnable(tap: TapRef, enable: bool);
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CfRef;
    fn CFMachPortCreateRunLoopSource(allocator: CfRef, port: TapRef, order: isize) -> CfRef;
    fn CFRunLoopGetCurrent() -> CfRef;
    fn CFRunLoopAddSource(run_loop: CfRef, source: CfRef, mode: CfRef);
    fn CFRunLoopRemoveSource(run_loop: CfRef, source: CfRef, mode: CfRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: CfRef);
    fn CFMachPortInvalidate(port: TapRef);
    fn CFRetain(value: CfRef) -> CfRef;
    fn CFRelease(value: CfRef);
}

const KEY_DOWN: u32 = 10;
const TAP_DISABLED_TIMEOUT: u32 = 0xffff_fffe;
const TAP_DISABLED_USER: u32 = 0xffff_ffff;
const PERMISSION_MESSAGE: &str = "Tapkin needs Input Monitoring permission to detect typing in other apps. It does not record or save what you type. Open System Settings → Privacy & Security → Input Monitoring, enable Tapkin, then choose Retry listener. If macOS requests an app relaunch, quit and reopen Tapkin. Accessibility may also be required if the event tap cannot be created.";

struct Context {
    activity: ActivitySink,
    failure: FailureSink,
    tap: TapRef,
    run_loop: CfRef,
    stopping: Arc<AtomicBool>,
}

unsafe extern "C" fn callback(
    _proxy: *mut c_void,
    kind: u32,
    event: EventRef,
    info: *mut c_void,
) -> EventRef {
    let context = unsafe { &*(info as *const Context) };
    match kind {
        KEY_DOWN => (context.activity)(InputEvent::AnyKeyPressed),
        TAP_DISABLED_TIMEOUT => unsafe {
            CGEventTapEnable(context.tap, true);
        },
        TAP_DISABLED_USER => {
            context.stopping.store(true, Ordering::Release);
            (context.failure)(PERMISSION_MESSAGE.into());
            unsafe {
                CFRunLoopStop(context.run_loop);
            }
        }
        _ => {}
    }
    // Never inspect event contents; listen-only taps cannot modify input.
    event
}

pub struct MacInput;
struct Listener {
    run_loop: usize,
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl InputListener for Listener {}
impl Drop for Listener {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        // The thread retains its run loop until it exits and Drop has joined it.
        unsafe {
            CFRunLoopStop(self.run_loop as CfRef);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        unsafe {
            CFRelease(self.run_loop as CfRef);
        }
    }
}

impl InputBackend for MacInput {
    fn start(
        &self,
        activity: ActivitySink,
        failure: FailureSink,
    ) -> Result<Box<dyn InputListener>, String> {
        if !unsafe { CGPreflightListenEventAccess() } {
            unsafe {
                CGRequestListenEventAccess();
            }
            if !unsafe { CGPreflightListenEventAccess() } {
                return Err(PERMISSION_MESSAGE.into());
            }
        }
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (run_tx, run_rx) = mpsc::sync_channel(0);
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopping);
        let worker = thread::Builder::new().name("tapkin-key-tap".into()).spawn(move || unsafe {
            let run_loop = CFRunLoopGetCurrent();
            let mut context = Box::new(Context { activity, failure, tap: ptr::null_mut(), run_loop, stopping: stop });
            // Session event tap, head insertion, listen-only, key-down events only.
            let tap = CGEventTapCreate(1, 0, 1, 1_u64 << KEY_DOWN, callback, (&mut *context as *mut Context).cast());
            if tap.is_null() {
                let _ = ready_tx.send(Err(PERMISSION_MESSAGE.to_string()));
                return;
            }
            context.tap = tap;
            let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
            if source.is_null() {
                CFMachPortInvalidate(tap);
                CFRelease(tap);
                let _ = ready_tx.send(Err("macOS could not create the keyboard listener run-loop source. Try Retry listener.".into()));
                return;
            }
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            CFRetain(run_loop);
            if ready_tx.send(Ok(run_loop as usize)).is_ok() && run_rx.recv().is_ok() {
                if !context.stopping.load(Ordering::Acquire) {
                    CFRunLoopRun();
                    if !context.stopping.load(Ordering::Acquire) {
                        (context.failure)("macOS keyboard listener stopped. Use Retry listener in Settings.".into());
                    }
                }
            } else {
                CFRelease(run_loop);
            }
            CFRunLoopRemoveSource(run_loop, source, kCFRunLoopCommonModes);
            CFMachPortInvalidate(tap);
            CFRelease(source);
            CFRelease(tap);
        }).map_err(|e| format!("Could not create keyboard listener thread: {e}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(run_loop)) => {
                run_tx
                    .send(())
                    .map_err(|_| "macOS listener stopped during startup".to_string())?;
                Ok(Box::new(Listener {
                    run_loop,
                    stopping,
                    worker: Some(worker),
                }))
            }
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                stopping.store(true, Ordering::Release);
                Err("macOS keyboard listener did not become ready. Try Retry listener.".into())
            }
        }
    }
}
