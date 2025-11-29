//! Kernel panic handler

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Disable interrupts to prevent further execution
    #[cfg(feature = "arch-x86_64")]
    crate::arch::disable_interrupts();

    // Print panic info
    if let Some(location) = info.location() {
        log::error!(
            "KERNEL PANIC at {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }

    if let Some(message) = info.message() {
        log::error!("  {}", message);
    }

    // Halt forever
    loop {
        #[cfg(feature = "arch-x86_64")]
        crate::arch::halt();
    }
}
