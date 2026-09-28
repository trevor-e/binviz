// Freestanding test program: no std, no libc, so it links for any target
// with only rust-lld. Produces small binaries that still carry full DWARF.
#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[derive(Clone, Copy)]
pub struct Vec2 {
    pub x: i32,
    pub y: i32,
}

#[inline(always)]
fn dot(a: Vec2, b: Vec2) -> i32 {
    a.x * b.x + a.y * b.y
}

#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn checksum(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[inline(never)]
fn fib(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a.wrapping_add(b);
        a = b;
        b = next;
    }
    a
}

pub static MESSAGE: &[u8] = b"binviz tiny fixture";
pub static mut STATE: u64 = 0;
static POINTS: [Vec2; 3] = [Vec2 { x: 1, y: 2 }, Vec2 { x: 3, y: 4 }, Vec2 { x: -5, y: 6 }];

#[inline(never)]
fn run() -> u64 {
    let mut acc = checksum(MESSAGE) as u64;
    for pair in POINTS.windows(2) {
        acc = acc.wrapping_add(dot(pair[0], pair[1]) as u64);
    }
    acc.wrapping_add(fib(40))
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe {
        STATE = run();
    }
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    run() as i32
}

// MinGW targets call GCC's static-constructor hook from `main`.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "C" fn __main() {}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
