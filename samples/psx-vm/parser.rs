//! A tiny expression compiler for the VM: a tokenizer with a sparse
//! character switch, a recursive-descent parser (mutually recursive
//! functions), and a code emitter. Also a couple of routines returning a
//! structure by value, sorting, and a binary search.
#![no_std]
#![allow(dead_code, static_mut_refs)]

unsafe extern "C" {
    fn sym_set(name: *const u8, value: i32) -> i32;
    fn sym_find(name: *const u8) -> i32;
    fn vm_fail(what: *const u8);
    fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8;
}

#[repr(C)]
pub struct Token {
    pub kind: u8,
    pub len: u8,
    pub value: i16,
    pub start: u32,
}

#[repr(C)]
pub struct Emitter {
    pub code: [u8; 256],
    pub len: u32,
    pub errors: u32,
}

#[unsafe(no_mangle)]
pub static mut EMIT: Emitter = Emitter {
    code: [0; 256],
    len: 0,
    errors: 0,
};

static mut SRC: *const u8 = core::ptr::null();
static mut POS: u32 = 0;
static mut CUR: Token = Token {
    kind: 0,
    len: 0,
    value: 0,
    start: 0,
};

pub const T_END: u8 = 0;
pub const T_NUM: u8 = 1;
pub const T_IDENT: u8 = 2;
pub const T_OP: u8 = 3;
pub const T_LPAREN: u8 = 4;
pub const T_RPAREN: u8 = 5;
pub const T_BAD: u8 = 6;

