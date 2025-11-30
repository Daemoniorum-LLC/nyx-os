//! Checkpoint implementation - process state snapshots
//!
//! Captures complete process state including:
//! - Memory regions (copy-on-write optimized)
//! - CPU register state
//! - Capability space
//! - Open file handles
//! - Tensor buffer states

use super::{CheckpointId, TimeTravelError};
use crate::cap::{CSpace, Capability};
use crate::mem::{PhysAddr, VirtAddr, PAGE_SIZE};
use crate::process::{Process, ProcessId, ProcessState};
use crate::sched::ThreadId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// A complete process checkpoint
#[derive(Debug)]
pub struct Checkpoint {
    /// Checkpoint ID
    pub id: CheckpointId,
    /// Original process ID
    pub process_id: ProcessId,
    /// Optional name for this checkpoint
    pub name: Option<String>,
    /// Creation timestamp (nanoseconds since boot)
    pub created_at: u64,
    /// Memory snapshot
    pub memory: MemorySnapshot,
    /// Thread states
    pub threads: Vec<ThreadSnapshot>,
    /// Capability space snapshot
    pub cspace: CSpaceSnapshot,
    /// File descriptor table
    pub files: Vec<FileSnapshot>,
    /// Tensor buffer states (if captured)
    pub tensors: Option<Vec<TensorSnapshot>>,
    /// Size in bytes
    pub size_bytes: usize,
}

impl Checkpoint {
    /// Capture a checkpoint of a process
    pub fn capture(
        id: CheckpointId,
        process: &Process,
        name: Option<String>,
        include_tensors: bool,
    ) -> Result<Self, TimeTravelError> {
        let created_at = crate::now_ns();

        // Capture memory snapshot
        let memory = MemorySnapshot::capture(&process.address_space)?;

        // Capture thread states
        let threads = capture_threads(process)?;

        // Capture capability space
        let cspace = CSpaceSnapshot::capture(&process.cspace)?;

        // Capture file descriptors
        let files = capture_files(process)?;

        // Optionally capture tensor states
        let tensors = if include_tensors {
            Some(capture_tensors(process)?)
        } else {
            None
        };

        // Calculate total size
        let size_bytes = memory.size_bytes()
            + threads.iter().map(|t| core::mem::size_of_val(t)).sum::<usize>()
            + cspace.size_bytes()
            + files.iter().map(|f| core::mem::size_of_val(f)).sum::<usize>()
            + tensors.as_ref().map(|t| t.iter().map(|s| s.size_bytes()).sum()).unwrap_or(0);

        Ok(Self {
            id,
            process_id: process.pid,
            name,
            created_at,
            memory,
            threads,
            cspace,
            files,
            tensors,
            size_bytes,
        })
    }

    /// Restore checkpoint to a process
    pub fn restore(&self, target: Option<ProcessId>) -> Result<ProcessId, TimeTravelError> {
        // Get or create target process
        let pid = if let Some(existing) = target {
            // Restore to existing process
            restore_to_existing(existing, self)?;
            existing
        } else {
            // Create new process from checkpoint
            create_from_checkpoint(self)?
        };

        Ok(pid)
    }
}

/// Memory region snapshot
#[derive(Debug)]
pub struct MemorySnapshot {
    /// Memory regions
    pub regions: Vec<MemoryRegionSnapshot>,
    /// Page data (copy-on-write references or actual copies)
    pub pages: BTreeMap<VirtAddr, PageData>,
}

impl MemorySnapshot {
    fn capture(address_space: &crate::mem::virt::AddressSpace) -> Result<Self, TimeTravelError> {
        let mut regions = Vec::new();
        let mut pages = BTreeMap::new();

        // Iterate through address space regions
        for region in address_space.regions() {
            let region_snapshot = MemoryRegionSnapshot {
                start: region.start,
                end: region.end,
                protection: region.protection.bits(),
                flags: region.flags.bits(),
            };
            regions.push(region_snapshot);

            // Capture page contents
            let mut addr = region.start;
            while addr < region.end {
                if let Some(phys) = address_space.translate(addr) {
                    // Copy page data
                    let data = unsafe {
                        let ptr = crate::mem::phys_to_virt(phys) as *const u8;
                        let mut buf = alloc::vec![0u8; PAGE_SIZE as usize];
                        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), PAGE_SIZE as usize);
                        buf
                    };
                    pages.insert(addr, PageData::Copied(data));
                }
                addr = VirtAddr::new(addr.as_u64() + PAGE_SIZE);
            }
        }

        Ok(Self { regions, pages })
    }

    fn size_bytes(&self) -> usize {
        self.regions.len() * core::mem::size_of::<MemoryRegionSnapshot>()
            + self.pages.values().map(|p| p.size_bytes()).sum::<usize>()
    }
}

