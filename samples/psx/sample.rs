//! The same program as sample.c, in Rust, for machines without a MIPS C
//! compiler: rustc emits LLVM IR (through the wasm32 target, which has
//! 32-bit pointers) and the llc that ships with Rust's llvm-tools compiles
//! it for the MIPS I. See build.py.
#![no_std]
#![allow(dead_code, static_mut_refs)]

#[repr(C)]
pub struct Entity {
    pub x: i32,
    pub y: i32,
    pub hp: i16,
    pub kind: u8,
    pub flags: u8,
    pub name: *const u8,
}

static mut FRAME: u32 = 0;
static NAMES: [&[u8]; 3] = [b"slime\0", b"bat\0", b"goblin\0"];
#[unsafe(no_mangle)]
pub static mut sink: i32 = 0;

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn entity_speed(kind: u8) -> i32 {
    match kind {
        0 => 1,
        1 => 6,
        2 => 3,
        3 => 9,
        4 => 2,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn entity_step(e: &mut Entity, dx: i32, dy: i32, ticks: i32, wrap: i32) -> i32 {
    let speed = entity_speed(e.kind);
    e.x += dx * speed * ticks;
    e.y += dy * speed * ticks;
    if e.x > wrap {
        e.x -= wrap;
    }
    e.hp -= 1;
    unsafe {
        FRAME = FRAME.wrapping_add(1);
    }
    e.hp as i32
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn entity_name(kind: u8) -> *const u8 {
    if (kind as usize) < NAMES.len() {
        NAMES[kind as usize].as_ptr()
    } else {
        b"unknown\0".as_ptr()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let mut e = Entity {
        x: 0,
        y: 0,
        hp: 10,
        kind: 2,
        flags: 0,
        name: entity_name(2),
    };
    let mut total = 0;
    let mut i = 0;
    while i < unsafe { core::ptr::read_volatile(&raw const sink) } {
        total += entity_step(&mut e, 1, -1, i, 100);
        i += 1;
    }
    total
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
