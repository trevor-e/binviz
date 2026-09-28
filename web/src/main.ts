import './styles.css';
import { Inspector } from './inspector';
import { SearchPalette } from './palette';
import { LABEL_FILE, PATCH_FILE, store, type Intent, type Target, type ViewName } from './store';
import { toast } from './ui';
import { basename, formatSize, h, icon } from './util';
import type { PackageSource } from './types';
import { FolderView, KIND_GROUPS } from './views/folder';
import type { View } from './views/base';
import { CallsView } from './views/calls';
import { CodeView } from './views/code';
import { CrashView } from './views/crash';
import { DiffView } from './views/diff';
import { DwarfView } from './views/dwarf';
import { HexView } from './views/hex';
import { LayoutView } from './views/layout';
import { OverviewView, importLabels, loadCodeLog } from './views/overview';
import { PatchView } from './views/patch';
import { SourcesView } from './views/sources';
import { SymbolsView } from './views/symbols';
import { TextView } from './views/text';
import { TilesView } from './views/tiles';

/** Bundled samples (copied from tests/fixtures by scripts/build-wasm.mjs) and their sources. */
const SAMPLES: { file: string; label: string; sources: string[]; folder?: boolean }[] = [
  { file: 'tiny-elf-x64', label: 'ELF · x86-64', sources: ['tiny.rs'] },
  { file: 'tiny-elf-a64', label: 'ELF · AArch64', sources: ['tiny.rs'] },
  { file: 'tiny-macho-a64', label: 'Mach-O · arm64', sources: ['tiny.rs'] },
  { file: 'tiny-macho-a64.o', label: 'Mach-O object', sources: ['tiny.rs'] },
  { file: 'tiny-pe-x64.exe', label: 'PE · x86-64', sources: ['tiny.rs'] },
  { file: 'shapes-pe.exe', label: 'PE · C++ with DWARF 5', sources: ['shapes.cpp'] },
  { file: 'shapes-pe.stripped.exe', label: 'PE · stripped (reverse engineering)', sources: [] },
  { file: 'pdbdemo.zip', label: 'PE · MSVC-style, debug info in a PDB', sources: [], folder: true },
  { file: 'objc-macho-a64.chained.stripped', label: 'Mach-O · stripped Objective-C', sources: [] },
  { file: 'tiny.nes', label: 'NES ROM (6502, text, tiles)', sources: [] },
  { file: 'tiny.gba', label: 'Game Boy Advance ROM (ARM and Thumb)', sources: [] },
  { file: 'Shop.xcarchive.zip', label: 'Zipped iOS app archive (3 binaries + dSYM)', sources: [], folder: true },
];

const NAV: { view: ViewName; label: string; key: string }[] = [
  { view: 'folder', label: 'Folder', key: 'f' },
  { view: 'crash', label: 'Crash', key: 'c' },
  { view: 'diff', label: 'Compare', key: 'd' },
  { view: 'overview', label: 'Overview', key: '1' },
  { view: 'layout', label: 'Layout', key: '2' },
  { view: 'hex', label: 'Hex', key: '3' },
  { view: 'code', label: 'Code', key: '4' },
  { view: 'calls', label: 'Call graph', key: '5' },
  { view: 'symbols', label: 'Symbols', key: '6' },
  { view: 'sources', label: 'Sources', key: '7' },
  { view: 'dwarf', label: 'DWARF', key: '8' },
  { view: 'text', label: 'Text', key: 't' },
  { view: 'tiles', label: 'Tiles', key: 'g' },
  { view: 'patch', label: 'Patch', key: 'p' },
];

// --- Theme ------------------------------------------------------------------

const media = window.matchMedia('(prefers-color-scheme: dark)');
/** Theme forced by a deep link for this page load only. */
let themeOverride: string | null = null;
function applyTheme() {
  const saved = themeOverride ?? localStorageGet('binviz-theme');
  const root = document.documentElement;
  if (saved === 'light' || saved === 'dark') root.dataset.theme = saved;
  else delete root.dataset.theme;
  root.dataset.resolvedTheme = saved === 'light' || saved === 'dark' ? saved : media.matches ? 'dark' : 'light';
  window.dispatchEvent(new Event('themechange'));
}
media.addEventListener('change', applyTheme);
applyTheme();

function localStorageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function localStorageSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* private mode */
  }
}

// --- Shell ------------------------------------------------------------------

const app = document.getElementById('app')!;
const views: Record<ViewName, View> = {
  folder: new FolderView(),
  crash: new CrashView(),
  diff: new DiffView(),
  overview: new OverviewView(),
  layout: new LayoutView(),
  hex: new HexView(),
  code: new CodeView(),
  calls: new CallsView(),
  symbols: new SymbolsView(),
  dwarf: new DwarfView(),
  sources: new SourcesView(),
  text: new TextView(),
  tiles: new TilesView(),
  patch: new PatchView(),
};
const inspector = new Inspector();
const palette = new SearchPalette();

const fileInput = h('input', { type: 'file', style: 'display:none' });
const debugInput = h('input', { type: 'file', style: 'display:none' });
const sourcesInput = h('input', { type: 'file', multiple: true, webkitdirectory: true, style: 'display:none' });
const packageInput = h('input', { type: 'file', multiple: true, webkitdirectory: true, style: 'display:none' });

const fileInfo = h('div', { class: 'fileinfo' });
const busy = h('div', { class: 'busy' });
busy.hidden = true;
/** Shows how far along reading a large file is. */
const progress = h('div', { class: 'progress', role: 'status' });
progress.hidden = true;

const button = (label: string, title: string, iconName: Parameters<typeof icon>[0], onClick: () => void, extra = '') => {
  const b = h('button', { class: `btn ${extra}`, title, type: 'button' }, icon(iconName), label ? h('span', null, label) : null);
  b.addEventListener('click', onClick);
  return b;
};
const openBtn = button('Open', 'Open a binary (Ctrl+O)', 'open', () => fileInput.click());
const debugBtn = button('Debug file', 'Load debug info from a separate file (.dSYM DWARF, .debug, .pdb, unstripped copy)', 'debug', () => debugInput.click());
const sourcesBtn = button('Sources', 'Load a source folder to show code next to addresses', 'source', () => sourcesInput.click());
const themeBtn = button('', 'Toggle light/dark theme', 'theme', () => {
  const next = document.documentElement.dataset.resolvedTheme === 'dark' ? 'light' : 'dark';
  localStorageSet('binviz-theme', next);
  themeOverride = null;
  applyTheme();
}, 'ghost icon-only');
const inspectorBtn = button('', 'Toggle inspector', 'chevron', () => {
  if (window.innerWidth <= 1100) app.classList.toggle('show-inspector');
  else app.classList.toggle('no-inspector');
}, 'ghost icon-only');

const topbar = h(
  'header',
  { class: 'topbar' },
  h('div', { class: 'brand' }, h('div', { class: 'brand-mark', 'aria-hidden': 'true' }, ...['code', 'header', 'rodata', 'debug'].map((f) => h('span', { style: `background:var(--f-${f})` }))), 'binviz'),
  fileInfo,
  palette.el,
  h('div', { class: 'actions' }, openBtn, debugBtn, sourcesBtn, themeBtn, inspectorBtn),
  busy,
  progress,
  fileInput,
  debugInput,
  sourcesInput,
  packageInput,
);

const navButtons = new Map<ViewName, HTMLButtonElement>();
const back = button('', 'Back (Alt+←, or the browser’s Back)', 'back', () => store.back(), 'ghost small icon-only');
const forward = button('', 'Forward (Alt+→)', 'forward', () => store.forward(), 'ghost small icon-only');
const sidebar = h('nav', { class: 'sidebar', 'aria-label': 'Views' });
for (const n of NAV) {
  const b = h('button', { class: 'nav-item', type: 'button' }, n.label, h('kbd', null, n.key));
  b.addEventListener('click', () => store.setView(n.view));
  navButtons.set(n.view, b);
  sidebar.appendChild(b);
}
sidebar.append(h('div', { class: 'nav-sep' }), h('div', { class: 'nav-history' }, back, forward));

