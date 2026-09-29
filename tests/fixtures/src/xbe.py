#!/usr/bin/env python3
"""Writes tiny.xbe, an original Xbox executable for the tests.

Laid out the way the XDK's image builder lays out a game's default.xbe: the
image header (with the retail keys), the certificate, the section headers,
their shared page counters and names, the XDK library versions, the debug
file names and a logo, all mapped at the base address 0x10000; then each
section's bytes, from a page of the file of its own. In memory the sections
are packed together, sharing pages: .text, D3D (library code in a section of
its own, as the XDK's libraries keep theirs), .rdata (the kernel thunk table
first, then strings and the TLS directory), .data (with uninitialized data
past its file bytes) and $$XTIMAGE (the title image, an inserted file).

The code is hand-assembled 32-bit x86 in MSVC's idioms: an entry point that
calls a function, a switch through a jump table and a D3D function, reads a
kernel variable through its thunk slot (KeTickCount), calls the kernel through
others, and ends in HalReturnToFirmware, which never returns; a function that
only a pointer in .data reaches; a TLS callback.

    python3 xbe.py <output folder>
"""

import hashlib
import struct
import sys
from pathlib import Path

BASE = 0x10000
RETAIL_ENTRY_KEY = 0xA8FC57AB
RETAIL_THUNK_KEY = 0x5B6D40B6
TIMESTAMP = 0x43CE2A00
TITLE_ID = 0x42560001  # "BV-001"

# The kernel's exports the code uses, by ordinal, in thunk table order.
KERNEL = [("KeTickCount", 156), ("DbgPrint", 8), ("HalReturnToFirmware", 49), ("NtClose", 187), ("KeBugCheck", 95)]

# Section flags.
WRITABLE, PRELOAD, EXECUTABLE, INSERTED_FILE, HEAD_PAGE_READ_ONLY, TAIL_PAGE_READ_ONLY = 1, 2, 4, 8, 0x10, 0x20


def u16(v):
    return struct.pack("<H", v)


def u32(v):
    return struct.pack("<I", v & 0xFFFFFFFF)


def align(v, n):
    return (v + n - 1) // n * n


def filler(n, seed):
    """n bytes that look random (signatures, keys, the logo), the same every time."""
    out = b""
    i = 0
    while len(out) < n:
        out += hashlib.sha256(f"{seed} {i}".encode()).digest()
        i += 1
    return out[:n]


class Section:
    """A section's bytes, with labels, and fixups resolved once every section has its address."""

    def __init__(self, name, flags, bss=0):
        self.name = name
        self.flags = flags
        self.bss = bss  # bytes of zeros past the file's, in memory
        self.data = bytearray()
        self.labels = {}
        self.fixups = []
        self.va = None

    def label(self, name):
        self.labels[name] = len(self.data)

    def b(self, *bs):
        self.data += bytes(bs)

    def raw(self, bs):
        self.data += bs

    def ref(self, op, label, kind, tail=b""):
        """The opcode bytes, then a label's address ("abs"), or its distance from
        the end of the field ("rel", 4 bytes; "rel8", 1 byte), then `tail`."""
        self.data += bytes(op)
        self.fixups.append((len(self.data), label, kind))
        self.data += bytes(1 if kind == "rel8" else 4)
        self.data += tail

    def align(self, n, fill=0xCC):
        while len(self.data) % n:
            self.data.append(fill)


