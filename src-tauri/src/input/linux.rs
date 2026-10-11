//! Listen-only Linux keyboard observer using evdev (`/dev/input/event*`).
//!
//! Devices are opened read-only and never grabbed (`EVIOCGRAB` is not used), so the
//! compositor/X server and every application keep receiving input unchanged. This
//! works in X11 and Wayland sessions alike. Characters are translated transiently
//! with the user's XKB keymap; no text or key history is retained.
use super::{
    single_character, ActivitySink, FailureSink, InputBackend, InputEvent, InputListener, KeyCode,
};
use std::{
    collections::HashMap,
    ffi::{CString, OsStr},
    fs::File,
    io::Read,
    mem,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Path, PathBuf},
    ptr,
    sync::mpsc,
    thread,
    time::Duration,
};
use xkbcommon_dl::{
    x11::xkbcommon_x11_option, xkb_context, xkb_context_flags, xkb_key_direction, xkb_keymap,
    xkb_keymap_compile_flags, xkb_state, xkb_state_component, xkbcommon_option, XkbCommon,
    XKB_MOD_INVALID, XKB_MOD_NAME_ALT, XKB_MOD_NAME_CAPS, XKB_MOD_NAME_CTRL, XKB_MOD_NAME_LOGO,
    XKB_MOD_NAME_NUM,
};

const INPUT_DIR: &str = "/dev/input";
const PERMISSION_MESSAGE: &str = "Tapkin needs read access to keyboard devices to detect typing in other apps. It does not record or save what you type, and it never blocks or changes your input. Add your account to the \"input\" group (for example: sudo usermod -aG input $USER), log out and back in, then choose Retry listener.";

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const KEY_ESC: u16 = 1;
const KEY_A: u16 = 30;
const KEY_Z: u16 = 44;
const KEY_SPACE: u16 = 57;
const KEY_ENTER: u16 = 28;
const BTN_MISC: u16 = 0x100;
const KEY_OK: u16 = 0x160;
const BTN_TRIGGER_HAPPY: u16 = 0x2c0;
const KEY_MAX: usize = 0x2ff;
const LED_NUML: usize = 0;
const LED_CAPSL: usize = 1;
/// XKB keycodes are evdev codes offset by 8.
const XKB_OFFSET: u32 = 8;

/// `_IOC(_IOC_READ, 'E', nr, len)` from `<linux/input.h>`.
const fn eviocg(nr: u64, len: usize) -> u64 {
    (2 << 30) | ((len as u64) << 16) | ((b'E' as u64) << 8) | nr
}

/// Linux key codes equal Windows set-1 scan codes for the non-extended keys that
/// Tapkin supports; only the extended navigation/numpad keys need translating.
fn key_code(code: u16) -> Option<KeyCode> {
    let extended = match code {
        96 => 0x1c,  // KEY_KPENTER
        98 => 0x35,  // KEY_KPSLASH
        102 => 0x47, // KEY_HOME
        103 => 0x48, // KEY_UP
        104 => 0x49, // KEY_PAGEUP
        105 => 0x4b, // KEY_LEFT
        106 => 0x4d, // KEY_RIGHT
        107 => 0x4f, // KEY_END
        108 => 0x50, // KEY_DOWN
        109 => 0x51, // KEY_PAGEDOWN
        111 => 0x53, // KEY_DELETE
        _ if code <= 0x58 => return KeyCode::from_windows(code.into(), false),
        _ => return None,
    };
    KeyCode::from_windows(extended, true)
}

/// Keyboard keys only; mouse, joystick and touch buttons are not typing activity.
fn is_keyboard_key(code: u16) -> bool {
    (KEY_ESC..BTN_MISC).contains(&code) || (KEY_OK..BTN_TRIGGER_HAPPY).contains(&code)
}

fn bit(bits: &[u8], index: usize) -> bool {
    bits.get(index / 8)
        .is_some_and(|byte| byte & (1 << (index % 8)) != 0)
}

/// Transient XKB state shared by all keyboards, as a compositor would keep it.
struct Keymap {
    xkb: &'static XkbCommon,
    context: *mut xkb_context,
    keymap: *mut xkb_keymap,
    state: *mut xkb_state,
}

