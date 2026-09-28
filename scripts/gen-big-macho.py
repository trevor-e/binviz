#!/usr/bin/env python3
"""Generates a large synthetic arm64 iOS Mach-O executable for performance tests.

It is shaped like a big app's main binary: millions of functions (listed in
LC_FUNCTION_STARTS), millions of C strings and Objective-C selectors, pointer
tables, data, an optional full symbol table (Objective-C, Swift- and C++-style
names), and an ad-hoc code signature. The code is real arm64 (prologue, body,
a call to the next function, epilogue) so disassembly works.

    python scripts/gen-big-macho.py out.bin --functions 2000000 [--strip]

Defaults produce a file of about 1.1 GB.
"""

import argparse
import struct
import uuid

PAGE = 0x4000
BASE = 0x1_0000_0000

# Function sizes (bytes), cycled with a fixed stride so the mix looks irregular.
SIZES = [20, 32, 48, 64, 96, 128, 160, 192, 256, 320, 384, 512, 640, 768, 1024, 64, 48, 32, 96, 128]
BODY = [0x91000400, 0xAA0203E1, 0xF9400001, 0x8B020020, 0xD1000400, 0xB9400002, 0x2A0103E0, 0x9B027C20]
PROLOGUE = [0xA9BF7BFD, 0x910003FD]  # stp x29, x30, [sp, #-16]! ; mov x29, sp
EPILOGUE = [0xA8C17BFD, 0xD65F03C0]  # ldp x29, x30, [sp], #16 ; ret


def align(n, a):
    return (n + a - 1) // a * a


