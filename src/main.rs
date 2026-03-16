#![cfg_attr(target_os = "macos", allow(non_camel_case_types))]

#[cfg(target_os = "macos")]
use core_foundation::base::mach_port_t;
#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{process, thread, time::Duration};

// ── macOS: IOKit FFI bindings ───────────────────────────────────────

#[cfg(target_os = "macos")]
type IOReturn = i32;
#[cfg(target_os = "macos")]
type io_object_t = mach_port_t;
#[cfg(target_os = "macos")]
type io_service_t = io_object_t;
#[cfg(target_os = "macos")]
type io_connect_t = mach_port_t;

#[cfg(target_os = "macos")]
const KIO_HIDCAPS_LOCK_STATE: u32 = 1;
#[cfg(target_os = "macos")]
const KIO_HIDPARAM_CONNECT_TYPE: u32 = 1;

#[cfg(target_os = "macos")]
extern "C" {
    fn mach_task_self() -> mach_port_t;
}

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const i8) -> *mut std::ffi::c_void;
    fn IOServiceGetMatchingService(
        main_port: mach_port_t,
        matching: *mut std::ffi::c_void,
    ) -> io_service_t;
    fn IOServiceOpen(
        service: io_service_t,
        owning_task: mach_port_t,
        connect_type: u32,
        connection: *mut io_connect_t,
    ) -> IOReturn;
    fn IOServiceClose(connection: io_connect_t) -> IOReturn;
    fn IOHIDGetModifierLockState(
        connection: io_connect_t,
        selector: u32,
        state: *mut bool,
    ) -> IOReturn;
    fn IOHIDSetModifierLockState(
        connection: io_connect_t,
        selector: u32,
        state: bool,
    ) -> IOReturn;
    fn IOObjectRelease(object: io_object_t) -> IOReturn;
}

static RUNNING: AtomicBool = AtomicBool::new(true);

// ── macOS implementation ────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn open_hid_connection() -> Result<io_connect_t, &'static str> {
    let class_name = CString::new("IOHIDSystem").unwrap();
    unsafe {
        let matching = IOServiceMatching(class_name.as_ptr());
        if matching.is_null() {
            return Err("Failed to create IOServiceMatching");
        }
        let service = IOServiceGetMatchingService(0, matching);
        if service == 0 {
            return Err("Failed to find IOHIDSystem service");
        }
        let mut connection: io_connect_t = 0;
        let ret = IOServiceOpen(service, mach_task_self(), KIO_HIDPARAM_CONNECT_TYPE, &mut connection);
        IOObjectRelease(service);
        if ret != 0 {
            return Err("Failed to open IOHIDSystem service");
        }
        Ok(connection)
    }
}

#[cfg(target_os = "macos")]
fn toggle_caps_lock(connection: io_connect_t) {
    unsafe {
        let mut state: bool = false;
        let ret = IOHIDGetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, &mut state);
        if ret != 0 {
            eprintln!("Warning: Failed to get Caps Lock state");
            return;
        }
        let ret = IOHIDSetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, !state);
        if ret != 0 {
            eprintln!("Warning: Failed to set Caps Lock state");
            return;
        }
        thread::sleep(Duration::from_millis(100));
        let ret = IOHIDSetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, state);
        if ret != 0 {
            eprintln!("Warning: Failed to restore Caps Lock state");
        }
    }
}

#[cfg(target_os = "macos")]
fn cleanup_caps_lock(connection: io_connect_t) {
    unsafe {
        let _ = IOHIDSetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, false);
        IOServiceClose(connection);
    }
}

// ── Windows implementation ──────────────────────────────────────────

#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_CAPITAL,
};

#[cfg(target_os = "windows")]
fn send_key_event(key: u16, flags: KEYBD_EVENT_FLAGS) {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(key),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "windows")]
fn toggle_caps_lock() {
    send_key_event(VK_CAPITAL.0, KEYBD_EVENT_FLAGS(0));
    send_key_event(VK_CAPITAL.0, KEYEVENTF_KEYUP);
    thread::sleep(Duration::from_millis(100));
    send_key_event(VK_CAPITAL.0, KEYBD_EVENT_FLAGS(0));
    send_key_event(VK_CAPITAL.0, KEYEVENTF_KEYUP);
}

#[cfg(target_os = "windows")]
fn cleanup_caps_lock() {
    let state = unsafe { GetKeyState(VK_CAPITAL.0 as i32) };
    // Low-order bit indicates whether Caps Lock is toggled on
    if state & 1 != 0 {
        send_key_event(VK_CAPITAL.0, KEYBD_EVENT_FLAGS(0));
        send_key_event(VK_CAPITAL.0, KEYEVENTF_KEYUP);
    }
}

// ── Ctrl+C handling ─────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
fn ctrlc_setup(flag: &'static AtomicBool) {
    unsafe {
        libc_signal(
            2, // SIGINT
            signal_handler as *const () as usize,
        );
    }
    RUNNING_PTR.store(flag as *const AtomicBool as usize, Ordering::SeqCst);
}

#[cfg(not(target_os = "windows"))]
static RUNNING_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(not(target_os = "windows"))]
extern "C" fn signal_handler(_sig: i32) {
    let ptr = RUNNING_PTR.load(Ordering::SeqCst);
    if ptr != 0 {
        let flag = unsafe { &*(ptr as *const AtomicBool) };
        flag.store(false, Ordering::SeqCst);
    }
}

#[cfg(not(target_os = "windows"))]
extern "C" {
    #[link_name = "signal"]
    fn libc_signal(signum: i32, handler: usize) -> usize;
}

#[cfg(target_os = "windows")]
fn ctrlc_setup(flag: &'static AtomicBool) {
    unsafe {
        windows::Win32::System::Console::SetConsoleCtrlHandler(
            Some(console_ctrl_handler),
            true,
        )
        .ok();
    }
    RUNNING_PTR_WIN.store(flag as *const AtomicBool as usize, Ordering::SeqCst);
}

#[cfg(target_os = "windows")]
static RUNNING_PTR_WIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "windows")]
unsafe extern "system" fn console_ctrl_handler(
    _ctrl_type: u32,
) -> windows::Win32::Foundation::BOOL {
    let ptr = RUNNING_PTR_WIN.load(Ordering::SeqCst);
    if ptr != 0 {
        let flag = unsafe { &*(ptr as *const AtomicBool) };
        flag.store(false, Ordering::SeqCst);
    }
    windows::Win32::Foundation::TRUE
}

// ── main ────────────────────────────────────────────────────────────

fn main() {
    #[cfg(target_os = "macos")]
    let connection = match open_hid_connection() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Make sure the app has Accessibility permissions.");
            process::exit(1);
        }
    };

    ctrlc_setup(&RUNNING);

    println!("Online keeper started. Press Ctrl+C to stop.");

    while RUNNING.load(Ordering::SeqCst) {
        #[cfg(target_os = "macos")]
        toggle_caps_lock(connection);

        #[cfg(target_os = "windows")]
        toggle_caps_lock();

        for _ in 0..50 {
            if !RUNNING.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    println!("\nStopping... cleaning up Caps Lock.");

    #[cfg(target_os = "macos")]
    cleanup_caps_lock(connection);

    #[cfg(target_os = "windows")]
    cleanup_caps_lock();
}
