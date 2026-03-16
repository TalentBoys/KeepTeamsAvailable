#![allow(non_camel_case_types)]

use core_foundation::base::mach_port_t;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{process, thread, time::Duration};

// IOKit FFI bindings
type IOReturn = i32;
type io_object_t = mach_port_t;
type io_service_t = io_object_t;
type io_connect_t = mach_port_t;

const KIO_HIDCAPS_LOCK_STATE: u32 = 1;
const KIO_HIDPARAM_CONNECT_TYPE: u32 = 1;

extern "C" {
    fn mach_task_self() -> mach_port_t;
}

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

/// Open a connection to the IOKit HID system
fn open_hid_connection() -> Result<io_connect_t, &'static str> {
    let class_name = CString::new("IOHIDSystem").unwrap();
    unsafe {
        let matching = IOServiceMatching(class_name.as_ptr());
        if matching.is_null() {
            return Err("Failed to create IOServiceMatching");
        }
        let service = IOServiceGetMatchingService(0, matching);
        // IOServiceMatching dict is consumed by IOServiceGetMatchingService
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

/// Toggle Caps Lock once (read current state, flip it, then flip back)
fn toggle_caps_lock(connection: io_connect_t) {
    unsafe {
        let mut state: bool = false;
        let ret = IOHIDGetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, &mut state);
        if ret != 0 {
            eprintln!("Warning: Failed to get Caps Lock state");
            return;
        }
        // Toggle on
        let ret = IOHIDSetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, !state);
        if ret != 0 {
            eprintln!("Warning: Failed to set Caps Lock state");
            return;
        }
        // Small delay then toggle back
        thread::sleep(Duration::from_millis(100));
        let ret = IOHIDSetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, state);
        if ret != 0 {
            eprintln!("Warning: Failed to restore Caps Lock state");
        }
    }
}

/// Ensure Caps Lock is off before exiting
fn cleanup_caps_lock(connection: io_connect_t) {
    unsafe {
        let _ = IOHIDSetModifierLockState(connection, KIO_HIDCAPS_LOCK_STATE, false);
        IOServiceClose(connection);
    }
}

fn main() {
    let connection = match open_hid_connection() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Make sure the app has Accessibility permissions.");
            process::exit(1);
        }
    };

    // Handle Ctrl+C: clean up Caps Lock and exit
    ctrlc_setup(&RUNNING);

    println!("Online keeper started. Press Ctrl+C to stop.");

    while RUNNING.load(Ordering::SeqCst) {
        println!("=");
        toggle_caps_lock(connection);

        // Sleep in small increments so Ctrl+C is responsive
        for _ in 0..50 {
            if !RUNNING.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    println!("\nStopping... cleaning up Caps Lock.");
    cleanup_caps_lock(connection);
}

/// Set up Ctrl+C handler using libc signals (no extra dependency needed)
fn ctrlc_setup(flag: &'static AtomicBool) {
    unsafe {
        libc_signal(
            2, // SIGINT
            signal_handler as *const () as usize,
        );
    }
    // Store the flag reference in a static for the handler
    RUNNING_PTR.store(flag as *const AtomicBool as usize, Ordering::SeqCst);
}

static RUNNING_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

extern "C" fn signal_handler(_sig: i32) {
    let ptr = RUNNING_PTR.load(Ordering::SeqCst);
    if ptr != 0 {
        let flag = unsafe { &*(ptr as *const AtomicBool) };
        flag.store(false, Ordering::SeqCst);
    }
}

extern "C" {
    #[link_name = "signal"]
    fn libc_signal(signum: i32, handler: usize) -> usize;
}