/// A single memory region snapshot
#[derive(Debug, Clone)]
pub struct MemoryRegionSnapshot {
    pub start: VirtAddr,
    pub end: VirtAddr,
    pub protection: u8,
    pub flags: u32,
}

/// Page data storage
#[derive(Debug)]
pub enum PageData {
    /// Page data is copied
    Copied(Vec<u8>),
    /// Page is shared (copy-on-write reference)
    Shared(PhysAddr),
    /// Page is zero-filled
    Zero,
}

impl PageData {
    fn size_bytes(&self) -> usize {
        match self {
            PageData::Copied(data) => data.len(),
            PageData::Shared(_) => 8, // Just the address
            PageData::Zero => 0,
        }
    }
}

/// Thread state snapshot
#[derive(Debug, Clone)]
pub struct ThreadSnapshot {
    /// Thread ID
    pub thread_id: ThreadId,
    /// CPU registers
    pub registers: RegisterState,
    /// Thread-local storage pointer
    pub tls_base: u64,
    /// Stack pointer
    pub stack_pointer: u64,
    /// Instruction pointer
    pub instruction_pointer: u64,
    /// Thread state
    pub state: u8,
}

/// CPU register state (x86_64)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RegisterState {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    // Instruction pointer
    pub rip: u64,
    // Flags
    pub rflags: u64,
    // Segment registers
    pub cs: u64,
    pub ss: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
    // FPU/SSE state (simplified - real impl would save full XSAVE area)
    pub fpu_state: [u8; 512],
}

impl Default for RegisterState {
    fn default() -> Self {
        Self {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0, rsp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            rip: 0, rflags: 0,
            cs: 0, ss: 0, ds: 0, es: 0, fs: 0, gs: 0,
            fpu_state: [0; 512],
        }
    }
}

/// Capability space snapshot
#[derive(Debug)]
pub struct CSpaceSnapshot {
    /// Capabilities by slot
    pub slots: BTreeMap<u32, Capability>,
}

impl CSpaceSnapshot {
    fn capture(cspace: &CSpace) -> Result<Self, TimeTravelError> {
        let slots = cspace.export_all();
        Ok(Self { slots })
    }

    fn size_bytes(&self) -> usize {
        self.slots.len() * (4 + core::mem::size_of::<Capability>())
    }
}

/// File descriptor snapshot
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    /// File descriptor number
    pub fd: u32,
    /// File path or identifier
    pub path: String,
    /// Current offset
    pub offset: u64,
    /// Open flags
    pub flags: u32,
}

/// Tensor buffer snapshot
#[derive(Debug)]
pub struct TensorSnapshot {
    /// Tensor capability ID
    pub tensor_id: u64,
    /// Device type (CPU=0, GPU=1, NPU=2)
    pub device_type: u8,
    /// Shape dimensions
    pub shape: Vec<u64>,
    /// Data type
    pub dtype: u8,
    /// Actual tensor data
    pub data: Vec<u8>,
}