const main = h('main', { class: 'main' });
const landing = h('div', { class: 'landing' });
main.appendChild(landing);
for (const v of Object.values(views)) main.appendChild(v.el);

app.replaceChildren(topbar, sidebar, main, inspector.el);
document.body.appendChild(h('div', { class: 'drop-overlay' }, 'Drop a binary, a folder or zip of them, or a crash report'));

// --- Rendering state ----------------------------------------------------------

function renderChrome() {
  const f = store.file;
  const pkg = store.package;
  app.classList.toggle('empty', !f && !pkg && !store.crash);
  landing.style.display = f || pkg || store.crash ? 'none' : 'grid';
  debugBtn.toggleAttribute('disabled', !f);
  sourcesBtn.toggleAttribute('disabled', !f?.dwarf);
  palette.input.disabled = !f;
  for (const [name, b] of navButtons) {
    if (name === 'folder') b.hidden = !pkg;
    else if (name === 'crash') b.hidden = !store.crash;
    // Games' own text encodings: for ROMs.
    else if (name === 'text' || name === 'tiles') b.hidden = f?.summary.format !== 'rom' && f?.summary.format !== 'unknown';
    // A patch applied, or bytes edited.
    else if (name === 'patch') b.hidden = !store.patch;
    else if (name === 'diff') b.toggleAttribute('disabled', !f && !pkg);
    else b.toggleAttribute('disabled', !f);
  }
  if (!f) {
    const name = pkg?.info.name ?? store.crash?.name;
    fileInfo.replaceChildren(...(name ? [h('span', { class: 'name', title: name }, basename(name))] : []));
    if (!pkg && !store.crash) renderLanding();
    return;
  }
  const s = f.summary;
  fileInfo.replaceChildren(
    ...(pkg ? [h('span', { class: 'name pkg', title: pkg.info.name }, basename(pkg.info.name)), h('span', { class: 'muted' }, '›'), binarySwitcher()] : [h('span', { class: 'name', title: f.name }, basename(f.name))]),
    h('span', { class: 'chip' }, s.formatName),
    h('span', { class: 'chip' }, s.arch),
    h('span', { class: 'chip' }, s.kind),
    h('span', { class: 'chip' }, formatSize(s.fileSize)),
    h('span', { class: `chip ${s.hasDwarf ? 'ok' : 'off'}`, title: s.hasDwarf ? 'DWARF debug info loaded' : 'No DWARF debug info' }, s.hasDwarf ? 'DWARF' : 'no DWARF'),
  );
}

/** Picks which of the package's binaries the views show, grouped by what they are. */
function binarySwitcher(): HTMLElement {
  const p = store.package!;
  const select = h('select', { class: 'binary-switch', title: 'The binary the views show', 'aria-label': 'Binary' });
  const groups = new Map<string, HTMLOptGroupElement>();
  for (const b of p.info.binaries) {
    const label = KIND_GROUPS[b.kind];
    let group = groups.get(label);
    if (!group) {
      group = h('optgroup', { label });
      groups.set(label, group);
      select.appendChild(group);
    }
    group.appendChild(h('option', { value: String(b.index) }, b.name));
  }
  select.value = String(p.current);
  select.addEventListener('change', () => void store.selectBinary(Number(select.value)));
  return select;
}

function renderView() {
  for (const v of Object.values(views)) {
    const active = v.visible;
    v.el.classList.toggle('active', active);
    if (active) v.show();
  }
  for (const [name, b] of navButtons) b.classList.toggle('active', name === store.view);
  back.toggleAttribute('disabled', !store.canGoBack());
  forward.toggleAttribute('disabled', !store.canGoForward());
}