/// Returns a token by value (through a hidden pointer, the first argument).
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn make_token(kind: u8, len: u8, value: i16, start: u32) -> Token {
    Token {
        kind,
        len,
        value,
        start,
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn is_digit(c: u8) -> i32 {
    (c >= b'0' && c <= b'9') as i32
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn is_ident(c: u8) -> i32 {
    ((c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z') || c == b'_') as i32
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn next_token() -> u8 {
    unsafe {
        let mut c = *SRC.add(POS as usize);
        while c == b' ' || c == b'\t' || c == b'\n' {
            POS += 1;
            c = *SRC.add(POS as usize);
        }
        let start = POS;
        let tok = match c {
            0 => make_token(T_END, 0, 0, start),
            b'(' => {
                POS += 1;
                make_token(T_LPAREN, 1, 0, start)
            }
            b')' => {
                POS += 1;
                make_token(T_RPAREN, 1, 0, start)
            }
            b'+' | b'-' | b'*' | b'/' | b'<' | b'=' | b'&' | b'|' => {
                POS += 1;
                make_token(T_OP, 1, c as i16, start)
            }
            b'0'..=b'9' => {
                let mut v: i32 = 0;
                while is_digit(*SRC.add(POS as usize)) != 0 {
                    v = v * 10 + (*SRC.add(POS as usize) - b'0') as i32;
                    POS += 1;
                }
                make_token(T_NUM, (POS - start) as u8, v as i16, start)
            }
            _ if is_ident(c) != 0 => {
                while is_ident(*SRC.add(POS as usize)) != 0 || is_digit(*SRC.add(POS as usize)) != 0 {
                    POS += 1;
                }
                make_token(T_IDENT, (POS - start) as u8, 0, start)
            }
            _ => {
                POS += 1;
                make_token(T_BAD, 1, c as i16, start)
            }
        };
        CUR = tok;
        CUR.kind
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn emit(b: u8) {
    unsafe {
        if (EMIT.len as usize) < EMIT.code.len() {
            EMIT.code[EMIT.len as usize] = b;
            EMIT.len += 1;
        } else {
            EMIT.errors += 1;
            vm_fail(b"program too long\0".as_ptr());
        }
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn emit_push(v: i16) {
    emit(1);
    emit((v & 0xFF) as u8);
    emit(((v >> 8) & 0xFF) as u8);
}

/// A copy of the identifier under the cursor, NUL-terminated, in `out` (16 bytes).
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn ident_text(out: *mut u8) {
    unsafe {
        let n = (CUR.len as usize).min(15);
        memcpy(out, SRC.add(CUR.start as usize), n);
        *out.add(n) = 0;
    }
}

// primary := NUM | IDENT | '(' expr ')'
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn parse_primary() {
    unsafe {
        match CUR.kind {
            T_NUM => {
                emit_push(CUR.value);
                next_token();
            }
            T_IDENT => {
                let mut name = [0u8; 16];
                ident_text(name.as_mut_ptr());
                let i = sym_find(name.as_ptr());
                if i < 0 {
                    EMIT.errors += 1;
                    vm_fail(b"unknown name in expression\0".as_ptr());
                } else {
                    emit(15);
                    emit(i as u8);
                }
                next_token();
            }
            T_LPAREN => {
                next_token();
                parse_expr();
                if CUR.kind != T_RPAREN {
                    EMIT.errors += 1;
                    vm_fail(b"expected )\0".as_ptr());
                }
                next_token();
            }
            _ => {
                EMIT.errors += 1;
                vm_fail(b"expected a value\0".as_ptr());
                next_token();
            }
        }
    }
}

// term := primary (('*' | '/') primary)*
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn parse_term() {
    parse_primary();
    unsafe {
        while CUR.kind == T_OP && (CUR.value == b'*' as i16 || CUR.value == b'/' as i16) {
            let op = CUR.value;
            next_token();
            parse_primary();
            emit(if op == b'*' as i16 { 4 } else { 5 });
        }
    }
}

// expr := term (('+' | '-' | '<' | '=' | '&' | '|') term)*
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn parse_expr() {
    parse_term();
    unsafe {
        while CUR.kind == T_OP && CUR.value != b'*' as i16 && CUR.value != b'/' as i16 {
            let op = CUR.value as u8;
            next_token();
            parse_term();
            let code = match op {
                b'+' => 2,
                b'-' => 3,
                b'<' => 6,
                b'=' => 7,
                b'&' => 17,
                b'|' => 18,
                _ => 0,
            };
            emit(code);
        }
    }
}

/// Compiles `text` into EMIT; returns the errors.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn compile(text: *const u8) -> u32 {
    unsafe {
        SRC = text;
        POS = 0;
        EMIT.len = 0;
        EMIT.errors = 0;
        next_token();
        parse_expr();
        if CUR.kind != T_END {
            EMIT.errors += 1;
            vm_fail(b"trailing input\0".as_ptr());
        }
        emit(16);
        emit(0);
        emit(0);
        EMIT.errors
    }
}

// --- Odds and ends ------------------------------------------------------------

#[repr(C)]
pub struct Range {
    pub lo: i32,
    pub hi: i32,
    pub count: i32,
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn range_of(values: *const i32, n: u32) -> Range {
    let mut r = Range {
        lo: i32::MAX,
        hi: i32::MIN,
        count: n as i32,
    };
    let mut i = 0;
    while i < n as usize {
        let v = unsafe { *values.add(i) };
        if v < r.lo {
            r.lo = v;
        }
        if v > r.hi {
            r.hi = v;
        }
        i += 1;
    }
    r
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn sort_i32(values: *mut i32, n: u32) {
    let mut i = 1;
    while i < n as usize {
        let v = unsafe { *values.add(i) };
        let mut j = i;
        while j > 0 && unsafe { *values.add(j - 1) } > v {
            unsafe { *values.add(j) = *values.add(j - 1) };
            j -= 1;
        }
        unsafe { *values.add(j) = v };
        i += 1;
    }
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn bsearch_i32(values: *const i32, n: u32, want: i32) -> i32 {
    let (mut lo, mut hi) = (0i32, n as i32 - 1);
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let v = unsafe { *values.add(mid as usize) };
        if v == want {
            return mid;
        }
        if v < want {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    -1
}

/// Sums signed and unsigned narrow values: sign extension the compiler must get right.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn checksum(bytes: *const u8, halves: *const i16, n: u32) -> i32 {
    let mut s: i32 = 0;
    let mut i = 0;
    while i < n as usize {
        s = s.wrapping_add(unsafe { *bytes.add(i) } as i32);
        s = s.wrapping_add(unsafe { *halves.add(i) } as i32);
        s = s.wrapping_add(unsafe { *(bytes.add(i) as *const i8) } as i32);
        i += 1;
    }
    s
}

/// 64-bit accumulate in a 32-bit world: register pairs.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn acc64(acc: u64, v: u32, shift: u32) -> u64 {
    acc.wrapping_add((v as u64) << (shift & 63)).wrapping_mul(3)
}
