//! A bytecode virtual machine: the kind of script interpreter a field
//! system runs. A dense opcode switch (a big jump table), a stack, a table
//! of syscall handlers reached through pointers, and a symbol table with a
//! hash.
#![no_std]
#![allow(dead_code, static_mut_refs)]

unsafe extern "C" {
    fn rt_hash(s: *const u8) -> u32;
    fn rt_streq(a: *const u8, b: *const u8) -> i32;
    fn rt_itoa(v: i32, out: *mut u8) -> u32;
    fn memset(dst: *mut u8, value: i32, n: usize) -> *mut u8;
}

pub const STACK: usize = 64;
pub const SYMBOLS: usize = 32;

#[repr(C)]
pub struct Symbol {
    pub name: *const u8,
    pub hash: u32,
    pub value: i32,
}

#[repr(C)]
pub struct Vm {
    pub pc: u32,
    pub sp: u32,
    pub flags: u8,
    pub halted: u8,
    pub steps: u16,
    pub code: *const u8,
    pub code_len: u32,
    pub stack: [i32; STACK],
    pub symbols: [Symbol; SYMBOLS],
    pub symbol_count: u32,
    pub out: [u8; 64],
    pub out_len: u32,
}

#[unsafe(no_mangle)]
pub static mut VM: Vm = Vm {
    pc: 0,
    sp: 0,
    flags: 0,
    halted: 0,
    steps: 0,
    code: core::ptr::null(),
    code_len: 0,
    stack: [0; STACK],
    symbols: [const { Symbol { name: core::ptr::null(), hash: 0, value: 0 } }; SYMBOLS],
    symbol_count: 0,
    out: [0; 64],
    out_len: 0,
};