function renderLanding() {
  if (store.container) {
    const c = store.container;
    const list = h('div', { style: 'display:flex;flex-direction:column;gap:6px;margin-top:16px;text-align:left;max-height:50vh;overflow:auto' });
    for (const m of c.info.members) {
      const b = h('button', { class: 'btn', style: 'justify-content:space-between;height:auto;padding:8px 12px' }, h('span', { class: 'mono' }, m.name), h('span', { class: 'muted' }, `${m.arch ?? ''} · ${formatSize(m.size)}`));
      b.addEventListener('click', () => void store.openMember(m.index));
      list.appendChild(b);
    }
    landing.replaceChildren(h('div', { class: 'landing-card' }, h('h1', null, basename(c.name)), h('p', { class: 'lead' }, `${c.info.kind} with ${c.info.members.length} members. Pick one to inspect.`), list));
    return;
  }
  const choose = h('button', { class: 'btn primary', type: 'button' }, 'Choose a file');
  choose.addEventListener('click', () => fileInput.click());
  const chooseFolder = h('button', { class: 'btn', type: 'button', title: 'Every binary in it opens, with its debug file (an .app, an .xcarchive, a build folder…)' }, 'Choose a folder');
  chooseFolder.addEventListener('click', () => packageInput.click());
  const samples = h('div', { class: 'samples' });
  for (const s of SAMPLES) {
    const b = h('button', { class: 'btn small', type: 'button', title: s.file }, s.label);
    b.addEventListener('click', () => openSample(s.file).catch((e) => toast(e instanceof Error ? e.message : String(e), 'error')));
    samples.appendChild(b);
  }
  landing.replaceChildren(
    h(
      'div',
      { class: 'landing-card' },
      h('h1', null, 'See what every byte of a binary is'),
      h('p', { class: 'lead' }, 'ELF, Mach-O and PE: headers down to single fields, sections and segments, symbols, disassembly, and DWARF mapped back to source.'),
      h(
        'div',
        { class: 'dropzone' },
        h('div', { class: 'big' }, 'Drop a binary, or a folder or zip of them'),
        h('div', { class: 'secondary', style: 'margin-bottom:14px' }, 'executables, shared libraries, object files, debug files (.dSYM, .debug, .pdb), universal binaries and archives; a folder or zip (an .ipa, an .app, a build) opens every binary in it, each paired with its debug file. Drop or paste a crash report (.crash, .ips, a tombstone) to symbolicate it.'),
        h('div', { class: 'dropzone-actions' }, choose, chooseFolder),
      ),
      h('div', { class: 'secondary', style: 'margin-top:22px' }, 'Or try a sample:'),
      samples,
      h('p', { class: 'privacy' }, 'Everything runs locally in your browser via WebAssembly. Files never leave your machine.'),
    ),
  );
}

store.on('file', () => {
  renderChrome();
  renderView();
});
store.on('container', () => renderChrome());
store.on('package', () => {
  renderChrome();
  renderView();
});
store.on('crash', () => {
  renderChrome();
  renderView();
});
store.on('diff', () => renderView());
store.on('patch', () => renderChrome());
store.on('view', () => renderView());
store.on('history', () => {
  back.toggleAttribute('disabled', !store.canGoBack());
  forward.toggleAttribute('disabled', !store.canGoForward());
});
store.on('sources', () => sourcesBtn.toggleAttribute('disabled', !store.file?.dwarf));
store.on('error', (msg) => toast(msg, 'error'));
store.on('status', (msg) => {
  if (msg) toast(msg, 'info', 2500);
});
/** What a package operation is doing, shown with its progress. */
let phase = '';
store.api.onBusyChange = (b) => {
  busy.hidden = !b;
  if (!b) {
    progress.hidden = true;
    phase = '';
  }
};
store.api.onProgress = (f) => {
  progress.hidden = false;
  progress.textContent = f < 0 ? 'Parsing…' : `${phase || 'Reading'} ${Math.round(f * 100)}%`;
};
store.api.onStatus = (text) => {
  phase = text.replace(/…$/, '');
  progress.hidden = false;
  progress.textContent = text;
};
store.api.onNotice = (text) => toast(text, 'error');
renderChrome();
renderView();

// --- Opening files -------------------------------------------------------------

