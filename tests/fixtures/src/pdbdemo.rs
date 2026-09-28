// A small Windows program with its debug info in a PDB, the way MSVC builds
// keep it: built for x86_64-pc-windows-msvc without the C runtime, linked by
// rust-lld (as lld-link), so that no Visual Studio is needed.
//
//   rustc --target x86_64-pc-windows-msvc -g -C opt-level=0 -C panic=abort \
//     -C linker=rust-lld -C linker-flavor=lld-link \
//     -C link-arg=-NODEFAULTLIB -C link-arg=-ENTRY:start -C link-arg=-SUBSYSTEM:CONSOLE \
//     pdbdemo.rs -o pdbdemo.exe
//
// (scripts/build-fixtures.sh does this.)
#![no_std]
#![no_main]

use core::ffi::c_void;
use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

// What core expects of the C runtime, which isn't linked.
#[unsafe(no_mangle)]
pub static _fltused: i32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn __CxxFrameHandler3() {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let (d, s) = (dst as *mut u8, src as *const u8);
    let mut i = 0;
    while i < n {
        unsafe { *d.add(i) = *s.add(i) };
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    if (dst as usize) < (src as usize) {
        return unsafe { memcpy(dst, src, n) };
    }
    let (d, s) = (dst as *mut u8, src as *const u8);
    let mut i = n;
    while i > 0 {
        i -= 1;
        unsafe { *d.add(i) = *s.add(i) };
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut c_void, c: i32, n: usize) -> *mut c_void {
    let d = dst as *mut u8;
    let mut i = 0;
    while i < n {
        unsafe { *d.add(i) = c as u8 };
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32 {
    let (a, b) = (a as *const u8, b as *const u8);
    let mut i = 0;
    while i < n {
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x != y {
            return x as i32 - y as i32;
        }
        i += 1;
    }
    0
}

/// A counter the program keeps.
#[unsafe(no_mangle)]
pub static mut FRAMES: u32 = 0;

#[derive(Clone, Copy)]
pub struct Shape {
    pub w: u32,
    pub h: u32,
}

#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn area(s: &Shape) -> u32 {
    s.w * s.h
}

#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn total_area(shapes: &[Shape; 3]) -> u32 {
    let mut total = 0;
    for s in shapes {
        total += area(s);
    }
    total
}

#[inline(never)]
fn tick() {
    unsafe {
        FRAMES += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn start() -> u32 {
    let shapes = [Shape { w: 2, h: 3 }, Shape { w: 4, h: 5 }, Shape { w: 6, h: 7 }];
    tick();
    total_area(&shapes)
}