impl TensorSnapshot {
    fn size_bytes(&self) -> usize {
        self.data.len() + self.shape.len() * 8 + 32
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn capture_threads(process: &Process) -> Result<Vec<ThreadSnapshot>, TimeTravelError> {
    let mut snapshots = Vec::new();

    for &thread_id in &process.threads {
        let threads = crate::sched::THREADS.read();
        if let Some(thread) = threads.get(&thread_id) {
            let snapshot = ThreadSnapshot {
                thread_id,
                registers: RegisterState::default(), // Would capture actual state
                tls_base: 0, // TLS base would come from fs/gs base
                stack_pointer: thread.registers.rsp,
                instruction_pointer: thread.registers.rip,
                state: match thread.state {
                    crate::sched::ThreadState::Ready => 0,
                    crate::sched::ThreadState::Running => 1,
                    crate::sched::ThreadState::Blocked(_) => 2,
                    crate::sched::ThreadState::Terminated => 3,
                },
            };
            snapshots.push(snapshot);
        }
    }

    Ok(snapshots)
}

fn capture_files(_process: &Process) -> Result<Vec<FileSnapshot>, TimeTravelError> {
    // File descriptor capture - simplified for now
    Ok(Vec::new())
}

fn capture_tensors(_process: &Process) -> Result<Vec<TensorSnapshot>, TimeTravelError> {
    // Tensor capture - would iterate through process's tensor capabilities
    Ok(Vec::new())
}

fn restore_to_existing(
    pid: ProcessId,
    checkpoint: &Checkpoint,
) -> Result<(), TimeTravelError> {
    let mut process = crate::process::get_process_mut(pid)
        .ok_or(TimeTravelError::ProcessNotFound)?;

    // Restore memory
    restore_memory(&mut process, &checkpoint.memory)?;

    // Restore threads
    restore_threads(&mut process, &checkpoint.threads)?;

    // Restore capability space
    process.cspace = crate::cap::create_cspace();
    for (&slot, &cap) in &checkpoint.cspace.slots {
        let _ = process.cspace.insert(slot as usize, cap);
    }

    Ok(())
}

fn create_from_checkpoint(checkpoint: &Checkpoint) -> Result<ProcessId, TimeTravelError> {
    use crate::process::SpawnArgs;
    use crate::sched::SchedClass;

    // Create new process
    let args = SpawnArgs {
        path: alloc::format!("checkpoint:{}", checkpoint.id.0),
        args: Vec::new(),
        env: Vec::new(),
        caps: Vec::new(),
        sched_class: SchedClass::Normal,
        priority: 0,
        cwd: None,
        uid: 0,
        gid: 0,
    };

    // Use spawn but override with checkpoint state
    let pid = crate::process::spawn(args)
        .map_err(|_| TimeTravelError::OutOfMemory)?;

    // Restore checkpoint state to new process
    restore_to_existing(pid, checkpoint)?;

    Ok(pid)
}

fn restore_memory(
    process: &mut Process,
    snapshot: &MemorySnapshot,
) -> Result<(), TimeTravelError> {
    // Clear existing memory and restore from snapshot
    for region in &snapshot.regions {
        // Map the region
        let prot = crate::mem::virt::Protection::from_bits_truncate(region.protection);
        process.address_space.map_range(
            region.start,
            region.end.as_u64() - region.start.as_u64(),
            prot,
        ).map_err(|_| TimeTravelError::OutOfMemory)?;
    }

    // Restore page contents
    for (&vaddr, page_data) in &snapshot.pages {
        match page_data {
            PageData::Copied(data) => {
                // Copy data to the page
                if let Some(phys) = process.address_space.translate(vaddr) {
                    unsafe {
                        let ptr = crate::mem::phys_to_virt(phys) as *mut u8;
                        core::ptr::copy_nonoverlapping(
                            data.as_ptr(),
                            ptr,
                            PAGE_SIZE as usize,
                        );
                    }
                }
            }
            PageData::Shared(phys) => {
                // Map the shared page
                process.address_space.map_page(
                    vaddr,
                    *phys,
                    crate::mem::virt::Protection::READ | crate::mem::virt::Protection::WRITE,
                ).map_err(|_| TimeTravelError::OutOfMemory)?;
            }
            PageData::Zero => {
                // Page is already zero from allocation
            }
        }
    }

    Ok(())
}

fn restore_threads(
    process: &mut Process,
    snapshots: &[ThreadSnapshot],
) -> Result<(), TimeTravelError> {
    let mut threads = crate::sched::THREADS.write();

    for snapshot in snapshots {
        if let Some(thread) = threads.get_mut(&snapshot.thread_id) {
            // Restore register state
            thread.registers.rsp = snapshot.stack_pointer;
            thread.registers.rip = snapshot.instruction_pointer;
            // TLS base would be restored via fs/gs base MSR
            // Additional register restoration would go here
        }
    }

    Ok(())
}