async function openFile(file: File) {
  // A patch is applied to what is open.
  if (store.file && (PATCH_FILE.test(file.name) || (await startsWith(file, 'PATCH')) || (await startsWith(file, 'UPS1')) || (await startsWith(file, 'BPS1')))) {
    await store.applyPatch(file.name, file);
    return;
  }
  // A ROM's code/data log and label files go with it.
  if (store.file?.summary.format === 'rom') {
    if (/\.cdl$/i.test(file.name) || (await startsWith(file, 'CDLv2'))) {
      await loadCodeLog(file);
      return;
    }
    if (LABEL_FILE.test(file.name)) {
      await importLabels(file);
      return;
    }
  }
  // A zip is a folder: every binary in it opens.
  if (await isZip(file)) {
    await openFolder([{ kind: 'zip', name: file.name, blob: file }]);
    return;
  }
  // A crash report is symbolicated with what is open.
  const report = await crashText(file);
  if (report) {
    await store.openCrash(file.name, report);
    return;
  }
  // A debug file goes with what is open (the folder's binary it pairs with, or the open binary).
  if (store.file) {
    const header = await store.api.sniff(file);
    if (header?.kind === 'debug') {
      if (store.package) await openFolder([{ kind: 'folder', name: file.name, files: [{ path: file.name, file }] }]);
      else await store.attachDebug(file.name, file);
      return;
    }
  }
  // The File stays on disk: the worker reads it in chunks, the hex view reads what it shows.
  await store.open(file.name, file);
}

/**
 * Opens every binary in folders and zips. With none in them, a folder dropped
 * next to an open binary is its source code instead (`files`).
 */
async function openFolder(sources: PackageSource[], files: { path: string; file: File }[] = [], quiet = false) {
  if (await store.openFolder(sources)) return;
  const name = sources.map((s) => s.name).join(' + ');
  if (files.length > 0 && store.file?.dwarf) await loadSources(files.filter((f) => !SKIP_DIRS.test(f.path) && f.file.size < 8 * 1024 * 1024));
  else if (!quiet) toast(`No binaries in ${name}`, 'error');
}

/** A crash report's text, if the file is one (it is text, and reads as a report). */
async function crashText(file: Blob): Promise<string | null> {
  if (file.size < 20 || file.size > 64 * 1024 * 1024) return null;
  const head = new Uint8Array(await file.slice(0, 4096).arrayBuffer());
  if (head.includes(0)) return null;
  const text = await file.text();
  return (await store.api.crashParse(text)) ? text : null;
}

/** Crash reports found in a dropped folder go with the binaries next to them. */
const CRASH_FILE = /(\.crash|\.ips|(^|\/)tombstone[^/]*)$/i;

async function startsWith(blob: Blob, magic: string): Promise<boolean> {
  const b = new Uint8Array(await blob.slice(0, magic.length).arrayBuffer());
  return b.length === magic.length && [...magic].every((c, i) => b[i] === c.charCodeAt(0));
}

async function isZip(blob: Blob): Promise<boolean> {
  const b = new Uint8Array(await blob.slice(0, 4).arrayBuffer());
  return b.length === 4 && b[0] === 0x50 && b[1] === 0x4b && ((b[2] === 3 && b[3] === 4) || (b[2] === 5 && b[3] === 6));
}

/** Opens a bundled sample together with its source files, so the source ↔ code mapping shows right away. */
async function openSample(file: string) {
  const sample = SAMPLES.find((s) => s.file === file);
  if (!sample) throw new Error(`unknown sample ${file}`);
  const res = await fetch(`samples/${file}`);
  if (!res.ok) throw new Error(`sample ${file} not found (run npm run wasm to copy the fixtures)`);
  if (sample.folder) {
    await store.openFolder([{ kind: 'zip', name: file, blob: await res.blob() }], { sample: file });
    return;
  }
  await store.open(file, await res.blob(), { sample: file, reopen: () => openSample(file) });
  const sources: { path: string; file: File }[] = [];
  for (const src of sample.sources) {
    const r = await fetch(`samples/src/${src}`);
    if (r.ok) sources.push({ path: src, file: new File([await r.blob()], src) });
  }
  if (sources.length > 0 && store.file?.dwarf) await store.loadSources(sources);
}

