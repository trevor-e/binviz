import './styles.css';
import { Inspector } from './inspector';
import { SearchPalette } from './palette';
import { store, type MapTab, type ViewName } from './store';
import { toast } from './ui';
import { basename, formatSize, h, icon } from './util';
import type { PackageSource } from './types';
import { FolderView, KIND_GROUPS } from './views/folder';
import type { View } from './views/base';
import { CallsView } from './views/calls';
import { CodeView } from './views/code';
import { DwarfView } from './views/dwarf';
import { HexView } from './views/hex';
import { LayoutView } from './views/layout';
import { MapView } from './views/map';
import { OverviewView } from './views/overview';
import { SectionsView } from './views/sections';
import { SourcesView } from './views/sources';
import { SymbolsView } from './views/symbols';

/** Bundled samples (copied from tests/fixtures by scripts/build-wasm.mjs) and their sources. */
const SAMPLES: { file: string; label: string; sources: string[]; folder?: boolean }[] = [
  { file: 'tiny-elf-x64', label: 'ELF · x86-64', sources: ['tiny.rs'] },
  { file: 'tiny-elf-a64', label: 'ELF · AArch64', sources: ['tiny.rs'] },
  { file: 'tiny-macho-a64', label: 'Mach-O · arm64', sources: ['tiny.rs'] },
  { file: 'tiny-macho-a64.o', label: 'Mach-O object', sources: ['tiny.rs'] },
  { file: 'tiny-pe-x64.exe', label: 'PE · x86-64', sources: ['tiny.rs'] },
  { file: 'shapes-pe.exe', label: 'PE · C++ with DWARF 5', sources: ['shapes.cpp'] },
  { file: 'shapes-pe.stripped.exe', label: 'PE · stripped (reverse engineering)', sources: [] },
  { file: 'Shop.xcarchive.zip', label: 'Zipped iOS app archive (3 binaries + dSYM)', sources: [], folder: true },
];

