//! Minimal "Hello World" example for Nyx OS
//!
//! This demonstrates the basic structure of a Nyx userspace program.

#![no_std]
#![no_main]

use libnyx::prelude::*;

/// Program entry point
#[no_mangle]
pub extern "C" fn _start() -> ! {
    main();
    exit(0);
}

fn main() {
    // Get our process ID
    let pid = match getpid() {
        Ok(p) => p,
        Err(e) => {
            // Can't print without a debug syscall, just exit with error
            exit(1);
        }
    };

    // Sleep for 1 second
    if let Err(_) = sleep_secs(1) {
        exit(2);
    }

    // Exit successfully
    exit(0);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(255);
}
