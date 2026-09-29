// Builds the binviz-wasm crate and generates the JS bindings into src/pkg.
// Usage: node scripts/build-wasm.mjs [--debug]
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateRawSync } from 'node:zlib';

const web = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const root = resolve(web, '..');
const debug = process.argv.includes('--debug');
const exe = process.platform === 'win32' ? '.exe' : '';

// Prefer tools on PATH, fall back to ~/.cargo/bin (rustup's default location).
function tool(name) {
  const local = join(homedir(), '.cargo', 'bin', name + exe);
  try {
    execFileSync(name, ['--version'], { stdio: 'ignore' });
    return name;
  } catch {
    if (existsSync(local)) return local;
    console.error(`error: ${name} not found. Install it with: ${name === 'wasm-bindgen' ? 'cargo install wasm-bindgen-cli --version 0.2.129' : 'https://rustup.rs'}`);
    process.exit(1);
  }
}

const run = (cmd, args) => {
  console.log(`> ${[cmd, ...args].join(' ')}`);
  execFileSync(cmd, args, { cwd: root, stdio: 'inherit' });
};

const profile = debug ? 'debug' : 'release';
run(tool('cargo'), ['build', '-p', 'binviz-wasm', '--target', 'wasm32-unknown-unknown', ...(debug ? [] : ['--release'])]);
run(tool('wasm-bindgen'), [
  '--target', 'web',
  '--out-dir', join(web, 'src', 'pkg'),
  '--out-name', 'binviz_wasm',
  join(root, 'target', 'wasm32-unknown-unknown', profile, 'binviz_wasm.wasm'),
]);

// Sample binaries for the landing page.
const samples = join(web, 'public', 'samples');
mkdirSync(samples, { recursive: true });
for (const f of ['tiny-elf-x64', 'tiny-elf-a64', 'tiny-macho-a64', 'tiny-macho-a64.o', 'tiny-pe-x64.exe', 'shapes-pe.exe', 'shapes-pe.stripped.exe', 'objc-macho-a64.chained.stripped', 'tiny.nes', 'tiny.gba', 'tiny-psx.exe']) {
  const src = join(root, 'tests', 'fixtures', 'bin', f);
  if (existsSync(src)) copyFileSync(src, join(samples, f));
}
// Their source files, so the samples show source ↔ code mapping without a folder drop.
mkdirSync(join(samples, 'src'), { recursive: true });
for (const f of ['tiny.rs', 'shapes.cpp']) {
  copyFileSync(join(root, 'tests', 'fixtures', 'src', f), join(samples, 'src', f));
}
writeFileSync(join(samples, 'Shop.xcarchive.zip'), samplePackage());
// A Windows build with its PDB beside it, as MSVC leaves them: they pair by the PDB's name.
if (existsSync(join(root, 'tests', 'fixtures', 'bin', 'pdbdemo.pdb'))) {
  const bin = (f) => readFileSync(join(root, 'tests', 'fixtures', 'bin', f));
  writeFileSync(join(samples, 'pdbdemo.zip'), zip([['Release/pdbdemo.exe', bin('pdbdemo.exe')], ['Release/pdbdemo.pdb', bin('pdbdemo.pdb')]]));
}
console.log('wasm ready');

/**
 * A small zipped .xcarchive built from the Mach-O fixtures: a stripped app, a
 * framework and an extension, resources (one image twice), and a dSYM for the
 * app (its unstripped copy, marked MH_DSYM, so the UUIDs match).
 */
