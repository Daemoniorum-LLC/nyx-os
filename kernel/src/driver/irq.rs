//! Interrupt Request (IRQ) management
//!
//! Provides IRQ registration, dispatch, and user-space notification.

use super::DriverError;
use crate::ipc::{create_notification, Notification};
use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

/// IRQ handler registry
static IRQ_HANDLERS: RwLock<[Option<IrqHandler>; MAX_IRQS]> = RwLock::new([const { None }; MAX_IRQS]);

/// IRQ pending counts
static IRQ_PENDING: [AtomicU64; MAX_IRQS] = [const { AtomicU64::new(0) }; MAX_IRQS];

/// IRQ waiters (notification objects)
static IRQ_WAITERS: RwLock<BTreeMap<u8, alloc::vec::Vec<ObjectId>>> = RwLock::new(BTreeMap::new());

/// Maximum number of IRQs
const MAX_IRQS: usize = 256;

/// IRQ vector offset (first 32 are exceptions)
const IRQ_VECTOR_OFFSET: u8 = 32;

/// IRQ handler info
#[derive(Clone)]
pub struct IrqHandler {
    /// Process that registered the handler
    pub owner_process: crate::process::ProcessId,
    /// Notification object for signaling
    pub notification: ObjectId,
    /// IRQ flags
    pub flags: IrqFlags,
    /// IRQ count
    pub count: u64,
}

bitflags::bitflags! {
    /// IRQ handler flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct IrqFlags: u32 {
        /// Shared IRQ (multiple handlers)
        const SHARED = 1 << 0;
        /// Edge-triggered
        const EDGE = 1 << 1;
        /// Level-triggered
        const LEVEL = 1 << 2;
        /// Wake from sleep
        const WAKE = 1 << 3;
        /// One-shot (disable after delivery)
        const ONESHOT = 1 << 4;
    }
}

/// Initialize IRQ subsystem
pub fn init() {
    log::debug!("Initializing IRQ subsystem");

    // Initialize APIC/IOAPIC
    init_apic();

    log::debug!("IRQ subsystem initialized");
}

/// Initialize APIC
fn init_apic() {
    // Basic APIC initialization (details in arch-specific code)
    crate::arch::x86_64::smp::init_apic_timer(100); // 100Hz
}

/// Validate IRQ number
pub fn validate_irq(irq: u8) -> Result<(), DriverError> {
    if (irq as usize) < MAX_IRQS {
        Ok(())
    } else {
        Err(DriverError::IrqNotFound)
    }
}

/// Register an IRQ handler
pub fn register_irq(
    irq: u8,
    process: crate::process::ProcessId,
    flags: IrqFlags,
) -> Result<Capability, DriverError> {
    validate_irq(irq)?;

    let mut handlers = IRQ_HANDLERS.write();

    // Check if already registered (unless shared)
    if let Some(existing) = &handlers[irq as usize] {
        if !existing.flags.contains(IrqFlags::SHARED) || !flags.contains(IrqFlags::SHARED) {
            return Err(DriverError::IrqAlreadyRegistered);
        }
    }

    // Create notification object for this handler
    let notification = crate::ipc::create_notification()
        .map_err(|_| DriverError::OutOfResources)?;

    let handler = IrqHandler {
        owner_process: process,
        notification: notification.object_id,
        flags,
        count: 0,
    };

    handlers[irq as usize] = Some(handler);

    // Enable IRQ in IOAPIC
    enable_irq(irq);

    log::debug!("Registered IRQ {} for process {:?}", irq, process);

    // Return a capability for waiting on this IRQ
    let cap = unsafe {
        Capability::new_unchecked(
            ObjectId::new(ObjectType::Interrupt),
            Rights::IRQ | Rights::WAIT | Rights::POLL,
        )
    };

    Ok(cap)
}

/// Unregister an IRQ handler
pub fn unregister_irq(irq: u8) -> Result<(), DriverError> {
    validate_irq(irq)?;

    let mut handlers = IRQ_HANDLERS.write();

    if handlers[irq as usize].is_none() {
        return Err(DriverError::IrqNotFound);
    }

    handlers[irq as usize] = None;

    // Disable IRQ in IOAPIC
    disable_irq(irq);

    log::debug!("Unregistered IRQ {}", irq);

    Ok(())
}

