//! Raw syscall interface

use core::arch::asm;

/// Perform a syscall
#[inline]
pub unsafe fn syscall0(num: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

#[inline]
pub unsafe fn syscall1(num: u64, arg0: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

#[inline]
pub unsafe fn syscall2(num: u64, arg0: u64, arg1: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

#[inline]
pub unsafe fn syscall3(num: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inout("rax") num => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        out("rcx") _,
        out("r11") _,
        options(nostack),
    );
    ret
}

/// Syscall numbers (must match kernel)
pub mod nr {
    pub const RING_SETUP: u64 = 0;
    pub const RING_ENTER: u64 = 1;
    pub const SEND: u64 = 2;
    pub const RECEIVE: u64 = 3;
    pub const CALL: u64 = 4;
    pub const REPLY: u64 = 5;

    pub const CAP_DERIVE: u64 = 16;
    pub const CAP_REVOKE: u64 = 17;

    pub const TENSOR_ALLOC: u64 = 96;
    pub const TENSOR_FREE: u64 = 97;
    pub const INFERENCE_CREATE: u64 = 99;
    pub const INFERENCE_SUBMIT: u64 = 100;

    pub const CHECKPOINT: u64 = 128;
    pub const RESTORE: u64 = 129;
}
