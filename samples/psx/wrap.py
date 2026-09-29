"""Wrap a linked MIPS ELF (from psx.ld) as a PS-X EXE, and write binviz notes naming its functions.

usage: wrap.py <linked.elf> <out.exe> <out.notes.json> [llvm-tools-dir]
"""
import json
import os
import struct
import subprocess
import sys

elf, exe, notes = sys.argv[1:4]
tools = sys.argv[4] if len(sys.argv) > 4 else ""
tool = lambda n: os.path.join(tools, n) if tools else n

data = open(elf, "rb").read()
assert data[:4] == b"\x7fELF" and data[4] == 1 and data[5] == 1, "expected a 32-bit little-endian ELF"
(e_entry, e_phoff, e_shoff, e_flags, e_ehsize, e_phentsize, e_phnum) = struct.unpack_from("<IIIIHHH", data, 24)

# Loadable bytes: every PT_LOAD segment, laid out from the lowest address.
loads = []
for i in range(e_phnum):
    p_type, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align = struct.unpack_from(
        "<IIIIIIII", data, e_phoff + i * e_phentsize
    )
    if p_type == 1:
        loads.append((p_vaddr, p_filesz, p_memsz, data[p_offset:p_offset + p_filesz]))
loads.sort()
base = loads[0][0]
end = max(v + fs for v, fs, _, _ in loads)
mem_end = max(v + ms for v, _, ms, _ in loads)
image = bytearray(end - base)
for v, fs, _, bytes_ in loads:
    image[v - base:v - base + fs] = bytes_
while len(image) % 2048:
    image.append(0)

header = bytearray(0x800)
header[0:8] = b"PS-X EXE"
struct.pack_into("<I", header, 0x10, e_entry)          # initial pc
struct.pack_into("<I", header, 0x14, 0)                # initial $gp (none: -G0)
struct.pack_into("<I", header, 0x18, base)             # load address
struct.pack_into("<I", header, 0x1C, len(image))       # size
struct.pack_into("<I", header, 0x28, end)              # bss address
struct.pack_into("<I", header, 0x2C, max(0, mem_end - end))  # bss size
struct.pack_into("<I", header, 0x30, 0x801FFFF0)       # stack
header[0x4C:0x4C + 56] = b"Sony Computer Entertainment Inc. for North America area".ljust(56, b"\0")
open(exe, "wb").write(bytes(header) + bytes(image))

# Names: every function and data symbol, with sizes, from llvm-nm.
out = subprocess.run([tool("llvm-nm"), "-S", "--defined-only", elf], capture_output=True, text=True, check=True).stdout
annotations = []
for line in out.splitlines():
    parts = line.split()
    if len(parts) != 4:
        continue
    address, size, kind, name = parts
    if kind.lower() not in "tdrb" or name.startswith(("$", "__", ".L")):
        continue
    annotations.append({"address": int(address, 16), "size": int(size, 16), "name": name, "comment": "", "reviewed": False,
                        "kind": "function" if kind.lower() == "t" else "data"})
json.dump(annotations, open(notes, "w"), indent=1)
print(f"{exe}: {len(image)} bytes at {base:#x}, entry {e_entry:#x}; {len(annotations)} names in {notes}")