#[unsafe(no_mangle)]
pub static mut VM_ERROR: *const u8 = core::ptr::null();

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_fail(what: *const u8) {
    unsafe {
        VM_ERROR = what;
        VM.halted = 1;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_reset(code: *const u8, len: u32) {
    unsafe {
        memset(&raw mut VM as *mut u8, 0, core::mem::size_of::<Vm>());
        VM.code = code;
        VM.code_len = len;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_push(v: i32) {
    unsafe {
        if VM.sp as usize >= STACK {
            vm_fail(b"stack overflow\0".as_ptr());
            return;
        }
        VM.stack[VM.sp as usize] = v;
        VM.sp += 1;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_pop() -> i32 {
    unsafe {
        if VM.sp == 0 {
            vm_fail(b"stack underflow\0".as_ptr());
            return 0;
        }
        VM.sp -= 1;
        VM.stack[(VM.sp as usize) & (STACK - 1)]
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_fetch() -> u32 {
    unsafe {
        if VM.pc >= VM.code_len {
            vm_fail(b"ran off the end of the code\0".as_ptr());
            return 0;
        }
        let b = *VM.code.add(VM.pc as usize);
        VM.pc += 1;
        b as u32
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_fetch16() -> i32 {
    let lo = vm_fetch();
    let hi = vm_fetch();
    ((hi << 8) | lo) as u16 as i16 as i32
}

// --- Symbols ---------------------------------------------------------------

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sym_find(name: *const u8) -> i32 {
    unsafe {
        let h = rt_hash(name);
        let mut i = 0;
        while i < VM.symbol_count as usize && i < SYMBOLS {
            let s = &VM.symbols[i];
            if s.hash == h && rt_streq(s.name, name) != 0 {
                return i as i32;
            }
            i += 1;
        }
        -1
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sym_set(name: *const u8, value: i32) -> i32 {
    unsafe {
        let i = sym_find(name);
        if i >= 0 {
            VM.symbols[(i as usize) & (SYMBOLS - 1)].value = value;
            return i;
        }
        if VM.symbol_count as usize >= SYMBOLS {
            vm_fail(b"symbol table full\0".as_ptr());
            return -1;
        }
        let k = VM.symbol_count as usize;
        VM.symbols[k] = Symbol {
            name,
            hash: rt_hash(name),
            value,
        };
        VM.symbol_count += 1;
        k as i32
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sym_get(name: *const u8) -> i32 {
    let i = sym_find(name);
    if i < 0 {
        vm_fail(b"unknown symbol\0".as_ptr());
        return 0;
    }
    unsafe { VM.symbols[(i as usize) & (SYMBOLS - 1)].value }
}

// --- Syscalls, through a table -----------------------------------------------

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sys_print_int() {
    let v = vm_pop();
    unsafe {
        if VM.out_len as usize + 12 < VM.out.len() {
            let n = rt_itoa(v, VM.out.as_mut_ptr().add(VM.out_len as usize));
            VM.out_len += n;
        }
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sys_newline() {
    unsafe {
        if (VM.out_len as usize) < VM.out.len() - 1 {
            VM.out[VM.out_len as usize] = b'\n';
            VM.out_len += 1;
        }
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sys_clear() {
    unsafe {
        VM.out_len = 0;
        VM.out[0] = 0;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sys_steps() {
    unsafe { vm_push(VM.steps as i32) }
}

static SYSCALLS: [extern "C" fn(); 4] = [sys_print_int, sys_newline, sys_clear, sys_steps];

// --- The interpreter ---------------------------------------------------------

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_step() {
    unsafe {
        if VM.halted != 0 {
            return;
        }
        VM.steps = VM.steps.wrapping_add(1);
        let op = vm_fetch();
        match op {
            0 => VM.halted = 1,
            1 => {
                let v = vm_fetch16();
                vm_push(v);
            }
            2 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a.wrapping_add(b));
            }
            3 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a.wrapping_sub(b));
            }
            4 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a.wrapping_mul(b));
            }
            5 => {
                let b = vm_pop();
                let a = vm_pop();
                if b == 0 {
                    vm_fail(b"division by zero\0".as_ptr());
                } else {
                    vm_push(a.wrapping_div(b));
                }
            }
            6 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push((a < b) as i32);
            }
            7 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push((a == b) as i32);
            }
            8 => {
                let a = vm_pop();
                vm_push((a == 0) as i32);
            }
            9 => {
                let target = vm_fetch16();
                VM.pc = target as u32;
            }
            10 => {
                let target = vm_fetch16();
                if vm_pop() != 0 {
                    VM.pc = target as u32;
                }
            }
            11 => {
                let v = vm_pop();
                vm_push(v);
                vm_push(v);
            }
            12 => {
                vm_pop();
            }
            13 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(b);
                vm_push(a);
            }
            14 => {
                let i = vm_fetch();
                let v = vm_pop();
                if (i as usize) < SYMBOLS && (i as usize) < VM.symbol_count as usize {
                    VM.symbols[i as usize].value = v;
                } else {
                    vm_fail(b"bad symbol index\0".as_ptr());
                }
            }
            15 => {
                let i = vm_fetch();
                if (i as usize) < SYMBOLS && (i as usize) < VM.symbol_count as usize {
                    vm_push(VM.symbols[i as usize].value);
                } else {
                    vm_fail(b"bad symbol index\0".as_ptr());
                }
            }
            16 => {
                let n = vm_fetch();
                if (n as usize) < SYSCALLS.len() {
                    SYSCALLS[n as usize]();
                } else {
                    vm_fail(b"bad syscall\0".as_ptr());
                }
            }
            17 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a & b);
            }
            18 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a | b);
            }
            19 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a << (b & 31));
            }
            20 => {
                let b = vm_pop();
                let a = vm_pop();
                vm_push(a >> (b & 31));
            }
            21 => {
                let a = vm_pop();
                vm_push(a.wrapping_neg());
            }
            22 => {
                let a = vm_pop();
                vm_push(if a < 0 { -a } else { a });
            }
            23 => {
                VM.flags ^= 1;
            }
            _ => vm_fail(b"bad opcode\0".as_ptr()),
        }
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vm_run(max_steps: u32) -> i32 {
    unsafe {
        let mut n = 0;
        while VM.halted == 0 && n < max_steps {
            vm_step();
            n += 1;
        }
        if VM.halted == 0 {
            vm_fail(b"step limit reached\0".as_ptr());
        }
        if VM.sp > 0 { VM.stack[(VM.sp - 1) as usize & (STACK - 1)] } else { 0 }
    }
}