impl Keymap {
    /// Use the X server's active keymap (XWayland mirrors the compositor's keymap),
    /// falling back to the XKB defaults/environment when X is unavailable.
    fn load() -> Option<Self> {
        let xkb = xkbcommon_option()?;
        unsafe {
            let context = (xkb.xkb_context_new)(xkb_context_flags::XKB_CONTEXT_NO_FLAGS);
            if context.is_null() {
                return None;
            }
            let (mut keymap, mut state) =
                x11_keymap(xkb, context).unwrap_or((ptr::null_mut(), ptr::null_mut()));
            if keymap.is_null() {
                keymap = (xkb.xkb_keymap_new_from_names)(
                    context,
                    ptr::null(),
                    xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
                );
            }
            if !keymap.is_null() && state.is_null() {
                state = (xkb.xkb_state_new)(keymap);
            }
            if keymap.is_null() || state.is_null() {
                if !keymap.is_null() {
                    (xkb.xkb_keymap_unref)(keymap);
                }
                (xkb.xkb_context_unref)(context);
                return None;
            }
            let map = Self {
                xkb,
                context,
                keymap,
                state,
            };
            map.reset(None);
            Some(map)
        }
    }

    fn mod_index(&self, name: &[u8]) -> u32 {
        unsafe { (self.xkb.xkb_keymap_mod_get_index)(self.keymap, name.as_ptr().cast()) }
    }

    fn mod_active(&self, name: &[u8]) -> bool {
        unsafe {
            (self.xkb.xkb_state_mod_name_is_active)(
                self.state,
                name.as_ptr().cast(),
                xkb_state_component::XKB_STATE_MODS_EFFECTIVE,
            ) > 0
        }
    }

    /// Clear pressed modifiers while keeping the lock/layout state. When LED state
    /// is known, it is the kernel's view of Caps Lock/Num Lock and wins.
    fn reset(&self, leds: Option<(bool, bool)>) {
        unsafe {
            let mut locked = (self.xkb.xkb_state_serialize_mods)(
                self.state,
                xkb_state_component::XKB_STATE_MODS_LOCKED,
            );
            let layout = (self.xkb.xkb_state_serialize_layout)(
                self.state,
                xkb_state_component::XKB_STATE_LAYOUT_EFFECTIVE,
            );
            if let Some((caps, num)) = leds {
                for (name, on) in [(XKB_MOD_NAME_CAPS, caps), (XKB_MOD_NAME_NUM, num)] {
                    let index = self.mod_index(name);
                    if index != XKB_MOD_INVALID && index < 32 {
                        if on {
                            locked |= 1 << index;
                        } else {
                            locked &= !(1 << index);
                        }
                    }
                }
            }
            (self.xkb.xkb_state_update_mask)(self.state, 0, 0, locked, 0, 0, layout);
        }
    }

    /// Best-effort single character for this press. Control, Alt and Super
    /// shortcuts are not text; AltGr (ISO Level 3) remains layout-aware.
    fn character(&self, code: u16) -> Option<char> {
        if [XKB_MOD_NAME_CTRL, XKB_MOD_NAME_ALT, XKB_MOD_NAME_LOGO]
            .iter()
            .any(|name| self.mod_active(name))
        {
            return None;
        }
        let mut text = [0_u8; 8];
        let length = unsafe {
            (self.xkb.xkb_state_key_get_utf8)(
                self.state,
                u32::from(code) + XKB_OFFSET,
                text.as_mut_ptr().cast(),
                text.len(),
            )
        };
        let length = usize::try_from(length).ok().filter(|&n| n < text.len())?;
        std::str::from_utf8(&text[..length])
            .ok()
            .and_then(single_character)
    }

    fn update(&self, code: u16, pressed: bool) {
        let direction = if pressed {
            xkb_key_direction::XKB_KEY_DOWN
        } else {
            xkb_key_direction::XKB_KEY_UP
        };
        unsafe {
            (self.xkb.xkb_state_update_key)(self.state, u32::from(code) + XKB_OFFSET, direction);
        }
    }
}

