#!/usr/bin/env bash
# Regenerates the test binaries in tests/fixtures/bin.
#
# Everything is produced with the Rust toolchain's bundled rust-lld, so no
# platform SDKs are needed. Required rustup targets:
#   rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl aarch64-apple-darwin
# Optional: a MinGW g++ on PATH (for the C++ PE fixture) and the llvm-tools
# component (for the split-debug ELF pair).
#
# Pass --large to also build the std-linked "demo" binaries (~5 MB each) into
# tests/fixtures/large (git-ignored), which are handy for manual testing.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/tests/fixtures/src"
out="$root/tests/fixtures/bin"
mkdir -p "$out"

host="$(rustc -vV | sed -n 's/^host: //p')"
bindir="$(rustc --print sysroot)/lib/rustlib/$host/bin"
lld="$bindir/rust-lld"
objcopy="$bindir/llvm-objcopy"

tiny_flags=(-g -C opt-level=1 -C panic=abort --emit=obj)

echo "tiny: ELF x86-64 / AArch64"
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
    arch="${target%%-*}"
    [ "$arch" = aarch64 ] && name=a64 || name=x64
    rustc "$src/tiny.rs" "${tiny_flags[@]}" --target "$target" -C relocation-model=static \
        -o "$out/tiny-elf-$name.o" 2>/dev/null
    "$lld" -flavor gnu -e _start -o "$out/tiny-elf-$name" "$out/tiny-elf-$name.o"
done

echo "tiny: Mach-O arm64 object, executable, dylib"
rustc "$src/tiny.rs" "${tiny_flags[@]}" --target aarch64-apple-darwin -o "$out/tiny-macho-a64.o" 2>/dev/null
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -e __start \
    -o "$out/tiny-macho-a64" "$out/tiny-macho-a64.o"
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -dylib \
    -o "$out/libtiny.dylib" "$out/tiny-macho-a64.o"

if [ -x "$objcopy" ] || [ -x "$objcopy.exe" ]; then
    echo "tiny: split debug info (ELF + .debug with .gnu_debuglink)"
    "$objcopy" --only-keep-debug "$out/tiny-elf-x64" "$out/tiny-elf-x64.debug"
    "$objcopy" --strip-debug --add-gnu-debuglink="$out/tiny-elf-x64.debug" \
        "$out/tiny-elf-x64" "$out/tiny-elf-x64.stripped"
fi

echo "tiny: PE x86-64 (windows-gnu, DWARF)"
rustc "$src/tiny.rs" "${tiny_flags[@]}" --target x86_64-pc-windows-gnu -o "$out/tiny-pe-x64.o" 2>/dev/null
"$lld" -flavor gnu -m i386pep --entry=_start --subsystem=console -o "$out/tiny-pe-x64.exe" "$out/tiny-pe-x64.o"
rm -f "$out/tiny-pe-x64.o"

echo "imports: calls through PLT entries, GOT slots and Mach-O stubs; pointers in data"
tmp="$(mktemp -d)"
imports_flags=(-C opt-level=1 -C panic=abort --emit=obj)
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
    arch="${target%%-*}"
    [ "$arch" = aarch64 ] && name=a64 || name=x64
    rustc "$src/libstub.rs" "${imports_flags[@]}" --target "$target" -C relocation-model=pic \
        -o "$tmp/libstub-$name.o" 2>/dev/null
    "$lld" -flavor gnu -shared -soname libstub.so -o "$tmp/libstub-$name.so" "$tmp/libstub-$name.o"
    rustc "$src/imports.rs" "${imports_flags[@]}" --target "$target" -C relocation-model=pic \
        -o "$tmp/imports-$name.o" 2>/dev/null
done
# x86-64: RELA relocations; AArch64: packed RELR relocations.
"$lld" -flavor gnu -pie -e main -o "$out/imports-elf-x64" "$tmp/imports-x64.o" "$tmp/libstub-x64.so"
"$lld" -flavor gnu -pie -z pack-relative-relocs -e main -o "$out/imports-elf-a64" \
    "$tmp/imports-a64.o" "$tmp/libstub-a64.so"
rustc "$src/imports.rs" "${imports_flags[@]}" --target aarch64-apple-darwin -o "$tmp/imports-macho.o" 2>/dev/null
# Mach-O with dyld info (plain pointers), and with chained fixups.
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -e _main \
    -o "$out/imports-macho-a64" "$tmp/imports-macho.o" "$src/libSystem.tbd"
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -fixup_chains -e _main \
    -o "$out/imports-macho-a64.chained" "$tmp/imports-macho.o" "$src/libSystem.tbd"
rm -rf "$tmp"

if command -v g++ >/dev/null 2>&1; then
    echo "C++: PE x86-64 (MinGW g++, DWARF 5)"
    g++ -g -O1 -static -o "$out/shapes-pe.exe" "$src/shapes.cpp"
    if [ -x "$objcopy" ] || [ -x "$objcopy.exe" ]; then
        echo "C++: the same PE with every symbol and all debug info stripped"
        "$objcopy" --strip-all "$out/shapes-pe.exe" "$out/shapes-pe.stripped.exe"
    fi
fi

if [ "${1:-}" = "--large" ]; then
    large="$root/tests/fixtures/large"
    mkdir -p "$large"
    flags=(-g -C opt-level=1)
    echo "large: demo (std) for $host, x86_64/aarch64 musl, aarch64-apple-darwin object"
    rustc "$src/demo.rs" "${flags[@]}" -o "$large/demo-host$( [[ $host == *windows* ]] && echo .exe)"
    rustc "$src/demo.rs" "${flags[@]}" --target x86_64-unknown-linux-musl -C linker=rust-lld -o "$large/demo-elf-x64"
    rustc "$src/demo.rs" "${flags[@]}" --target aarch64-unknown-linux-musl -C linker=rust-lld -o "$large/demo-elf-a64"
    rustc "$src/demo.rs" "${flags[@]}" --target aarch64-apple-darwin --emit=obj -o "$large/demo-macho-a64.o"
fi

ls -l "$out"