/// Enable an IRQ
fn enable_irq(irq: u8) {
    // Program IOAPIC to enable this IRQ
    let ioapic_base = 0xFEC0_0000u64; // Standard IOAPIC base

    let vector = IRQ_VECTOR_OFFSET + irq;

    unsafe {
        // Select redirection table entry (2 registers per IRQ)
        let ioregsel = ioapic_base as *mut u32;
        let iowin = (ioapic_base + 0x10) as *mut u32;

        // Low 32 bits: vector and delivery mode
        core::ptr::write_volatile(ioregsel, 0x10 + (irq as u32) * 2);
        core::ptr::write_volatile(iowin, vector as u32); // Fixed delivery, physical dest

        // High 32 bits: destination APIC ID (0 = BSP)
        core::ptr::write_volatile(ioregsel, 0x10 + (irq as u32) * 2 + 1);
        core::ptr::write_volatile(iowin, 0);
    }
}

/// Disable an IRQ
fn disable_irq(irq: u8) {
    let ioapic_base = 0xFEC0_0000u64;

    unsafe {
        let ioregsel = ioapic_base as *mut u32;
        let iowin = (ioapic_base + 0x10) as *mut u32;

        // Set mask bit in low 32 bits
        core::ptr::write_volatile(ioregsel, 0x10 + (irq as u32) * 2);
        let current = core::ptr::read_volatile(iowin);
        core::ptr::write_volatile(iowin, current | (1 << 16)); // Mask bit
    }
}

/// Handle IRQ from interrupt handler
pub fn handle_irq(vector: u8) {
    let irq = vector.saturating_sub(IRQ_VECTOR_OFFSET);

    // Increment pending count
    IRQ_PENDING[irq as usize].fetch_add(1, Ordering::SeqCst);

    // Signal any registered handlers
    let handlers = IRQ_HANDLERS.read();
    if let Some(handler) = &handlers[irq as usize] {
        // Signal the notification object
        crate::ipc::signal(handler.notification, 1 << irq)
            .ok();
    }

    // Send EOI
    crate::arch::x86_64::smp::send_eoi();
}

/// Wait for an IRQ
pub fn wait_irq(irq: u8) -> Result<(), DriverError> {
    validate_irq(irq)?;

    let notification = {
        let handlers = IRQ_HANDLERS.read();
        handlers[irq as usize].as_ref().map(|h| h.notification)
    };

    if let Some(notification) = notification {
        // Wait on the notification object
        crate::ipc::wait(notification, 1 << irq, None)
            .map_err(|_| DriverError::HardwareError)?;

        // Clear pending count
        IRQ_PENDING[irq as usize].store(0, Ordering::SeqCst);

        Ok(())
    } else {
        Err(DriverError::IrqNotFound)
    }
}

/// Acknowledge an IRQ (re-enable after handling)
pub fn ack_irq(irq: u8) {
    // For level-triggered IRQs, may need to unmask
    // For edge-triggered, just clear any pending state
    IRQ_PENDING[irq as usize].store(0, Ordering::SeqCst);
}

/// Check if IRQ is pending
pub fn is_irq_pending(irq: u8) -> bool {
    IRQ_PENDING[irq as usize].load(Ordering::SeqCst) > 0
}

/// Get IRQ count
pub fn irq_count(irq: u8) -> u64 {
    let handlers = IRQ_HANDLERS.read();
    handlers[irq as usize]
        .as_ref()
        .map(|h| h.count)
        .unwrap_or(0)
}

/// MSI (Message Signaled Interrupts) support
pub mod msi {
    use super::*;

    /// Allocate MSI vectors
    pub fn allocate_vectors(count: u8) -> Result<u8, DriverError> {
        // Find contiguous free vectors starting at 48
        static NEXT_MSI_VECTOR: AtomicU64 = AtomicU64::new(48);

        let base = NEXT_MSI_VECTOR.fetch_add(count as u64, Ordering::SeqCst) as u8;

        if base + count > 255 {
            return Err(DriverError::OutOfResources);
        }

        Ok(base)
    }

    /// Configure MSI for a device
    pub fn configure(
        base_vector: u8,
        count: u8,
        address: u64,
        data: u32,
    ) -> Result<(), DriverError> {
        // MSI configuration is device-specific
        // Address format: 0xFEE0_0000 | (dest_id << 12)
        // Data format: vector | (edge_trigger << 15) | (level_assert << 14)
        Ok(())
    }

    /// Configure MSI-X for a device
    pub fn configure_msix(
        table_addr: u64,
        pba_addr: u64,
        vectors: &[(u8, u64, u32)], // (vector, address, data)
    ) -> Result<(), DriverError> {
        // MSI-X uses a memory-mapped table
        for (i, &(vector, address, data)) in vectors.iter().enumerate() {
            unsafe {
                let entry_addr = table_addr + (i as u64) * 16;
                core::ptr::write_volatile(entry_addr as *mut u64, address);
                core::ptr::write_volatile((entry_addr + 8) as *mut u32, data);
                core::ptr::write_volatile((entry_addr + 12) as *mut u32, 0); // Unmask
            }
        }
        Ok(())
    }
}
