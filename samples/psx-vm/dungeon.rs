//! The "dungeon" overlay, loaded at the same address as the town one. This
//! is the one the memory image has loaded.
#![no_std]
#![allow(dead_code, static_mut_refs)]

unsafe extern "C" {
    fn compile(text: *const u8) -> u32;
    fn vm_reset(code: *const u8, len: u32);
    fn vm_run(max_steps: u32) -> i32;
    fn sym_set(name: *const u8, value: i32) -> i32;
    fn vm_fail(what: *const u8);
    static mut EMIT: [u8; 264];
}

static mut DEPTH: i32 = 0;
static mut TORCHES: i32 = 3;

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn dungeon_descend() -> i32 {
    unsafe {
        DEPTH += 1;
        if TORCHES == 0 {
            vm_fail(b"it is too dark to go on\0".as_ptr());
            return -1;
        }
        TORCHES -= 1;
        sym_set(b"depth\0".as_ptr(), DEPTH);
        if compile(b"depth * depth + hp\0".as_ptr()) != 0 {
            return -1;
        }
        vm_reset(EMIT.as_ptr(), *(EMIT.as_ptr().add(256) as *const u32));
        vm_run(100)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dungeon_main() -> i32 {
    let mut t = 0;
    let mut i = 0;
    while i < 4 {
        t += dungeon_descend();
        i += 1;
    }
    t
}