impl Drop for Keymap {
    fn drop(&mut self) {
        unsafe {
            (self.xkb.xkb_state_unref)(self.state);
            (self.xkb.xkb_keymap_unref)(self.keymap);
            (self.xkb.xkb_context_unref)(self.context);
        }
    }
}

/// Fetch the core keyboard keymap and lock/layout state from the X server.
unsafe fn x11_keymap(
    xkb: &XkbCommon,
    context: *mut xkb_context,
) -> Option<(*mut xkb_keymap, *mut xkb_state)> {
    use x11_dl::{xlib::Xlib, xlib_xcb::Xlib_xcb};
    std::env::var_os("DISPLAY")?;
    let x11 = xkbcommon_x11_option()?;
    let xlib = Xlib::open().ok()?;
    let xlib_xcb = Xlib_xcb::open().ok()?;
    let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    if display.is_null() {
        return None;
    }
    let result = unsafe {
        let connection = (xlib_xcb.XGetXCBConnection)(display);
        let ready = !connection.is_null()
            && (x11.xkb_x11_setup_xkb_extension)(
                connection,
                1,
                0,
                xkbcommon_dl::x11::xkb_x11_setup_xkb_extension_flags::XKB_X11_SETUP_XKB_EXTENSION_NO_FLAGS,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            ) == 1;
        let device = if ready {
            (x11.xkb_x11_get_core_keyboard_device_id)(connection)
        } else {
            -1
        };
        if device < 0 {
            None
        } else {
            let keymap = (x11.xkb_x11_keymap_new_from_device)(
                context,
                connection,
                device,
                xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            );
            if keymap.is_null() {
                None
            } else {
                let state = (x11.xkb_x11_state_new_from_device)(keymap, connection, device);
                if state.is_null() {
                    (xkb.xkb_keymap_unref)(keymap);
                    None
                } else {
                    Some((keymap, state))
                }
            }
        }
    };
    // The compiled keymap and state are independent of the connection.
    unsafe { (xlib.XCloseDisplay)(display) };
    result
}

struct Device {
    file: File,
}

impl Device {
    /// Open read-only and nonblocking. The device is never grabbed.
    fn open(path: &Path) -> std::io::Result<Option<Self>> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        let device = Self { file };
        Ok(device.is_keyboard().then_some(device))
    }

    fn query(&self, nr: u64, bits: &mut [u8]) -> bool {
        let request = eviocg(nr, bits.len());
        unsafe { libc::ioctl(self.file.as_raw_fd(), request as _, bits.as_mut_ptr()) >= 0 }
    }

    /// Typing keyboards report letter, space and enter keys. This excludes mice,
    /// power buttons and media-key-only interfaces.
    fn is_keyboard(&self) -> bool {
        let mut types = [0_u8; 4];
        let mut keys = [0_u8; KEY_MAX / 8 + 1];
        self.query(0x20, &mut types)
            && bit(&types, EV_KEY.into())
            && self.query(0x20 + u64::from(EV_KEY), &mut keys)
            && [KEY_A, KEY_Z, KEY_SPACE, KEY_ENTER]
                .iter()
                .all(|&key| bit(&keys, key.into()))
    }

    fn leds(&self) -> Option<(bool, bool)> {
        let mut leds = [0_u8; 8];
        self.query(0x19, &mut leds)
            .then(|| (bit(&leds, LED_CAPSL), bit(&leds, LED_NUML)))
    }
}

/// Converts raw evdev events into Tapkin input events. Kept free of I/O for tests.
#[derive(Default)]
struct Decoder {
    dropping: bool,
}