def text():
    t = Section(".text", PRELOAD | EXECUTABLE)
    t.label("entry")
    t.b(0x55)                                          # push ebp
    t.b(0x8B, 0xEC)                                    # mov ebp, esp
    t.b(0x56)                                          # push esi
    t.ref([0xE8], "init", "rel")                       # call init
    t.b(0x6A, 0x02)                                    # push 2
    t.ref([0xE8], "dispatch", "rel")                   # call dispatch
    t.b(0x83, 0xC4, 0x04)                              # add esp, 4
    t.b(0x8B, 0xF0)                                    # mov esi, eax
    t.ref([0xE8], "D3DDevice_Clear", "rel")            # call D3DDevice_Clear
    t.ref([0xA1], "__imp_KeTickCount", "abs")          # mov eax, [__imp_KeTickCount]: its address
    t.b(0x8B, 0x00)                                    # mov eax, [eax]: the tick count
    t.b(0x03, 0xF0)                                    # add esi, eax
    t.b(0x56)                                          # push esi
    t.ref([0x68], "greeting", "abs")                   # push offset greeting
    t.ref([0xFF, 0x15], "__imp_DbgPrint", "abs")       # call [__imp_DbgPrint]
    t.b(0x83, 0xC4, 0x08)                              # add esp, 8
    t.b(0x6A, 0x02)                                    # push 2 (HalQuickRebootRoutine)
    t.ref([0xFF, 0x15], "__imp_HalReturnToFirmware", "abs")  # call [__imp_HalReturnToFirmware]: never returns
    t.align(16)

    t.label("init")
    t.ref([0xC7, 0x05], "g_counter", "abs", u32(1))    # mov dword ptr [g_counter], 1
    t.b(0xC3)                                          # ret
    t.align(16)

    t.label("dispatch")
    t.b(0x8B, 0x44, 0x24, 0x04)                        # mov eax, [esp+4]
    t.b(0x83, 0xF8, 0x03)                              # cmp eax, 3
    t.ref([0x77], "default", "rel8")                   # ja default
    t.ref([0xFF, 0x24, 0x85], "cases", "abs")          # jmp [eax*4+cases]
    t.label("case0")
    t.b(0x33, 0xC0)                                    # xor eax, eax
    t.b(0xC3)                                          # ret
    t.label("case1")
    t.b(0xB8, 10, 0, 0, 0)                             # mov eax, 10
    t.b(0xC3)                                          # ret
    t.label("case2")
    t.b(0xB8, 20, 0, 0, 0)                             # mov eax, 20
    t.b(0xC3)                                          # ret
    t.label("case3")
    t.b(0x6A, 0x07)                                    # push 7
    t.ref([0xE8], "fatal", "rel")                      # call fatal: never returns
    t.label("default")
    t.b(0x83, 0xC8, 0xFF)                              # or eax, -1
    t.b(0xC3)                                          # ret
    t.align(4)
    t.label("cases")                                   # the jump table, after the code as MSVC puts it
    for case in ("case0", "case1", "case2", "case3"):
        t.ref([], case, "abs")
    t.align(16)

    t.label("fatal")
    t.b(0xFF, 0x74, 0x24, 0x04)                        # push dword ptr [esp+4]
    t.ref([0xFF, 0x15], "__imp_KeBugCheck", "abs")     # call [__imp_KeBugCheck]: never returns
    t.align(16)

    t.label("callback")                                # only the pointer in .data reaches it
    t.b(0x55)                                          # push ebp
    t.b(0x8B, 0xEC)                                    # mov ebp, esp
    t.b(0xFF, 0x75, 0x08)                              # push dword ptr [ebp+8]
    t.ref([0xFF, 0x15], "__imp_NtClose", "abs")        # call [__imp_NtClose]
    t.b(0x5D)                                          # pop ebp
    t.b(0xC2, 0x04, 0x00)                              # ret 4
    t.align(16)

    t.label("tls_callback")
    t.b(0xC2, 0x0C, 0x00)                              # ret 12
    return t


def d3d():
    d = Section("D3D", PRELOAD | EXECUTABLE)
    d.label("D3DDevice_Clear")
    d.ref([0xA1], "g_frames", "abs")                   # mov eax, [g_frames]
    d.b(0x40)                                          # inc eax
    d.ref([0xA3], "g_frames", "abs")                   # mov [g_frames], eax
    d.b(0xC3)                                          # ret
    return d


def rdata():
    r = Section(".rdata", PRELOAD | HEAD_PAGE_READ_ONLY | TAIL_PAGE_READ_ONLY)
    r.label("kernel_thunks")
    for name, ordinal in KERNEL:
        r.label("__imp_" + name)
        r.raw(u32(0x80000000 | ordinal))
    r.raw(u32(0))
    r.label("greeting")
    r.raw(b"tiny: %d frames\n\0")
    r.align(2, 0)
    r.label("press_start")
    r.raw("Press START".encode("utf-16-le") + b"\0\0")
    r.align(4, 0)
    r.label("tls_directory")
    for label in ("tls_start", "tls_end", "tls_index", "tls_callbacks"):
        r.ref([], label, "abs")
    r.raw(u32(0) + u32(0))                             # SizeOfZeroFill, Characteristics
    r.label("tls_callbacks")
    r.ref([], "tls_callback", "abs")
    r.raw(u32(0))
    return r


def data():
    d = Section(".data", PRELOAD | WRITABLE, bss=0x40)
    d.label("g_counter")
    d.raw(u32(0))
    d.label("g_frames")
    d.raw(u32(0))
    d.label("callbacks")
    d.ref([], "callback", "abs")
    d.label("tls_index")
    d.raw(u32(0))
    d.label("tls_start")
    d.raw(b"per-thread data\0")
    d.label("tls_end")
    return d


def title_image():
    x = Section("$$XTIMAGE", INSERTED_FILE)
    pixels = filler(32, "title image")
    x.raw(b"XPR0" + u32(12 + len(pixels)) + u32(12) + pixels)
    return x


