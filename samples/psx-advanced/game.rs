//! The "game" object of the advanced sample: spawning, a recursive tree
//! walk, a frame loop, and `main`. It calls the engine object's functions.
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

unsafe extern "C" {
    static mut WORLD: World;
    fn fx_mul(a: i32, b: i32) -> i32;
    fn log_event(text: *const u8);
    fn world_command(op: u32, a: i32, b: i32) -> i32;
    fn entity_update(e: &mut Entity);
    fn pool_alloc(size: u32) -> *mut u8;
}

static KIND_NAMES: [&[u8]; 4] = [b"none\0", b"slime\0", b"bat\0", b"goblin\0"];

#[repr(C)]
pub struct Node {
    pub value: i32,
    pub left: *const Node,
    pub right: *const Node,
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn spawn(kind: u8, x: i32, y: i32) -> i32 {
    unsafe {
        let i = WORLD.count as usize;
        if i >= 8 {
            log_event(b"no free entity slot\0".as_ptr());
            return -1;
        }
        let e = &mut WORLD.entities[i];
        e.x = x << 16;
        e.y = y << 16;
        e.vx = fx_mul(1 << 16, 3 << 14);
        e.vy = 0;
        e.hp = 10;
        e.kind = kind;
        e.state = 1;
        e.name = KIND_NAMES[(kind & 3) as usize].as_ptr();
        WORLD.count += 1;
        log_event(b"spawned\0".as_ptr());
        i as i32
    }
}

/// Sums a tree, recursively.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn tree_sum(n: *const Node) -> i32 {
    if n.is_null() {
        return 0;
    }
    unsafe { (*n).value + tree_sum((*n).left) + tree_sum((*n).right) }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn build_tree(depth: u32, value: i32) -> *const Node {
    if depth == 0 {
        return core::ptr::null();
    }
    unsafe {
        let n = pool_alloc(core::mem::size_of::<Node>() as u32) as *mut Node;
        if n.is_null() {
            return core::ptr::null();
        }
        (*n).value = value;
        (*n).left = build_tree(depth - 1, value * 2);
        (*n).right = build_tree(depth - 1, value * 2 + 1);
        n
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn frame() -> i32 {
    unsafe {
        world_command(0, 0, 0);
        let mut i = 0;
        while i < WORLD.count as usize && i < 8 {
            entity_update(&mut WORLD.entities[i]);
            if WORLD.entities[i].hp < 5 {
                world_command(5, i as i32, 0);
            }
            i += 1;
        }
        world_command(7, 0, 0)
    }
}

#[unsafe(no_mangle)]
pub static mut FRAMES_TO_RUN: i32 = 3;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    unsafe {
        log_event(b"game start\0".as_ptr());
        spawn(1, 10, 20);
        spawn(2, 30, 40);
        spawn(3, 50, 60);
        let tree = build_tree(3, 1);
        let mut total = tree_sum(tree);
        let mut n = core::ptr::read_volatile(&raw const FRAMES_TO_RUN);
        while n > 0 {
            total += frame();
            n -= 1;
        }
        total
    }
}
