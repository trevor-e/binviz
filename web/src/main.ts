import './styles.css';
import { Inspector } from './inspector';
import { SearchPalette } from './palette';
import { store, type MapTab, type ViewName } from './store';
import { toast } from './ui';
import { basename, formatSize, h, icon } from './util';
import type { View } from './views/base';
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
const SAMPLES: { file: string; label: string; sources: string[] }[] = [
  { file: 'tiny-elf-x64', label: 'ELF · x86-64', sources: ['tiny.rs'] },
  { file: 'tiny-elf-a64', label: 'ELF · AArch64', sources: ['tiny.rs'] },
  { file: 'tiny-macho-a64', label: 'Mach-O · arm64', sources: ['tiny.rs'] },
  { file: 'tiny-macho-a64.o', label: 'Mach-O object', sources: ['tiny.rs'] },
  { file: 'tiny-pe-x64.exe', label: 'PE · x86-64', sources: ['tiny.rs'] },
  { file: 'shapes-pe.exe', label: 'PE · C++ with DWARF 5', sources: ['shapes.cpp'] },
  { file: 'shapes-pe.stripped.exe', label: 'PE · stripped (reverse engineering)', sources: [] },
];

const NAV: { view: ViewName; label: string; key: string }[] = [
  { view: 'overview', label: 'Overview', key: '1' },
  { view: 'layout', label: 'Layout', key: '2' },
  { view: 'hex', label: 'Hex', key: '3' },
  { view: 'code', label: 'Code', key: '4' },
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
  overview: new OverviewView(),
  layout: new LayoutView(),
  hex: new HexView(),
  code: new CodeView(),
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

const fileInfo = h('div', { class: 'fileinfo' });
const busy = h('div', { class: 'busy' });
busy.hidden = true;

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
  fileInput,
  debugInput,
  sourcesInput,
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
document.body.appendChild(h('div', { class: 'drop-overlay' }, 'Drop a binary to open it, or a folder to load sources'));

// --- Rendering state ----------------------------------------------------------

function renderChrome() {
  const f = store.file;
  app.classList.toggle('empty', !f);
  landing.style.display = f ? 'none' : 'grid';
  debugBtn.toggleAttribute('disabled', !f);
  sourcesBtn.toggleAttribute('disabled', !f?.dwarf);
  palette.input.disabled = !f;
  if (!f) {
    fileInfo.replaceChildren();
    renderLanding();
    return;
  }
  const s = f.summary;
  fileInfo.replaceChildren(
    h('span', { class: 'name', title: f.name }, basename(f.name)),
    h('span', { class: 'chip' }, s.formatName),
    h('span', { class: 'chip' }, s.arch),
    h('span', { class: 'chip' }, s.kind),
    h('span', { class: 'chip' }, formatSize(s.fileSize)),
    h('span', { class: `chip ${s.hasDwarf ? 'ok' : 'off'}`, title: s.hasDwarf ? 'DWARF debug info loaded' : 'No DWARF debug info' }, s.hasDwarf ? 'DWARF' : 'no DWARF'),
  );
}

function renderView() {
  for (const [name, v] of Object.entries(views) as [ViewName, View][]) {
    const active = name === store.view && !!store.file;
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
      h('div', { class: 'dropzone' }, h('div', { class: 'big' }, 'Drop a binary here'), h('div', { class: 'secondary', style: 'margin-bottom:14px' }, 'executables, shared libraries, object files, .dSYM DWARF files, universal binaries and archives'), choose),
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
store.api.onBusyChange = (b) => (busy.hidden = !b);
renderChrome();
renderView();

// --- Opening files -------------------------------------------------------------

async function openFile(file: File) {
  await store.open(file.name, new Uint8Array(await file.arrayBuffer()));
}

/** Opens a bundled sample together with its source files, so the source ↔ code mapping shows right away. */
async function openSample(file: string) {
  const sample = SAMPLES.find((s) => s.file === file);
  if (!sample) throw new Error(`unknown sample ${file}`);
  const res = await fetch(`samples/${file}`);
  if (!res.ok) throw new Error(`sample ${file} not found (run npm run wasm to copy the fixtures)`);
  await store.open(file, new Uint8Array(await res.arrayBuffer()));
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
  if (sample && store.file?.name !== sample) await openSample(sample);
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
  await store.attachDebug(f.name, new Uint8Array(await f.arrayBuffer()));
  if (store.file?.dwarf) toast(`Loaded DWARF from ${f.name}: ${store.file.dwarf.unitCount} units`);
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

// Drag and drop: a file opens as a binary; a folder loads sources.
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
  const items = [...(e.dataTransfer?.items ?? [])];
  const entries = items.map((i) => i.webkitGetAsEntry?.()).filter((x): x is FileSystemEntry => !!x);
  if (entries.some((en) => en.isDirectory)) {
    const files: { path: string; file: File }[] = [];
    for (const en of entries) await walk(en, files);
    await loadSources(files);
    return;
  }
  const file = e.dataTransfer?.files?.[0];
  if (file) await openFile(file);
});

async function walk(entry: FileSystemEntry, out: { path: string; file: File }[]) {
  if (out.length > 20000 || SKIP_DIRS.test(entry.fullPath)) return;
  if (entry.isFile) {
    const file = await new Promise<File>((res, rej) => (entry as FileSystemFileEntry).file(res, rej));
    if (file.size < 8 * 1024 * 1024) out.push({ path: entry.fullPath.replace(/^\//, ''), file });
    return;
  }
  const reader = (entry as FileSystemDirectoryEntry).createReader();
  for (;;) {
    const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
    if (batch.length === 0) break;
    for (const child of batch) await walk(child, out);
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
  if (nav && store.file) store.setView(nav.view);
});

// A handle for poking at the app from the devtools console during development.
if (import.meta.env.DEV) (window as unknown as { binviz: unknown }).binviz = { store, palette, inspector };

// Honour a deep link in the initial URL.
void applyHash().catch((e) => toast(String(e instanceof Error ? e.message : e), 'error'));