def uleb(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--functions", type=int, default=2_000_000)
    ap.add_argument("--strings", type=int, default=3_000_000)
    ap.add_argument("--selectors", type=int, default=1_000_000)
    ap.add_argument("--strip", action="store_true", help="keep only a couple of symbols, like an App Store build")
    args = ap.parse_args()
    n = args.functions

    # --- __text ---------------------------------------------------------------
    templates = {}
    for size in set(SIZES):
        words = PROLOGUE + [BODY[i % len(BODY)] for i in range(size // 4 - 5)] + [0x94000000] + EPILOGUE
        templates[size] = bytearray(struct.pack(f"<{len(words)}I", *words))
    starts = []
    text = bytearray()
    for i in range(n):
        starts.append(len(text))
        text += templates[SIZES[(i * 7) % len(SIZES)]]
    text_off = PAGE  # header and load commands live in the first page
    text_addr = BASE + text_off
    # Each function calls the next one: patch the `bl` before its epilogue.
    for i in range(n):
        size = SIZES[(i * 7) % len(SIZES)]
        bl = starts[i] + size - 12
        target = starts[i + 1] if i + 1 < n else starts[0]
        imm = ((target - bl) >> 2) & 0x3FFFFFF
        struct.pack_into("<I", text, bl, 0x94000000 | imm)

    # --- strings ----------------------------------------------------------------
    parts = []
    for i in range(args.strings):
        k = i % 6
        if k == 0:
            parts.append(f"error: request {i} failed with status %d")
        elif k == 1:
            parts.append(f"/Users/build/App/Sources/Module{i % 997}/Feature{i % 131}View.swift")
        elif k == 2:
            parts.append(f"https://api.example.com/v{i % 7}/items/{i}")
        elif k == 3:
            parts.append(f"com.example.app.setting.{i}")
        elif k == 4:
            parts.append(f"Loaded %@ in %.2f ms (cache {i})")
        else:
            parts.append(f"FeatureFlag_{i}_enabled")
    cstring = ("\0".join(parts) + "\0").encode()
    parts = [f"initWithFrame{i}:options:completion:" if i % 3 == 0 else f"didSelectItem{i}AtIndexPath:" for i in range(args.selectors)]
    methname = ("\0".join(parts) + "\0").encode()
    del parts

    cstring_off = align(text_off + len(text), 16)
    methname_off = cstring_off + len(cstring)
    text_seg_size = align(methname_off + len(methname), PAGE)

    # --- __DATA_CONST,__const: a pointer per 4th function ------------------------
    ptrs = bytearray(struct.pack(f"<{n // 4}Q", *(text_addr + starts[i] for i in range(0, n - n % 4, 4))))
    const_off = text_seg_size
    const_seg_size = align(len(ptrs), PAGE)

    # --- __DATA: data and bss ------------------------------------------------------
    data = bytearray()
    for i in range(16 * 1024 * 1024 // 32):
        data += struct.pack("<QQQQ", i, text_addr + starts[i % n], 0, 0x3FF0000000000000)
    data_off = const_off + const_seg_size
    data_seg_file = align(len(data), PAGE)
    bss_size = 4 * 1024 * 1024

    # --- __LINKEDIT: function starts, symbols, strings, signature -------------
    linkedit_off = data_off + data_seg_file
    fstarts = bytearray()
    prev = BASE
    for s in starts:
        fstarts += uleb(text_addr + s - prev)
        prev = text_addr + s
    fstarts += b"\0"
    fstarts_off = linkedit_off
    symoff = align(fstarts_off + len(fstarts), 8)
    names = [b"__mh_execute_header", b"_main"]
    values = [BASE, text_addr]
    sects = [1, 1]
    if not args.strip:
        for i in range(n):
            k = i % 10
            if k < 5:
                names.append(f"-[BVZFeature{i % 4099}Controller handleEvent{i}:withContext:]".encode())
            elif k < 8:
                mod = f"Module{i % 211}"
                cls = f"Type{i % 3001}"
                fn = f"method{i}"
                names.append(f"_$s{len(mod)}{mod}{len(cls)}{cls}C{len(fn)}{fn}yyF".encode())
            elif k < 9:
                ns, cls, fn = "Game", f"System{i % 997}", f"update{i}"
                names.append(f"__ZN{len(ns)}{ns}{len(cls)}{cls}{len(fn)}{fn}Ev".encode())
            else:
                names.append(f"_c_helper_{i}".encode())
            values.append(text_addr + starts[i])
            sects.append(1)
    strtab = bytearray(b" \0")
    nlist = bytearray()
    for name, value, sect in zip(names, values, sects):
        nlist += struct.pack("<IBBHQ", len(strtab), 0x0F, sect, 0, value)
        strtab += name + b"\0"
    nsyms = len(names)
    del names, values, sects
    stroff = symoff + len(nlist)
    strtab += b"\0" * (align(len(strtab), 16) - len(strtab))
    sig_off = align(stroff + len(strtab), 16)

    # Ad-hoc code signature: a SuperBlob holding a CodeDirectory of page hashes.
    ident = b"com.example.bigapp\0"
    nslots = (sig_off + 4095) // 4096
    cd_header = 88
    hash_off = cd_header + len(ident)
    cd_len = hash_off + 32 * nslots
    cd = bytearray(struct.pack(">9I", 0xFADE0C02, cd_len, 0x20400, 0x20002, hash_off, cd_header, 0, nslots, sig_off))
    cd += struct.pack(">BBBBI", 32, 2, 0, 12, 0)
    cd += struct.pack(">IIIQQQQ", 0, 0, 0, 0, text_off & ~(PAGE - 1), text_seg_size, 1)
    assert len(cd) == cd_header, len(cd)
    cd += ident
    cd += bytes(range(32)) * nslots
    superblob = struct.pack(">III", 0xFADE0CC0, 20 + len(cd), 1) + struct.pack(">II", 0, 20) + cd
    file_end = sig_off + len(superblob)
    linkedit_size = file_end - linkedit_off

    # --- load commands -----------------------------------------------------------------
    def seg(name, vmaddr, vmsize, fileoff, filesize, prot, sections=()):
        cmd = struct.pack("<II16sQQQQIIII", 0x19, 72 + 80 * len(sections), name.encode(), vmaddr, vmsize, fileoff, filesize, prot, prot, len(sections), 0)
        for sect, segname, addr, size, offset, al, flags in sections:
            cmd += struct.pack("<16s16sQQIIIIIIII", sect.encode(), segname.encode(), addr, size, offset, al, 0, 0, flags, 0, 0, 0)
        return cmd

    cmds = [
        seg("__PAGEZERO", 0, BASE, 0, 0, 0),
        seg("__TEXT", BASE, text_seg_size, 0, text_seg_size, 5, [
            ("__text", "__TEXT", text_addr, len(text), text_off, 2, 0x80000400),
            ("__cstring", "__TEXT", BASE + cstring_off, len(cstring), cstring_off, 0, 0x2),
            ("__objc_methname", "__TEXT", BASE + methname_off, len(methname), methname_off, 0, 0x2),
        ]),
        seg("__DATA_CONST", BASE + const_off, const_seg_size, const_off, const_seg_size, 3, [
            ("__const", "__DATA_CONST", BASE + const_off, len(ptrs), const_off, 3, 0),
        ]),
        seg("__DATA", BASE + data_off, data_seg_file + bss_size, data_off, data_seg_file, 3, [
            ("__data", "__DATA", BASE + data_off, len(data), data_off, 3, 0),
            ("__bss", "__DATA", BASE + data_off + data_seg_file, bss_size, 0, 3, 0x1),
        ]),
        seg("__LINKEDIT", BASE + linkedit_off, align(linkedit_size, PAGE), linkedit_off, linkedit_size, 1),
        struct.pack("<IIII", 0x26, 16, fstarts_off, len(fstarts)),
        struct.pack("<IIIIII", 0x2, 24, symoff, nsyms, stroff, len(strtab)),
        struct.pack("<II18I", 0xB, 80, 0, 0, 0, nsyms, nsyms, 0, *([0] * 12)),
        struct.pack("<II16s", 0x1B, 24, uuid.UUID("5f0c6e59-2b1a-4c8e-9d31-7a2b6c0e8f11").bytes),
        struct.pack("<IIIIII", 0x32, 24, 2, 0x000F0000, 0x00110000, 0),
        struct.pack("<IIQQ", 0x80000028, 24, text_off, 0),
        struct.pack("<III", 0xE, 32, 12) + b"/usr/lib/dyld".ljust(20, b"\0"),
        struct.pack("<IIIIII", 0xC, 56, 24, 2, 0x05276403, 0x00010000) + b"/usr/lib/libSystem.B.dylib".ljust(32, b"\0"),
        struct.pack("<IIII", 0x1D, 16, sig_off, len(superblob)),
    ]
    sizeofcmds = sum(len(c) for c in cmds)
    header = struct.pack("<IiiIIIII", 0xFEEDFACF, 0x0100000C, 0, 2, len(cmds), sizeofcmds, 0x00200085, 0)
    assert len(header) + sizeofcmds <= text_off

    with open(args.out, "wb") as f:
        f.truncate(file_end)
        for off, blob in [
            (0, header + b"".join(cmds)),
            (text_off, text),
            (cstring_off, cstring),
            (methname_off, methname),
            (const_off, ptrs),
            (data_off, data),
            (fstarts_off, fstarts),
            (symoff, nlist),
            (stroff, strtab),
            (sig_off, superblob),
        ]:
            f.seek(off)
            f.write(blob)
    print(f"{args.out}: {file_end / 1048576:.0f} MiB, {n} functions, {nsyms} symbols, "
          f"{args.strings + args.selectors} strings")


if __name__ == "__main__":
    main()
