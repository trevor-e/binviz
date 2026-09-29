//! The "town" overlay, loaded at 0x80100000 like the dungeon one: the two
//! share an address, as a game's field overlays do.
#![no_std]
#![allow(dead_code, static_mut_refs)]

unsafe extern "C" {
    fn compile(text: *const u8) -> u32;
    fn vm_reset(code: *const u8, len: u32);
    fn vm_run(max_steps: u32) -> i32;
    fn sym_set(name: *const u8, value: i32) -> i32;
    static mut EMIT: [u8; 264];
}

static mut VISITS: u32 = 0;

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn town_enter() -> i32 {
    unsafe {
        VISITS += 1;
        sym_set(b"shop_open\0".as_ptr(), (VISITS & 1) as i32);
        if compile(b"gold - 30 * shop_open\0".as_ptr()) != 0 {
            return -1;
        }
        vm_reset(EMIT.as_ptr(), *(EMIT.as_ptr().add(256) as *const u32));
        vm_run(100)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn town_main() -> i32 {
    town_enter() + town_enter()
}
