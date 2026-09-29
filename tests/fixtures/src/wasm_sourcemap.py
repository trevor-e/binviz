#!/usr/bin/env python3
"""Writes a source map for a WebAssembly module from its DWARF line table,
as Emscripten's tools/wasm-sourcemap.py does for `emcc -gsource-map`, and
names the map in another module's sourceMappingURL section (a build of the
same code without its DWARF).

usage: wasm_sourcemap.py <module with DWARF> <out.map> <module to name it in> <url>

Each mapping's generated column is a byte offset in the module: the DWARF
line table's address (which counts from the code section's contents) plus
where the code section's contents start. Needs llvm-dwarfdump on PATH.
"""
import json
import re
import subprocess
import sys

BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'


def read_uleb(data, pos):
    value = shift = 0
    while True:
        b = data[pos]
        pos += 1
        value |= (b & 0x7F) << shift
        shift += 7
        if b < 0x80:
            return value, pos


def uleb(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)


def vlq(n):
    v = (-n << 1) | 1 if n < 0 else n << 1
    out = ''
    while True:
        digit = v & 31
        v >>= 5
        out += BASE64[digit | (32 if v else 0)]
        if not v:
            return out


def code_offset(data):
    pos = 8
    while pos < len(data):
        size, start = read_uleb(data, pos + 1)
        if data[pos] == 10:
            return start
        pos = start + size
    sys.exit('no code section')


def line_rows(path):
    """(address, file, line, column, end of sequence) for every row of every line table."""
    text = subprocess.run(['llvm-dwarfdump', '--debug-line', path], capture_output=True, text=True, check=True).stdout
    files, index, rows = {}, None, []
    for line in text.splitlines():
        if line.startswith('debug_line['):
            files = {}
        m = re.match(r'\s*file_names\[\s*(\d+)\]:', line)
        if m:
            index = int(m.group(1))
            continue
        m = re.match(r'\s*name:\s*"(.*)"', line)
        if m and index is not None:
            files[index] = m.group(1)
            index = None
            continue
        m = re.match(r'0x([0-9a-f]+)\s+(\d+)\s+(\d+)\s+(\d+)\s+\d+\s+\d+\s+(?:\d+\s+)?(.*)', line)
        if m:
            rows.append((int(m.group(1), 16), files.get(int(m.group(4)), '?'), int(m.group(2)),
                         int(m.group(3)), 'end_sequence' in m.group(5)))
    return rows


def main():
    module, out, target, url = sys.argv[1:5]
    data = open(module, 'rb').read()
    base = code_offset(data)
    rows = sorted(line_rows(module), key=lambda r: r[0])
    # Of several rows at one address, the last.
    by_address = {}
    for r in rows:
        by_address[r[0]] = r
    sources, segments = [], []
    last = [0, 0, 0, 0]
    for address, path, line, column, end in sorted(by_address.values()):
        column_in_module = base + address
        if end or line == 0:
            segments.append(vlq(column_in_module - last[0]))
        else:
            if path not in sources:
                sources.append(path)
            fields = [column_in_module, sources.index(path), line - 1, max(column - 1, 0)]
            segments.append(''.join(vlq(f - p) for f, p in zip(fields, last)))
            last[1:] = fields[1:]
        last[0] = column_in_module
    with open(out, 'w') as f:
        json.dump({'version': 3, 'sources': sources, 'names': [], 'mappings': ','.join(segments)}, f)
        f.write('\n')
    # The map's name, in a custom section at the end of the module.
    name, value = b'sourceMappingURL', url.encode()
    body = uleb(len(name)) + name + uleb(len(value)) + value
    with open(target, 'ab') as f:
        f.write(b'\x00' + uleb(len(body)) + body)


main()