const NAV: { view: ViewName; label: string; key: string }[] = [
  { view: 'folder', label: 'Folder', key: 'f' },
  { view: 'overview', label: 'Overview', key: '1' },
  { view: 'layout', label: 'Layout', key: '2' },
  { view: 'hex', label: 'Hex', key: '3' },
  { view: 'code', label: 'Code', key: '4' },
  { view: 'calls', label: 'Call graph', key: '0' },
  { view: 'symbols', label: 'Symbols', key: '5' },
  { view: 'sections', label: 'Sections', key: '6' },
  { view: 'dwarf', label: 'DWARF', key: '7' },
  { view: 'sources', label: 'Sources', key: '8' },
  { view: 'map', label: 'Map', key: '9' },
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
  overview: new OverviewView(),
  layout: new LayoutView(),
  hex: new HexView(),
  code: new CodeView(),
  calls: new CallsView(),
  symbols: new SymbolsView(),
  sections: new SectionsView(),
  dwarf: new DwarfView(),
  sources: new SourcesView(),
  map: new MapView(),
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
const debugBtn = button('Debug file', 'Load DWARF from a separate file (.dSYM DWARF, .debug, unstripped copy)', 'debug', () => debugInput.click());
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
const back = button('', 'Back (Alt+←)', 'back', () => void store.back(), 'ghost small icon-only');
const forward = button('', 'Forward (Alt+→)', 'forward', () => void store.forward(), 'ghost small icon-only');
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
document.body.appendChild(h('div', { class: 'drop-overlay' }, 'Drop a binary, or a folder or zip of them'));

// --- Rendering state ----------------------------------------------------------

function renderChrome() {
  const f = store.file;
  const pkg = store.package;
  app.classList.toggle('empty', !f && !pkg);
  landing.style.display = f || pkg ? 'none' : 'grid';
  debugBtn.toggleAttribute('disabled', !f);
  sourcesBtn.toggleAttribute('disabled', !f?.dwarf);
  palette.input.disabled = !f;
  for (const [name, b] of navButtons) {
    if (name === 'folder') b.hidden = !pkg;
    else b.toggleAttribute('disabled', !f);
  }
  if (!f) {
    fileInfo.replaceChildren(...(pkg ? [h('span', { class: 'name', title: pkg.info.name }, basename(pkg.info.name))] : []));
    if (!pkg) renderLanding();
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
        h('div', { class: 'secondary', style: 'margin-bottom:14px' }, 'executables, shared libraries, object files, debug files (.dSYM, .debug), universal binaries and archives; a folder or zip (an .ipa, an .app, a build) opens every binary in it, each paired with its debug file'),
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
store.on('view', () => renderView());
store.on('selection', () => {
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
  // A zip is a folder: every binary in it opens.
  if (await isZip(file)) {
    await openFolder([{ kind: 'zip', name: file.name, blob: file }]);
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
async function openFolder(sources: PackageSource[], files: { path: string; file: File }[] = []) {
  if (await store.openFolder(sources)) return;
  const name = sources.map((s) => s.name).join(' + ');
  if (files.length > 0 && store.file?.dwarf) await loadSources(files.filter((f) => !SKIP_DIRS.test(f.path) && f.file.size < 8 * 1024 * 1024));
  else toast(`No binaries in ${name}`, 'error');
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
    await store.openFolder([{ kind: 'zip', name: file, blob: await res.blob() }]);
    return;
  }
  await store.open(file, await res.blob());
  const sources: { path: string; file: File }[] = [];
  for (const src of sample.sources) {
    const r = await fetch(`samples/src/${src}`);
    if (r.ok) sources.push({ path: src, file: new File([await r.blob()], src) });
  }
  if (sources.length > 0 && store.file?.dwarf) await store.loadSources(sources);
}

/**
 * Deep links: `#sample=shapes-pe.exe&goto=total_area&view=code&theme=dark`.
 * `goto` takes anything the Go to box does (address, @offset, symbol, file:line).
 */
async function applyHash() {
  const params = new URLSearchParams(location.hash.slice(1));
  const theme = params.get('theme');
  if (theme === 'light' || theme === 'dark') {
    themeOverride = theme;
    applyTheme();
  }
  const sample = params.get('sample');
  if (sample && store.file?.name !== sample && store.package?.info.name !== sample) await openSample(sample);
  const view = NAV.find((n) => n.view === params.get('view'))?.view;
  const tab = params.get('tab');
  if (view === 'map' && store.file && (tab === 'files' || tab === 'units' || tab === 'coverage')) {
    store.openMap(tab as MapTab, params.get('file') ?? undefined);
    return;
  }
  const goto = params.get('goto');
  if (goto && store.file) {
    const r = await store.api.resolve(goto);
    await store.select(r.kind === 'address' ? { address: r.value } : { offset: r.value }, { view });
  } else if (view && store.file) {
    store.setView(view);
  }
  // `search=<query>` opens the search box with results.
  const search = params.get('search');
  if (search && store.file) palette.focus(search);
  // `die=<name>` opens the DWARF view on the first DIE with that name.
  const die = params.get('die');
  if (die && store.file?.dwarf) {
    const found = await store.api.dieSearch(die, 50);
    const match = found.find((d) => d.name === die) ?? found[0];
    if (match) store.openDie(match.unit, match.offset);
    else toast(`No DIE named ${die}`, 'error');
  }
}
window.addEventListener('hashchange', () => applyHash().catch((e) => toast(String(e instanceof Error ? e.message : e), 'error')));

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
  // Zips stay zips; everything else (folders walked in full) is one folder.
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
  if (entries.length === 0) for (const f of dropped) files.push({ path: f.name, file: f });
  const folders = entries.filter((en) => en.isDirectory).map((en) => en.name);
  if (files.length > 0) sources.unshift({ kind: 'folder', name: folders.join(' + ') || files[0].path, files });
  await openFolder(sources, files);
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
    void store.back();
    return;
  }
  if (e.altKey && e.key === 'ArrowRight') {
    e.preventDefault();
    void store.forward();
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
  if (nav && (nav.view === 'folder' ? store.package : store.file)) store.setView(nav.view);
});

// A handle for poking at the app from the devtools console during development.
if (import.meta.env.DEV) (window as unknown as { binviz: unknown }).binviz = { store, palette, inspector };

// Honour a deep link in the initial URL.
void applyHash().catch((e) => toast(String(e instanceof Error ? e.message : e), 'error'));