impl Decoder {
    fn event(
        &mut self,
        kind: u16,
        code: u16,
        value: i32,
        keymap: Option<&Keymap>,
    ) -> Option<InputEvent> {
        match kind {
            EV_SYN if code == SYN_DROPPED => {
                // The kernel buffer overflowed; releases may be lost until resync.
                self.dropping = true;
                Some(InputEvent::ResetKeys)
            }
            EV_SYN if code == SYN_REPORT => {
                self.dropping = false;
                None
            }
            EV_KEY if !self.dropping && is_keyboard_key(code) => {
                let id = u32::from(code);
                match value {
                    0 => {
                        if let Some(keymap) = keymap {
                            keymap.update(code, false);
                        }
                        Some(InputEvent::KeyUp { id })
                    }
                    1 | 2 => {
                        // Translate with the state before this key's own update.
                        let character = keymap.and_then(|keymap| keymap.character(code));
                        if value == 1 {
                            if let Some(keymap) = keymap {
                                keymap.update(code, true);
                            }
                        }
                        Some(InputEvent::KeyDown {
                            id,
                            key: key_code(code),
                            character,
                            repeat: value == 2,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

fn is_event_node(name: &OsStr) -> bool {
    name.as_bytes().starts_with(b"event")
}

struct Worker {
    activity: ActivitySink,
    stop: OwnedFd,
    inotify: Option<File>,
    devices: HashMap<PathBuf, Device>,
    keymap: Option<Keymap>,
    decoder: Decoder,
}

impl Worker {
    /// Opens every accessible keyboard. Permission errors are reported only when
    /// no keyboard can be observed at all.
    fn scan(&mut self) -> Result<(), String> {
        let mut denied = false;
        let entries = std::fs::read_dir(INPUT_DIR)
            .map_err(|e| format!("Could not list {INPUT_DIR}: {e}. Try Retry listener."))?;
        for entry in entries.flatten() {
            if is_event_node(&entry.file_name()) {
                denied |= self.add(entry.path());
            }
        }
        if !self.devices.is_empty() {
            Ok(())
        } else if denied {
            Err(PERMISSION_MESSAGE.into())
        } else {
            Err("No keyboard was found. Connect a keyboard, then choose Retry listener.".into())
        }
    }

    /// Returns true when the device exists but this user may not read it.
    fn add(&mut self, path: PathBuf) -> bool {
        if self.devices.contains_key(&path) {
            return false;
        }
        match Device::open(&path) {
            Ok(Some(device)) => {
                if let (Some(keymap), Some(leds)) = (&self.keymap, device.leds()) {
                    keymap.reset(Some(leds));
                }
                self.devices.insert(path, device);
                false
            }
            Ok(None) => false,
            Err(error) => error.kind() == std::io::ErrorKind::PermissionDenied,
        }
    }

    fn remove(&mut self, path: &Path) {
        if self.devices.remove(path).is_some() {
            // Pending releases from an unplugged keyboard will never arrive.
            if let Some(keymap) = &self.keymap {
                keymap.reset(None);
            }
            (self.activity)(InputEvent::ResetKeys);
        }
    }

    fn hotplug(&mut self) {
        let mut buffer = [0_u8; 4096];
        let Some(inotify) = self.inotify.as_mut() else {
            return;
        };
        let mut added = Vec::new();
        while let Ok(length) = inotify.read(&mut buffer) {
            let mut offset = 0;
            let header = mem::size_of::<libc::inotify_event>();
            while offset + header <= length {
                let event: libc::inotify_event =
                    unsafe { ptr::read_unaligned(buffer[offset..].as_ptr().cast()) };
                let name_len = event.len as usize;
                let name = &buffer[offset + header..(offset + header + name_len).min(length)];
                let name = name.split(|&b| b == 0).next().unwrap_or_default();
                let name = OsStr::from_bytes(name);
                // Udev applies permissions after creating a node, so retry on attribute changes.
                if is_event_node(name) {
                    added.push(Path::new(INPUT_DIR).join(name));
                }
                offset += header + name_len;
            }
        }
        for path in added {
            self.add(path);
        }
    }

    /// Reads all queued events. Returns false when the device is gone.
    fn drain(&mut self, path: &Path) -> bool {
        let Some(device) = self.devices.get_mut(path) else {
            return true;
        };
        let size = mem::size_of::<libc::input_event>();
        let mut buffer = [0_u8; 64 * mem::size_of::<libc::input_event>()];
        loop {
            match device.file.read(&mut buffer) {
                Ok(0) => return false,
                Ok(length) => {
                    for chunk in buffer[..length].chunks_exact(size) {
                        let event: libc::input_event =
                            unsafe { ptr::read_unaligned(chunk.as_ptr().cast()) };
                        if let Some(event) = self.decoder.event(
                            event.type_,
                            event.code,
                            event.value,
                            self.keymap.as_ref(),
                        ) {
                            (self.activity)(event);
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return true,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
    }

    /// Waits for keyboard input, hotplug or a stop request. Returns an error only
    /// when the listener can no longer continue.
    fn run(&mut self) -> Result<(), String> {
        loop {
            let paths: Vec<PathBuf> = self.devices.keys().cloned().collect();
            let mut fds = vec![poll_fd(self.stop.as_raw_fd())];
            if let Some(inotify) = &self.inotify {
                fds.push(poll_fd(inotify.as_raw_fd()));
            }
            let first_device = fds.len();
            fds.extend(
                paths
                    .iter()
                    .map(|path| poll_fd(self.devices[path].file.as_raw_fd())),
            );
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if ready < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(format!(
                    "The Linux keyboard listener stopped: {error}. Use Retry listener in Settings."
                ));
            }
            if fds[0].revents != 0 {
                return Ok(());
            }
            if first_device > 1 && fds[1].revents != 0 {
                self.hotplug();
            }
            for (fd, path) in fds[first_device..].iter().zip(&paths) {
                if fd.revents & (libc::POLLIN | libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0
                    && (fd.revents & libc::POLLNVAL != 0 || !self.drain(path))
                {
                    self.remove(path);
                }
            }
        }
    }
}

fn poll_fd(fd: RawFd) -> libc::pollfd {
    libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }
}

fn watch_input_dir() -> Option<File> {
    unsafe {
        let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
        if fd < 0 {
            return None;
        }
        let file = File::from_raw_fd(fd);
        let dir = CString::new(INPUT_DIR).ok()?;
        (libc::inotify_add_watch(fd, dir.as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB) >= 0)
            .then_some(file)
    }
}

fn stop_pipe() -> Result<(OwnedFd, OwnedFd), String> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } < 0 {
        return Err(format!(
            "Could not create keyboard listener: {}. Try Retry listener.",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

pub struct LinuxInput;

struct Listener {
    stop: OwnedFd,
    worker: Option<thread::JoinHandle<()>>,
}

impl InputListener for Listener {}

impl Drop for Listener {
    fn drop(&mut self) {
        unsafe {
            libc::write(self.stop.as_raw_fd(), [1_u8].as_ptr().cast(), 1);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl InputBackend for LinuxInput {
    fn start(
        &self,
        activity: ActivitySink,
        failure: FailureSink,
    ) -> Result<Box<dyn InputListener>, String> {
        let (stop_read, stop_write) = stop_pipe()?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("tapkin-key-evdev".into())
            .spawn(move || {
                let mut worker = Worker {
                    activity,
                    stop: stop_read,
                    // Watch before scanning so a keyboard plugged in meanwhile is not missed.
                    inotify: watch_input_dir(),
                    devices: HashMap::new(),
                    keymap: Keymap::load(),
                    decoder: Decoder::default(),
                };
                if let Err(error) = worker.scan() {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
                if ready_tx.send(Ok(())).is_err() {
                    return;
                }
                if let Err(error) = worker.run() {
                    failure(error);
                }
            })
            .map_err(|e| format!("Could not create keyboard listener thread: {e}"))?;
        let listener = Listener {
            stop: stop_write,
            worker: Some(worker),
        };
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Box::new(listener)),
            Ok(Err(error)) => Err(error),
            // Dropping the listener stops a worker that became ready too late.
            Err(_) => {
                Err("Linux keyboard listener did not become ready. Try Retry listener.".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_supported_physical_key() {
        let mapped: std::collections::BTreeSet<_> =
            (0..=KEY_MAX as u16).filter_map(key_code).collect();
        // Every KeyCode name documented in the README has a Linux key code.
        assert_eq!(mapped.len(), 89);
        for (code, name) in [
            (30, "KeyA"),
            (44, "KeyZ"),
            (11, "Digit0"),
            (57, "Space"),
            (28, "Enter"),
            (96, "NumpadEnter"),
            (103, "ArrowUp"),
            (72, "Numpad8"),
            (111, "Delete"),
            (83, "NumpadDecimal"),
            (98, "NumpadDivide"),
            (55, "NumpadMultiply"),
            (87, "F11"),
            (88, "F12"),
            (53, "Slash"),
        ] {
            assert_eq!(key_code(code), KeyCode::parse(name), "{name}");
        }
        for code in [0, 29, 42, 84, 86, 97, 110, 125, 0x110] {
            assert_eq!(key_code(code), None);
        }
    }

    #[test]
    fn ignores_pointer_buttons() {
        assert!(is_keyboard_key(KEY_A));
        assert!(is_keyboard_key(KEY_OK));
        assert!(!is_keyboard_key(0));
        assert!(!is_keyboard_key(0x110)); // BTN_LEFT
        assert!(!is_keyboard_key(0x14a)); // BTN_TOUCH
        assert!(!is_keyboard_key(BTN_TRIGGER_HAPPY));
    }

    #[test]
    fn decodes_press_repeat_release_and_drops() {
        let mut decoder = Decoder::default();
        assert!(matches!(
            decoder.event(EV_KEY, KEY_A, 1, None),
            Some(InputEvent::KeyDown { id: 30, key: Some(k), character: None, repeat: false })
                if Some(k) == KeyCode::parse("KeyA")
        ));
        assert!(matches!(
            decoder.event(EV_KEY, KEY_A, 2, None),
            Some(InputEvent::KeyDown {
                id: 30,
                repeat: true,
                ..
            })
        ));
        assert!(matches!(
            decoder.event(EV_KEY, KEY_A, 0, None),
            Some(InputEvent::KeyUp { id: 30 })
        ));
        assert!(decoder.event(EV_KEY, 0x110, 1, None).is_none());
        assert!(decoder.event(0x02, 0, 1, None).is_none()); // EV_REL
        assert!(matches!(
            decoder.event(EV_SYN, SYN_DROPPED, 0, None),
            Some(InputEvent::ResetKeys)
        ));
        // Events up to the next report are incomplete after a drop.
        assert!(decoder.event(EV_KEY, KEY_A, 0, None).is_none());
        assert!(decoder.event(EV_SYN, SYN_REPORT, 0, None).is_none());
        assert!(decoder.event(EV_KEY, KEY_A, 1, None).is_some());
    }

    #[test]
    fn reads_bitmasks() {
        let bits = [0b0000_0010, 0b1000_0000];
        assert!(bit(&bits, 1));
        assert!(bit(&bits, 15));
        assert!(!bit(&bits, 0));
        assert!(!bit(&bits, 99));
        assert_eq!(eviocg(0x20, 4), 0x8004_4520);
    }

    #[test]
    fn translates_characters_with_the_default_keymap() {
        let Some(keymap) = Keymap::load() else {
            return; // libxkbcommon is not installed on this machine.
        };
        // Only US-compatible keys are checked; the user's layout may differ elsewhere.
        let mut decoder = Decoder::default();
        if keymap.character(KEY_SPACE) != Some(' ') {
            return;
        }
        decoder.event(EV_KEY, 42, 1, Some(&keymap)); // Left Shift
        let shifted = decoder.event(EV_KEY, KEY_SPACE, 1, Some(&keymap));
        assert!(matches!(
            shifted,
            Some(InputEvent::KeyDown {
                character: Some(' '),
                ..
            })
        ));
        decoder.event(EV_KEY, KEY_SPACE, 0, Some(&keymap));
        decoder.event(EV_KEY, 42, 0, Some(&keymap));
        decoder.event(EV_KEY, 29, 1, Some(&keymap)); // Left Control
        assert!(matches!(
            decoder.event(EV_KEY, KEY_SPACE, 1, Some(&keymap)),
            Some(InputEvent::KeyDown {
                character: None,
                ..
            })
        ));
    }
}
