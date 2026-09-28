// The shared library the `imports` fixture links against (ELF): just the
// symbols it imports.
#![no_std]
#![crate_type = "cdylib"]

#[unsafe(no_mangle)]
pub extern "C" fn puts(_s: *const u8) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn strlen(_s: *const u8) -> usize {
    0
}

#[unsafe(no_mangle)]
pub static errno_value: i32 = 0;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
