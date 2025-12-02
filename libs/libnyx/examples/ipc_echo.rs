//! IPC Echo Server/Client Example
//!
//! Demonstrates basic IPC messaging between processes.
//!
//! The server receives messages and echoes them back.
//! The client sends a message and waits for the response.

#![no_std]
#![no_main]

use libnyx::prelude::*;
use libnyx::cap::Capability;

/// Program entry point
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // In a real program, you'd determine server/client mode from args
    // For this example, we show both sides
    exit(0);
}

/// Echo server: receives messages and echoes them back
fn run_server(endpoint: Capability) -> Result<(), Error> {
    let mut buffer = [0u8; 4096];

    loop {
        // Wait for a message
        let len = ipc::receive(endpoint, &mut buffer, None)?;

        // Echo it back
        ipc::reply(endpoint, &buffer[..len])?;
    }
}

/// Echo client: sends a message and receives the echo
fn run_client(endpoint: Capability, message: &[u8]) -> Result<usize, Error> {
    let mut response = [0u8; 4096];

    // Send request and wait for reply
    let len = ipc::call(endpoint, message, &mut response)?;

    // Verify the echo matches
    if &response[..len] == message {
        Ok(len)
    } else {
        Err(Error::InvalidArgument)
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(255);
}