function samplePackage() {
  const bin = (f) => readFileSync(join(root, 'tests', 'fixtures', 'bin', f));
  const plist = (exe, id, version = '4.2', build = '1234') =>
    Buffer.from(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
\t<key>CFBundleExecutable</key><string>${exe}</string>
\t<key>CFBundleIdentifier</key><string>${id}</string>
\t<key>CFBundleName</key><string>${exe}</string>
\t<key>CFBundleShortVersionString</key><string>${version}</string>
\t<key>CFBundleVersion</key><string>${build}</string>
\t<key>MinimumOSVersion</key><string>15.0</string>
\t<key>CFBundleSupportedPlatforms</key><array><string>iPhoneOS</string></array>
</dict>
</plist>
`);
  // Deterministic filler, so the sample is the same on every build.
  let seed = 7;
  const noise = (n) => Buffer.from(Array.from({ length: n }, () => ((seed = (seed * 1103515245 + 12345) >>> 0) >>> 16) & 0xff));
  const dsym = Buffer.from(bin('imports-macho-a64.chained'));
  dsym.writeUInt32LE(10, 12); // MH_DSYM
  const icon = noise(3000);
  const app = 'Shop.xcarchive/Products/Applications/Shop.app/';
  const files = [
    ['Shop.xcarchive/Info.plist', plist('Shop', 'archive')],
    [app + 'ShopApp', bin('imports-macho-a64.chained.stripped')],
    [app + 'Info.plist', plist('ShopApp', 'com.example.shop')],
    [app + 'Frameworks/Tiny.framework/Tiny', bin('libtiny.dylib')],
    [app + 'Frameworks/Tiny.framework/Info.plist', plist('Tiny', 'com.example.tiny', '1.0', '7')],
    [app + 'Frameworks/Tiny.framework/Headers/Tiny.h', Buffer.from('// Tiny.framework\n'.repeat(60))],
    [app + 'PlugIns/Widget.appex/Widget', bin('tiny-macho-a64')],
    [app + 'PlugIns/Widget.appex/Info.plist', plist('Widget', 'com.example.shop.widget')],
    [app + 'Assets.car', noise(40_000)],
    [app + 'icon.png', icon],
    [app + 'Bundle.bundle/icon-copy.png', icon],
    [app + 'en.lproj/Localizable.strings', Buffer.from('"hello" = "Hello";\n'.repeat(100))],
    [app + '_CodeSignature/CodeResources', Buffer.from('<plist/>'.repeat(200))],
    ['Shop.xcarchive/dSYMs/Shop.app.dSYM/Contents/Resources/DWARF/ShopApp', dsym],
    ['Shop.xcarchive/dSYMs/Shop.app.dSYM/Contents/Info.plist', plist('ShopApp', 'com.apple.xcode.dsym.com.example.shop')],
  ];
  return zip(files);
}

/** A zip of `[path, bytes]` entries, deflated. */
function zip(files) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc32 = (buf) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const local = [];
  const central = [];
  let offset = 0;
  for (const [path, data] of files) {
    const name = Buffer.from(path);
    const packed = deflateRawSync(data, { level: 9 });
    const crc = crc32(data);
    const head = Buffer.alloc(30);
    head.writeUInt32LE(0x04034b50, 0);
    head.writeUInt16LE(20, 4);
    head.writeUInt16LE(8, 8); // deflate
    head.writeUInt16LE(0x21, 10); // a fixed DOS date/time (1980-01-01)
    head.writeUInt16LE(0x21, 12);
    head.writeUInt32LE(crc, 14);
    head.writeUInt32LE(packed.length, 18);
    head.writeUInt32LE(data.length, 22);
    head.writeUInt16LE(name.length, 26);
    local.push(head, name, packed);
    const cd = Buffer.alloc(46);
    cd.writeUInt32LE(0x02014b50, 0);
    cd.writeUInt16LE(0x0314, 4); // made by Unix
    cd.writeUInt16LE(20, 6);
    cd.writeUInt16LE(8, 10);
    cd.writeUInt16LE(0x21, 12);
    cd.writeUInt16LE(0x21, 14);
    cd.writeUInt32LE(crc, 16);
    cd.writeUInt32LE(packed.length, 20);
    cd.writeUInt32LE(data.length, 24);
    cd.writeUInt16LE(name.length, 28);
    cd.writeUInt32LE((0o100644 << 16) >>> 0, 38);
    cd.writeUInt32LE(offset, 42);
    central.push(cd, name);
    offset += head.length + name.length + packed.length;
  }
  const cdSize = central.reduce((a, b) => a + b.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(cdSize, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, ...central, end]);
}
