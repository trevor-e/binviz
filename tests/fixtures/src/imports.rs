// A program that imports functions and data from a shared library and keeps
// pointers in data, for testing cross-references: calls through PLT entries
// and Mach-O stubs, GOT slots, and pointers that the loader relocates
// (RELATIVE / RELR relocations, chained fixups).
#![no_std]
#![no_main]

unsafe extern "C" {
    fn puts(s: *const u8) -> i32;
    fn strlen(s: *const u8) -> usize;
    static errno_value: i32;
}

static MESSAGES: [&[u8]; 3] = [b"hello from binviz\0", b"goodbye\0", b"unused message\0"];

static HANDLERS: [extern "C" fn(usize) -> i32; 2] = [greet, farewell];

static mut COUNTER: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn greet(i: usize) -> i32 {
    unsafe {
        COUNTER += 1;
        puts(MESSAGES[i % 3].as_ptr())
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn farewell(i: usize) -> i32 {
    unsafe { strlen(MESSAGES[(i + 1) % 3].as_ptr()) as i32 + errno_value }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let mut total = 0;
    for (i, h) in HANDLERS.iter().enumerate() {
        total += h(i);
    }
    total + greet(2)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