/**
 * What the URL says: `#sample=shapes-pe.exe&view=code&goto=total_area`.
 * binviz writes it as you go (Back and Forward are the browser's), and a
 * link or a reload reads it: `sample` opens a bundled sample (`bin`, one of
 * its binaries); `goto` takes anything the Go to box does (address,
 * @offset, symbol, file:line); `tab` is a view's tab; `die` a DIE
 * (unit:offset, or a name), `line` a source line (file:line), `name` a
 * source file or unit; `search` opens the search box; `theme` forces light
 * or dark.
 */
async function applyHash() {
  const params = new URLSearchParams(location.hash.slice(1));
  const theme = params.get('theme');
  if (theme === 'light' || theme === 'dark') {
    themeOverride = theme;
    applyTheme();
  }
  const sample = params.get('sample');
  if (sample && store.sample !== sample) await openSample(sample);
  const bin = params.get('bin');
  const pkg = store.package;
  if (bin && pkg) {
    const i = pkg.info.binaries.findIndex((b) => b.path === bin);
    if (i >= 0 && i !== pkg.current) await store.selectBinary(i);
  }
  if (!store.file) return;
  let viewParam = params.get('view');
  let state = params.get('tab') ?? undefined;
  let name = params.get('name') ?? undefined;
  // Views that became tabs of others.
  if (viewParam === 'sections') {
    viewParam = 'layout';
    state = 'sections';
  } else if (viewParam === 'map') {
    name ??= params.get('file') ?? undefined;
    if (state === 'coverage') viewParam = 'layout';
    else {
      viewParam = 'sources';
      state = state === 'units' ? 'units' : 'files';
    }
  }
  const view = NAV.find((n) => n.view === viewParam)?.view;
  let target: Target | undefined;
  const goto = params.get('goto');
  if (goto) {
    const r = await store.api.resolve(goto);
    target = r.kind === 'address' ? { address: r.value } : { offset: r.value };
  }
  const intent: Intent = {};
  const die = params.get('die');
  if (die && store.file.dwarf) {
    const at = /^(\d+):(0x[0-9a-f]+)$/i.exec(die);
    if (at) intent.die = { unit: Number(at[1]), offset: BigInt(at[2]) };
    else {
      const found = await store.api.dieSearch(die, 50);
      const match = found.find((d) => d.name === die) ?? found[0];
      if (match) intent.die = { unit: match.unit, offset: match.offset };
      else toast(`No DIE named ${die}`, 'error');
    }
  }
  const line = /^(\d+):(\d+)$/.exec(params.get('line') ?? '');
  if (line) intent.source = { file: Number(line[1]), line: Number(line[2]) };
  if (name) intent.contributor = name;
  const shown = intent.die ? 'dwarf' : intent.source || intent.contributor ? 'sources' : view;
  if (shown || target) await store.goTo({ view: shown, state, target, intent }, 'replace');
  // `search=<query>` opens the search box with results.
  const search = params.get('search');
  if (search) palette.focus(search);
}

/** The fragment last shown: the browser reports it twice (popstate, then hashchange). */
let shownHash = location.hash;
window.addEventListener('popstate', (e) => {
  shownHash = location.hash;
  void (async () => {
    // One of binviz's places: shown again. Otherwise the URL says where to go.
    if (!(await store.popped(e.state))) await applyHash();
  })().catch((err) => toast(String(err instanceof Error ? err.message : err), 'error'));
});
window.addEventListener('hashchange', () => {
  if (location.hash === shownHash) return;
  shownHash = location.hash;
  store.adoptEntry();
  applyHash().catch((e) => toast(String(e instanceof Error ? e.message : e), 'error'));
});

fileInput.addEventListener('change', () => {
  const f = fileInput.files?.[0];
  if (f) void openFile(f);
  fileInput.value = '';
});

debugInput.addEventListener('change', async () => {
  const f = debugInput.files?.[0];
  debugInput.value = '';
  if (!f) return;
  await store.attachDebug(f.name, f);
  if (store.file?.dwarf) toast(`Loaded DWARF from ${f.name}: ${store.file.dwarf.unitCount} units`);
});

