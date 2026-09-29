//! The game object: scripts compiled and run through the VM, a tail call,
//! a large local buffer, and `main`.
#![no_std]
#![allow(dead_code, static_mut_refs)]

unsafe extern "C" {
    fn compile(text: *const u8) -> u32;
    fn vm_reset(code: *const u8, len: u32);
    fn vm_run(max_steps: u32) -> i32;
    fn vm_fail(what: *const u8);
    fn sym_set(name: *const u8, value: i32) -> i32;
    fn sym_get(name: *const u8) -> i32;
    fn sort_i32(values: *mut i32, n: u32);
    fn bsearch_i32(values: *const i32, n: u32, want: i32) -> i32;
    fn checksum(bytes: *const u8, halves: *const i16, n: u32) -> i32;
    fn acc64(acc: u64, v: u32, shift: u32) -> u64;
    fn memset(dst: *mut u8, value: i32, n: usize) -> *mut u8;
    static mut EMIT: [u8; 264];
    static mut VM_ERROR: *const u8;
}

static SCRIPTS: [&[u8]; 3] = [b"gold + 2 * 3\0", b"(hp < 10) & alive\0", b"level * level - 1\0"];

#[unsafe(no_mangle)]
pub static mut RESULTS: [i32; 8] = [0; 8];

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn run_script(index: u32) -> i32 {
    unsafe {
        let text = SCRIPTS[(index as usize) & 3.min(SCRIPTS.len() - 1)].as_ptr();
        if compile(text) != 0 {
            vm_fail(b"script did not compile\0".as_ptr());
            return -1;
        }
        let len = *(EMIT.as_ptr().add(256) as *const u32);
        vm_reset(EMIT.as_ptr(), len);
        vm_run(1000)
    }
}

/// A tail call: the compiler jumps to `run_script` rather than calling it.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn run_first() -> i32 {
    run_script(0)
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn setup_symbols() {
    unsafe {
        sym_set(b"gold\0".as_ptr(), 100);
        sym_set(b"hp\0".as_ptr(), 7);
        sym_set(b"alive\0".as_ptr(), 1);
        sym_set(b"level\0".as_ptr(), 12);
    }
}

/// A big frame: a local table filled, sorted and searched.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn table_check(seed: i32) -> i32 {
    let mut table = [0i32; 48];
    let mut halves = [0i16; 48];
    let mut bytes = [0u8; 48];
    let mut x = seed;
    let mut i = 0;
    while i < 48 {
        x = x.wrapping_mul(1103515245).wrapping_add(12345);
        table[i] = (x >> 8) & 0x3FF;
        halves[i] = (x & 0xFFFF) as i16;
        bytes[i] = (x >> 16) as u8;
        i += 1;
    }
    unsafe {
        sort_i32(table.as_mut_ptr(), 48);
        let found = bsearch_i32(table.as_ptr(), 48, table[20]);
        let sum = checksum(bytes.as_ptr(), halves.as_ptr(), 48);
        let mut acc: u64 = 1;
        let mut k = 0;
        while k < 8 {
            acc = acc64(acc, table[k] as u32, k as u32 * 5);
            k += 1;
        }
        found + sum + (acc as i32) + (acc >> 32) as i32
    }
}

#[unsafe(no_mangle)]
pub static mut SCRIPTS_TO_RUN: i32 = 3;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    unsafe {
        setup_symbols();
        let n = core::ptr::read_volatile(&raw const SCRIPTS_TO_RUN);
        let mut total = run_first();
        let mut i = 1;
        while i < n as u32 {
            RESULTS[(i as usize) & 7] = run_script(i);
            total += RESULTS[(i as usize) & 7];
            i += 1;
        }
        if !VM_ERROR.is_null() {
            total = -total;
        }
        total + table_check(total) + sym_get(b"gold\0".as_ptr()) + adjust_all(total & 7)
    }
}

// --- A family of near-identical functions, for `similar_functions` ------------

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn adjust_gold(delta: i32) -> i32 {
    unsafe {
        let mut v = sym_get(b"gold\0".as_ptr()) + delta;
        if v < 0 {
            v = 0;
        }
        if v > 9999 {
            v = 9999;
        }
        sym_set(b"gold\0".as_ptr(), v);
        v
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn adjust_hp(delta: i32) -> i32 {
    unsafe {
        let mut v = sym_get(b"hp\0".as_ptr()) + delta;
        if v < 0 {
            v = 0;
        }
        if v > 999 {
            v = 999;
        }
        sym_set(b"hp\0".as_ptr(), v);
        v
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn adjust_level(delta: i32) -> i32 {
    unsafe {
        let mut v = sym_get(b"level\0".as_ptr()) + delta;
        if v < 1 {
            v = 1;
        }
        if v > 99 {
            v = 99;
        }
        sym_set(b"level\0".as_ptr(), v);
        v
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn adjust_torches(delta: i32) -> i32 {
    unsafe {
        let mut v = sym_get(b"torches\0".as_ptr()) + delta;
        if v < 0 {
            v = 0;
        }
        if v > 12 {
            v = 12;
        }
        sym_set(b"torches\0".as_ptr(), v);
        v
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn adjust_all(delta: i32) -> i32 {
    adjust_gold(delta) + adjust_hp(delta) + adjust_level(delta) + adjust_torches(delta)
}
