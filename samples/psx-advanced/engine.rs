//! The "engine" object of the advanced sample: fixed-point maths, a byte
//! copy, a memory pool, a table of handlers reached only through pointers,
//! logging with strings, and a dispatcher with a real jump table.
#![no_std]
#![allow(dead_code, static_mut_refs)]

#[repr(C)]
pub struct Entity {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub hp: i16,
    pub kind: u8,
    pub state: u8,
    pub name: *const u8,
}

#[repr(C)]
pub struct World {
    pub tick: u32,
    pub count: u32,
    pub entities: [Entity; 8],
    pub log_count: u32,
}

#[unsafe(no_mangle)]
pub static mut WORLD: World = World {
    tick: 0,
    count: 0,
    entities: [const {
        Entity { x: 0, y: 0, vx: 0, vy: 0, hp: 0, kind: 0, state: 0, name: core::ptr::null() }
    }; 8],
    log_count: 0,
};

static mut POOL: [u8; 4096] = [0; 4096];
static mut POOL_USED: u32 = 0;
#[unsafe(no_mangle)]
pub static mut LAST_LOG: *const u8 = core::ptr::null();

/// 16.16 fixed-point multiply.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn fx_mul(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> 16) as i32
}

/// 16.16 fixed-point divide by long division (the R3000 has no 64-bit
/// divide), guarding against zero.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn fx_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        return if a < 0 { i32::MIN } else { i32::MAX };
    }
    let negative = (a < 0) != (b < 0);
    let (mut rem, den) = (a.unsigned_abs(), b.unsigned_abs());
    let mut quotient: u32 = rem / den;
    rem %= den;
    let mut bits = 16;
    while bits > 0 {
        rem <<= 1;
        quotient <<= 1;
        if rem >= den {
            rem -= den;
            quotient |= 1;
        }
        bits -= 1;
    }
    if negative { (quotient as i32).wrapping_neg() } else { quotient as i32 }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn mem_copy(dst: *mut u8, src: *const u8, n: u32) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe { *dst.add(i as usize) = *src.add(i as usize) };
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn pool_alloc(size: u32) -> *mut u8 {
    unsafe {
        let size = (size + 3) & !3;
        if POOL_USED + size > POOL.len() as u32 {
            log_event(b"pool exhausted\0".as_ptr());
            return core::ptr::null_mut();
        }
        let p = POOL.as_mut_ptr().add(POOL_USED as usize);
        POOL_USED += size;
        p
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn log_event(text: *const u8) {
    unsafe {
        LAST_LOG = text;
        WORLD.log_count += 1;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn str_len(s: *const u8) -> u32 {
    let mut n = 0;
    // (volatile, so that the compiler doesn't replace the loop with a call to strlen)
    while unsafe { core::ptr::read_volatile(s.add(n as usize)) } != 0 {
        n += 1;
    }
    n
}

// --- Handlers reached only through the table below ---------------------------

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn on_idle(e: &mut Entity) {
    e.vx = 0;
    e.vy = 0;
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn on_walk(e: &mut Entity) {
    e.x += e.vx;
    e.y += e.vy;
    if e.x > 320 << 16 {
        e.x = 0;
        log_event(b"wrapped around\0".as_ptr());
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn on_hurt(e: &mut Entity) {
    e.hp -= 3;
    if e.hp <= 0 {
        e.state = 3;
        log_event(b"entity died\0".as_ptr());
    } else {
        e.state = 1;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn on_dead(e: &mut Entity) {
    e.kind = 0;
}

static HANDLERS: [extern "C" fn(&mut Entity); 4] = [on_idle, on_walk, on_hurt, on_dead];

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn entity_update(e: &mut Entity) {
    let h = HANDLERS[(e.state & 3) as usize];
    h(e);
}

// --- A dispatcher with a jump table ------------------------------------------

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn world_command(op: u32, a: i32, b: i32) -> i32 {
    unsafe {
        match op {
            0 => {
                WORLD.tick = WORLD.tick.wrapping_add(1);
                WORLD.tick as i32
            }
            1 => fx_mul(a, b),
            2 => fx_div(a, b),
            3 => {
                let p = pool_alloc(a as u32);
                p as i32
            }
            4 => {
                log_event(b"command four\0".as_ptr());
                str_len(b"command four\0".as_ptr()) as i32
            }
            5 => {
                let i = (a as usize) & 7;
                entity_update(&mut WORLD.entities[i]);
                WORLD.entities[i].hp as i32
            }
            6 => {
                let n = WORLD.count;
                let mut i = 0;
                while i < n as usize && i < 8 {
                    entity_update(&mut WORLD.entities[i]);
                    i += 1;
                }
                n as i32
            }
            7 => {
                let mut sum = 0i32;
                let mut i = 0;
                while i < 8 {
                    sum += WORLD.entities[i].hp as i32;
                    i += 1;
                }
                sum
            }
            _ => {
                log_event(b"bad command\0".as_ptr());
                -1
            }
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