def xbe():
    sections = [text(), d3d(), rdata(), data(), title_image()]
    # Packed in memory from the first page after the headers', 32 bytes apart.
    va = BASE + 0x1000
    for s in sections:
        s.va = va
        va = align(va + len(s.data) + s.bss, 32)
    size_of_image = va - BASE
    labels = {name: s.va + off for s in sections for name, off in s.labels.items()}
    for s in sections:
        for at, label, kind in s.fixups:
            t = labels[label]
            if kind == "abs":
                s.data[at:at + 4] = u32(t)
            elif kind == "rel":
                s.data[at:at + 4] = u32(t - (s.va + at + 4))
            else:
                off = t - (s.va + at + 1)
                assert -128 <= off <= 127, (label, off)
                s.data[at] = off & 0xFF

    # The headers, in the order the image builder writes them.
    cert_at = 0x178
    headers_at = cert_at + 0x1D0
    counts_at = headers_at + 0x38 * len(sections)
    names_at = counts_at + 2 * (len(sections) + 1)
    names = b""
    name_at = {}
    for s in sections:
        name_at[s.name] = names_at + len(names)
        names += s.name.encode() + b"\0"
    libraries = [("XAPILIB", 0x4000), ("XBOXKRNL", 0x4000), ("LIBCMT", 0x4000), ("D3D8", 0x4001), ("DSOUND", 0x2000)]
    libraries_at = align(names_at + len(names), 4)
    unicode_name = "tiny.exe".encode("utf-16-le") + b"\0\0"
    unicode_at = libraries_at + 16 * len(libraries)
    path = b"D:\\tiny\\Release\\tiny.exe\0"
    path_at = align(unicode_at + len(unicode_name), 4)
    logo = filler(0x60, "logo")
    logo_at = align(path_at + len(path), 4)
    size_of_headers = logo_at + len(logo)

    h = bytearray(b"XBEH")
    h += filler(256, "signature")
    h += u32(BASE)
    h += u32(size_of_headers)
    h += u32(size_of_image)
    h += u32(0x178)                                    # size of image header
    h += u32(TIMESTAMP)
    h += u32(BASE + cert_at)
    h += u32(len(sections))
    h += u32(BASE + headers_at)
    h += u32(0x5)                                      # mount utility drive, limit development kit memory to 64 MiB
    h += u32(labels["entry"] ^ RETAIL_ENTRY_KEY)
    h += u32(labels["tls_directory"])
    h += u32(0x10000)                                  # the PE's stack commit
    h += u32(0x100000)                                 # heap reserve
    h += u32(0x1000)                                   # heap commit
    h += u32(BASE)                                     # base address
    h += u32(align(size_of_image, 0x1000))             # size of image
    h += u32(0)                                        # checksum
    h += u32(TIMESTAMP)                                # timestamp
    h += u32(BASE + path_at)
    h += u32(BASE + path_at + path.index(b"tiny.exe"))
    h += u32(BASE + unicode_at)
    h += u32(labels["kernel_thunks"] ^ RETAIL_THUNK_KEY)
    h += u32(0)                                        # no non-kernel imports
    h += u32(len(libraries))
    h += u32(BASE + libraries_at)
    h += u32(BASE + libraries_at + 16)                 # XBOXKRNL
    h += u32(BASE + libraries_at)                      # XAPILIB
    h += u32(BASE + logo_at)
    h += u32(len(logo))
    assert len(h) == 0x178

    c = bytearray(u32(0x1D0))
    c += u32(TIMESTAMP)
    c += u32(TITLE_ID)
    c += "Tiny".encode("utf-16-le").ljust(80, b"\0")
    c += (u32(TITLE_ID + 1) + bytes(4 * 15))           # one alternate title ID
    c += u32(0x3)                                      # allowed media: hard disk, DVD-X2
    c += u32(0x7)                                      # regions: North America, Japan, the rest of the world
    c += u32(0)                                        # ratings
    c += u32(0)                                        # disk number
    c += u32(0x100)                                    # version
    c += filler(16, "LAN key")
    c += filler(16, "signature key")
    c += filler(256, "alternate signature keys")
    assert len(c) == 0x1D0

    raws = [0x1000 * (i + 1) for i in range(len(sections))]
    sh = bytearray()
    for i, s in enumerate(sections):
        sh += u32(s.flags)
        sh += u32(s.va)
        sh += u32(len(s.data) + s.bss)
        sh += u32(raws[i])
        sh += u32(len(s.data))
        sh += u32(BASE + name_at[s.name])
        sh += u32(0)                                   # reference count
        sh += u32(BASE + counts_at + 2 * i)            # head shared page reference count
        sh += u32(BASE + counts_at + 2 * (i + 1))      # tail, the next section's head
        sh += hashlib.sha1(u32(len(s.data)) + bytes(s.data)).digest()

    out = bytearray(size_of_headers)
    out[0:len(h)] = h
    out[cert_at:cert_at + len(c)] = c
    out[headers_at:headers_at + len(sh)] = sh
    out[names_at:names_at + len(names)] = names
    for i, (name, flags) in enumerate(libraries):
        at = libraries_at + 16 * i
        out[at:at + 16] = name.encode().ljust(8, b"\0") + u16(1) + u16(0) + u16(5849) + u16(flags)
    out[unicode_at:unicode_at + len(unicode_name)] = unicode_name
    out[path_at:path_at + len(path)] = path
    out[logo_at:logo_at + len(logo)] = logo
    for s, raw in zip(sections, raws):
        out += bytes(raw - len(out))
        out += s.data
    return bytes(out)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    out.mkdir(parents=True, exist_ok=True)
    (out / "tiny.xbe").write_bytes(xbe())


if __name__ == "__main__":
    main()
