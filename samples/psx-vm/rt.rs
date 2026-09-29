//! The runtime object: what a C library would give a game (memcpy, memset,
//! string routines, number formatting). In a real project these are the
//! SDK's or the compiler's, marked as library code rather than decompiled.
#![no_std]
#![allow(dead_code, static_mut_refs)]

#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe { core::ptr::write_volatile(dst.add(i), core::ptr::read_volatile(src.add(i))) };
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn memset(dst: *mut u8, value: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe { core::ptr::write_volatile(dst.add(i), value as u8) };
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn rt_strlen(s: *const u8) -> u32 {
    let mut n = 0;
    while unsafe { core::ptr::read_volatile(s.add(n as usize)) } != 0 {
        n += 1;
    }
    n
}

#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn rt_streq(a: *const u8, b: *const u8) -> i32 {
    let mut i = 0;
    loop {
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x != y {
            return 0;
        }
        if x == 0 {
            return 1;
        }
        i += 1;
    }
}

/// FNV-1a over a C string.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn rt_hash(s: *const u8) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    let mut i = 0;
    loop {
        let c = unsafe { *s.add(i) };
        if c == 0 {
            return h;
        }
        h = (h ^ c as u32).wrapping_mul(0x0100_0193);
        i += 1;
    }
}

/// Writes `v` in decimal into `out` (at least 12 bytes), returns the length.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn rt_itoa(v: i32, out: *mut u8) -> u32 {
    let mut n: u32 = 0;
    let mut u = v.unsigned_abs();
    let mut tmp = [0u8; 16];
    let mut k = 0;
    loop {
        tmp[k & 15] = b'0' + (u % 10) as u8;
        k += 1;
        u /= 10;
        if u == 0 {
            break;
        }
    }
    if v < 0 {
        unsafe { *out.add(n as usize) = b'-' };
        n += 1;
    }
    while k > 0 {
        k -= 1;
        unsafe { *out.add(n as usize) = tmp[k & 15] };
        n += 1;
    }
    unsafe { *out.add(n as usize) = 0 };
    n
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
