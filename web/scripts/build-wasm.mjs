// Builds the binviz-wasm crate and generates the JS bindings into src/pkg.
// Usage: node scripts/build-wasm.mjs [--debug]
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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
for (const f of ['tiny-elf-x64', 'tiny-elf-a64', 'tiny-macho-a64', 'tiny-macho-a64.o', 'tiny-pe-x64.exe', 'shapes-pe.exe', 'shapes-pe.stripped.exe']) {
  const src = join(root, 'tests', 'fixtures', 'bin', f);
  if (existsSync(src)) copyFileSync(src, join(samples, f));
}
// Their source files, so the samples show source ↔ code mapping without a folder drop.
mkdirSync(join(samples, 'src'), { recursive: true });
for (const f of ['tiny.rs', 'shapes.cpp']) {
  copyFileSync(join(root, 'tests', 'fixtures', 'src', f), join(samples, 'src', f));
}
console.log('wasm ready');