packageInput.addEventListener('change', async () => {
  const files = [...(packageInput.files ?? [])].map((file) => ({ path: (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name, file }));
  packageInput.value = '';
  if (files.length === 0) return;
  const name = files[0].path.split('/')[0] || 'folder';
  const kept = files.filter((f) => !IGNORED.test(f.path));
  await openFolder([{ kind: 'folder', name, files: kept }], kept);
});

sourcesInput.addEventListener('change', async () => {
  const files = [...(sourcesInput.files ?? [])].filter(isSourceFile).map((file) => ({ path: (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name, file }));
  sourcesInput.value = '';
  await loadSources(files);
});

async function loadSources(files: { path: string; file: File }[]) {
  if (!store.file?.dwarf) {
    toast('Load a binary with DWARF first, then its sources.', 'error');
    return;
  }
  const matched = await store.loadSources(files);
  toast(matched ? `Matched ${matched} of ${store.file.sourceFiles.length} source files` : `None of the ${files.length} files matched the paths in the debug info`, matched ? 'info' : 'error');
}

const SKIP_DIRS = /(^|\/)(\.git|node_modules|target|build|dist|\.venv|__pycache__)(\/|$)/;
function isSourceFile(f: File): boolean {
  const path = (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name;
  return !SKIP_DIRS.test(path) && f.size < 8 * 1024 * 1024;
}

window.addEventListener('binviz:attach-debug', () => debugInput.click());
window.addEventListener('binviz:load-sources', () => sourcesInput.click());

// Drag and drop: one file opens as a binary (a debug file goes with what is
// open); folders, zips and several files open as a folder of binaries.
let dragDepth = 0;
window.addEventListener('dragenter', (e) => {
  if (!e.dataTransfer?.types.includes('Files')) return;
  dragDepth++;
  document.body.classList.add('dragging');
});
window.addEventListener('dragleave', () => {
  dragDepth = Math.max(0, dragDepth - 1);
  if (dragDepth === 0) document.body.classList.remove('dragging');
});
window.addEventListener('dragover', (e) => e.preventDefault());
window.addEventListener('drop', async (e) => {
  e.preventDefault();
  dragDepth = 0;
  document.body.classList.remove('dragging');
  // Both lists are only readable until the handler first awaits.
  const items = [...(e.dataTransfer?.items ?? [])];
  const entries = items.map((i) => i.webkitGetAsEntry?.()).filter((x): x is FileSystemEntry => !!x);
  const dropped = [...(e.dataTransfer?.files ?? [])];
  try {
    // On the Compare view, waiting for the earlier build: this is it.
    if (store.view === 'diff' && !store.diff && (store.file || store.package)) {
      await compareDropped(entries, dropped);
      return;
    }
    await openDropped(entries, dropped);
  } catch (err) {
    toast(err instanceof Error ? err.message : String(err), 'error');
  }
});

/** Version control, dependencies and Finder litter: never walked into. */
const IGNORED = /(^|\/)(__MACOSX|\.git|node_modules|\.DS_Store)(\/|$)/;

async function openDropped(entries: FileSystemEntry[], dropped: File[]) {
  if (!entries.some((en) => en.isDirectory) && dropped.length <= 1) {
    if (dropped[0]) await openFile(dropped[0]);
    return;
  }
  // Zips stay zips; crash reports are symbolicated once the rest is open;
  // everything else (folders walked in full) is one folder.
  const sources: PackageSource[] = [];
  const files: { path: string; file: File }[] = [];
  const reports: { name: string; text: string }[] = [];
  const loose: File[] = entries.length > 0 ? [] : [...dropped];
  for (const en of entries) {
    if (en.isDirectory) await walkAll(en, files);
    else {
      const f = dropped.find((x) => x.name === en.name);
      if (f) loose.push(f);
    }
  }
  for (const f of loose) {
    const report = await crashText(f);
    if (report) reports.push({ name: f.name, text: report });
    else if (await isZip(f)) sources.push({ kind: 'zip', name: f.name, blob: f });
    else files.push({ path: f.name, file: f });
  }
  for (const f of files.filter((f) => CRASH_FILE.test(f.path))) {
    const report = await crashText(f.file);
    if (report) reports.push({ name: basename(f.path), text: report });
  }
  const folders = entries.filter((en) => en.isDirectory).map((en) => en.name);
  if (files.length > 0) sources.unshift({ kind: 'folder', name: folders.join(' + ') || files[0].path, files });
  if (sources.length > 0) await openFolder(sources, files, reports.length > 0);
  if (reports.length > 0) await store.openCrash(reports[0].name, reports[0].text);
}

// Pasting a crash report anywhere (but into a text field) symbolicates it.
window.addEventListener('paste', async (e) => {
  const target = e.target as HTMLElement;
  if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
  const text = e.clipboardData?.getData('text/plain') ?? '';
  if (text.length < 20) return;
  if (await store.api.crashParse(text)) await store.openCrash('Pasted crash report', text);
  else toast('The pasted text isn’t a crash report binviz can read (Apple .crash or .ips, an Android tombstone, a stack trace)', 'error');
});

/** The earlier build to compare sizes with, dropped on the Compare view. */
async function compareDropped(entries: FileSystemEntry[], dropped: File[]) {
  if (!entries.some((en) => en.isDirectory) && dropped.length === 1) {
    const f = dropped[0];
    await store.compareWith(f.name, (await isZip(f)) ? { kind: 'folder', sources: [{ kind: 'zip', name: f.name, blob: f }] } : { kind: 'file', name: f.name, blob: f });
    return;
  }
  const sources: PackageSource[] = [];
  const files: { path: string; file: File }[] = [];
  for (const en of entries) {
    if (en.isDirectory) await walkAll(en, files);
    else {
      const f = dropped.find((x) => x.name === en.name);
      if (f && (await isZip(f))) sources.push({ kind: 'zip', name: f.name, blob: f });
      else if (f) files.push({ path: f.name, file: f });
    }
  }
  const name = entries.map((en) => en.name).join(' + ') || 'folder';
  if (files.length > 0) sources.unshift({ kind: 'folder', name, files });
  await store.compareWith(name, { kind: 'folder', sources });
}

/** Every file under `entry` (up to 500 000 of them). */
async function walkAll(entry: FileSystemEntry, out: { path: string; file: File }[]) {
  const path = entry.fullPath.replace(/^\//, '');
  if (IGNORED.test(path) || out.length >= 500_000) return;
  if (entry.isFile) {
    out.push({ path, file: await new Promise<File>((res, rej) => (entry as FileSystemFileEntry).file(res, rej)) });
    return;
  }
  const reader = (entry as FileSystemDirectoryEntry).createReader();
  for (;;) {
    const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
    if (batch.length === 0) break;
    for (const child of batch) await walkAll(child, out);
  }
}

// --- Keyboard -------------------------------------------------------------------

window.addEventListener('keydown', (e) => {
  const target = e.target as HTMLElement;
  const typing = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT';
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') {
    e.preventDefault();
    fileInput.click();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === 'k' || e.key.toLowerCase() === 'g')) {
    e.preventDefault();
    palette.focus();
    return;
  }
  if (e.altKey && e.key === 'ArrowLeft') {
    e.preventDefault();
    store.back();
    return;
  }
  if (e.altKey && e.key === 'ArrowRight') {
    e.preventDefault();
    store.forward();
    return;
  }
  if (typing || e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.key === 'Escape') {
    (document.activeElement as HTMLElement | null)?.blur?.();
    return;
  }
  if (e.key === '/') {
    e.preventDefault();
    palette.focus();
    return;
  }
  if (e.key === 'n' && store.file) {
    e.preventDefault();
    inspector.startNote();
    return;
  }
  const nav = NAV.find((n) => n.key === e.key);
  const ready = nav && (nav.view === 'folder' ? store.package : nav.view === 'crash' ? store.crash : nav.view === 'diff' ? store.file || store.package : nav.view === 'patch' ? store.patch : store.file);
  if (nav && ready) store.setView(nav.view);
});

// A handle for poking at the app from the devtools console during development.
if (import.meta.env.DEV) (window as unknown as { binviz: unknown }).binviz = { store, palette, inspector };

// Honour a deep link in the initial URL.
void applyHash().catch((e) => toast(String(e instanceof Error ? e.message : e), 'error'));
