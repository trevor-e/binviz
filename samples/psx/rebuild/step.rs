//! A rebuild of `entity_step` to score against the original: the two
//! multiplications are written the other way round, and `wrap` is a global.
//! `python build.py --rebuild rebuild/step.rs`, then
//! `binviz --notes build/sample.notes.json match build/sample.exe build/step.o entity_step`.
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
static mut WRAP: i32 = 100;

unsafe extern "C" {
    fn entity_speed(kind: u8) -> i32;
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn entity_step(e: &mut Entity, dx: i32, dy: i32, ticks: i32) -> i32 {
    let speed = unsafe { entity_speed(e.kind) };
    e.x += ticks * speed * dx;
    e.y += ticks * speed * dy;
    let wrap = unsafe { WRAP };
    if e.x > wrap {
        e.x -= wrap;
    }
    e.hp -= 1;
    unsafe {
        FRAME = FRAME.wrapping_add(1);
    }
    e.hp as i32
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
