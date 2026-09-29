#!/usr/bin/env bash
# Regenerates the test binaries in tests/fixtures/bin.
#
# Everything is produced with the Rust toolchain's bundled rust-lld, so no
# platform SDKs are needed. Required rustup targets:
#   rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl aarch64-apple-darwin
# Optional: a MinGW g++ on PATH (for the C++ PE fixture), clang (any build with
# the x86 target, Apple's included: for the 32-bit PEs with PDBs and the 32-bit
# ELF of C types), the llvm-tools component (for the split-debug ELF pair) and
# the x86_64-pc-windows-msvc target (for the PE with a PDB). Python 3 writes the
# game ROMs.
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
# The executables' DWARF stays in the object, which their debug map names: once
# as linked, once with the functions reordered and main dead-stripped.
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -e __start \
    -dead_strip -no_exported_symbols -order_file "$src/tiny.order" \
    -o "$out/tiny-macho-a64.reordered" "$out/tiny-macho-a64.o"

if [ -x "$objcopy" ] || [ -x "$objcopy.exe" ]; then
    echo "tiny: split debug info (ELF + .debug with .gnu_debuglink)"
    "$objcopy" --only-keep-debug "$out/tiny-elf-x64" "$out/tiny-elf-x64.debug"
    "$objcopy" --strip-debug --add-gnu-debuglink="$out/tiny-elf-x64.debug" \
        "$out/tiny-elf-x64" "$out/tiny-elf-x64.stripped"
    # No symbols at all: names come back only from the debug file.
    "$objcopy" --strip-all "$out/tiny-elf-x64" "$out/tiny-elf-x64.stripped-all"
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

echo "objc: Objective-C classes, categories, selector references and objc_msgSend\$ stubs"
rustc "$src/objc.rs" "${imports_flags[@]}" --target aarch64-apple-darwin -o "$tmp/objc.o" 2>/dev/null
# With dyld info (binds by opcode), and with chained fixups.
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -e _main \
    -o "$out/objc-macho-a64" "$tmp/objc.o" "$src/libSystem.tbd" "$src/libobjc.tbd"
"$lld" -flavor darwin -arch arm64 -platform_version macos 12.0 12.0 -fixup_chains -e _main \
    -o "$tmp/objc-macho-a64.chained" "$tmp/objc.o" "$src/libSystem.tbd" "$src/libobjc.tbd"
if [ -x "$objcopy" ] || [ -x "$objcopy.exe" ]; then
    # The app binary of the sample package; the unstripped copy stands in for its dSYM.
    "$objcopy" --strip-all "$out/imports-macho-a64.chained" "$out/imports-macho-a64.chained.stripped"
    # Without its symbols, only the Objective-C metadata names the methods.
    "$objcopy" --strip-all "$tmp/objc-macho-a64.chained" "$out/objc-macho-a64.chained.stripped"
fi
rm -rf "$tmp"

if rustup target list --installed 2>/dev/null | grep -qx x86_64-pc-windows-msvc; then
    echo "pdbdemo: PE x86-64 with its debug info in a PDB (windows-msvc, no C runtime)"
    rustc "$src/pdbdemo.rs" --target x86_64-pc-windows-msvc -g -C opt-level=0 -C panic=abort \
        -C linker="$lld" -C linker-flavor=lld-link \
        -C link-arg=-NODEFAULTLIB -C link-arg=-ENTRY:start -C link-arg=-SUBSYSTEM:CONSOLE \
        -o "$out/pdbdemo.exe"
fi

if command -v clang >/dev/null 2>&1; then
    echo "x86demo: PE32 (clang for the MSVC ABI, lld-link, no C runtime) with a PDB, and linked /FIXED"
    tmp="$(mktemp -d)"
    cp "$src/x86demo.cpp" "$src/x86demo-msvc.s" "$src/kernel32.def" "$tmp"
    ln -s "$lld" "$tmp/lld-link"
    (
        # Relative paths only, so that none of this machine's end up in the PDB.
        cd "$tmp"
        clang -target i686-pc-windows-msvc -O2 -g -gcodeview -fno-exceptions -fno-rtti \
            -ffile-compilation-dir=. -c x86demo.cpp -o x86demo.obj
        clang -target i686-pc-windows-msvc -c x86demo-msvc.s -o x86demo-msvc.obj
        ./lld-link /lib /machine:x86 /def:kernel32.def /out:kernel32.lib
        link=(/nologo /brepro /nodefaultlib /entry:start /subsystem:console /debug /opt:noref,noicf
            /safeseh /pdbsourcepath:c:/src x86demo.obj x86demo-msvc.obj kernel32.lib)
        ./lld-link "${link[@]}" /pdb:x86demo.pdb /pdbaltpath:x86demo.pdb /out:x86demo.exe
        # Without base relocations, as games of the time were linked: the same code at the
        # same addresses (under the same names, so that the tables in .rdata don't move).
        mkdir fixed
        ./lld-link "${link[@]}" /fixed /pdb:fixed/x86demo.pdb /pdbaltpath:x86demo.pdb /out:fixed/x86demo.exe
    )
    cp "$tmp/x86demo.exe" "$tmp/x86demo.pdb" "$out"
    cp "$tmp/fixed/x86demo.exe" "$out/x86demo-fixed.exe"
    rm -rf "$tmp"

    echo "layouts: C types for header export, 32-bit x86 twice: ELF with DWARF (System V layout), PE with a PDB (Microsoft's)"
    tmp="$(mktemp -d)"
    cp "$src/layouts.c" "$tmp"
    ln -s "$lld" "$tmp/lld-link"
    (
        # Relative paths only, as for x86demo (the PDB records the linker's).
        cd "$tmp"
        clang -target i386-pc-linux-gnu -O1 -g -ffreestanding -fno-pic -fno-stack-protector \
            -ffile-compilation-dir=. -c layouts.c -o layouts.o
        "$lld" -flavor gnu -m elf_i386 -e _start -o layouts-elf-x86 layouts.o
        clang -target i686-pc-windows-msvc -O1 -g -gcodeview -ffreestanding \
            -ffile-compilation-dir=. -c layouts.c -o layouts.obj
        ./lld-link /nologo /brepro /nodefaultlib /entry:start /subsystem:console /debug \
            /pdbsourcepath:c:/src /pdb:layouts-pe-x86.pdb /pdbaltpath:layouts-pe-x86.pdb \
            /out:layouts-pe-x86.exe layouts.obj
    )
    cp "$tmp/layouts-elf-x86" "$tmp/layouts-pe-x86.exe" "$tmp/layouts-pe-x86.pdb" "$out"
    rm -rf "$tmp"
fi

echo "ROMs: NES, Game Boy, Game Boy Advance, Mega Drive, SNES, Nintendo 64, PlayStation (hand-assembled)"
python3 "$src/roms.py" "$out" 2>/dev/null || python "$src/roms.py" "$out"

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
