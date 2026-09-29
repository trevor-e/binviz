//! An overlay: code the game loads from the disc at 0x80100000 for one
//! field, calling the boot executable's functions at their fixed
//! addresses. Linked against the main executable's symbols.
#![no_std]
#![allow(dead_code, static_mut_refs)]

unsafe extern "C" {
    fn log_event(text: *const u8);
    fn spawn(kind: u8, x: i32, y: i32) -> i32;
    fn world_command(op: u32, a: i32, b: i32) -> i32;
    fn fx_div(a: i32, b: i32) -> i32;
}

static mut FIELD_TIMER: i32 = 0;

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn field_enter() -> i32 {
    unsafe {
        log_event(b"entering the forest\0".as_ptr());
        FIELD_TIMER = 0;
        spawn(3, 100, 100) + spawn(1, 120, 100)
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn field_tick(dt: i32) -> i32 {
    unsafe {
        FIELD_TIMER += dt;
        if FIELD_TIMER > 60 {
            log_event(b"forest ambush\0".as_ptr());
            FIELD_TIMER = 0;
            return world_command(6, 0, 0);
        }
        fx_div(FIELD_TIMER << 16, 60 << 16)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn field_main() -> i32 {
    let mut t = field_enter();
    let mut i = 0;
    while i < 70 {
        t += field_tick(1);
        i += 1;
    }
    t
}
