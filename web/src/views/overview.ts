// Overview: identity, byte composition, file map and the file ↔ memory diagram.
import { FAMILIES, KIND_LABELS, STATUS_LABELS, entropyColor, entropyRamp, familyColors, familyOf, statusColors, type Family } from '../colors';
import { store } from '../store';
import type { LabelFormat, LogSummary, MapStatus, RegionInfo, RegionKind } from '../types';
import { downloadText, emptyState, legend, swatch, toast, tooltip } from '../ui';
import { basename, debounce, fmtAddr, formatCount, formatSize, h, hex, num, percent } from '../util';
import { View } from './base';
import { statusLegend } from './coverage';

type MapMode = 'regions' | 'entropy' | 'coverage';
const MAP_MODES: { mode: MapMode; label: string; title: string }[] = [
  { mode: 'regions', label: 'Regions', title: 'What each part of the file is' },
  { mode: 'entropy', label: 'Entropy', title: 'How random the bytes are: compressed or encrypted data stands out' },
  { mode: 'coverage', label: 'Coverage', title: 'How much is mapped out while reverse engineering: named, recovered, still unexplored' },
];

export class OverviewView extends View {
  private mapMode: MapMode = 'regions';
  private kinds: RegionKind[] = [];
  private entropy: Float32Array = new Float32Array();
  /** Coverage per cell, read when first shown (and again when notes change). */
  private statuses: MapStatus[] = [];
  private buckets = 0;
  private canvas?: HTMLCanvasElement;
  private hover = -1;

  constructor() {
    super('overview');
    window.addEventListener('themechange', () => this.visible && this.drawMap());
    window.addEventListener(
      'resize',
      debounce(() => this.visible && this.canvas && this.loadMap(), 200),
    );
    store.on('annotations', () => {
      this.statuses = [];
      if (this.visible && this.mapMode === 'coverage') void this.loadCoverage();
    });
  }

  protected render() {
    const mapping = h('div', { class: 'mapping' });
    const page = h('div', { class: 'page' });
    const rom = store.file!.summary.format === 'rom';
    page.append(this.identity(), h('div', { class: 'grid-2' }, this.composition(), rom ? this.romCard() : this.dwarfCard()), this.fileMapCard(), this.mappingCard(mapping));
    this.el.replaceChildren(page);
    void this.loadMap();
    // Measure the diagram's width only once it is in the document.
    requestAnimationFrame(() => void this.drawMapping(mapping));
  }

  protected onSelection() {
    this.drawMap();
  }

  private identity(): HTMLElement {
    const f = store.file!;
    const s = f.summary;
    const facts: [string, Node | string][] = [
      ['Format', `${s.formatName} (${s.littleEndian ? 'little' : 'big'}-endian)`],
      ['Kind', s.kind],
      ['Architecture', `${s.arch}, ${s.bits}-bit`],
      ['Size', `${formatSize(s.fileSize)} (${formatCount(num(s.fileSize))} bytes)`],
    ];
    if (s.entry !== undefined) {
      const entry = s.entry;
      const link = h('span', { class: 'link mono' }, fmtAddr(entry));
      link.addEventListener('click', () => void store.select({ address: entry }, { view: 'code' }));
      facts.push(['Entry point', link]);
    }
    if (s.imageBase !== undefined) facts.push(['Image base', h('span', { class: 'mono' }, hex(s.imageBase))]);
    // A ROM's CRC32 is among its properties.
    if (s.buildId && s.format !== 'rom') facts.push([s.format === 'mach-o' ? 'UUID' : s.format === 'pe' ? 'PDB signature' : 'Build ID', h('span', { class: 'mono' }, s.buildId)]);
    if (s.debugLink) facts.push([s.format === 'pe' ? 'PDB path' : 'Debug link', h('span', { class: 'mono' }, s.debugLink)]);
    facts.push(['Contents', `${s.sectionCount} sections · ${s.segmentCount} segments · ${formatCount(s.symbolCount)} symbols`]);
    if (s.syntheticAddresses) facts.push(['Addresses', 'Relocatable object: its sections are laid out at synthetic addresses, with relocations applied to the DWARF']);
    for (const p of s.properties) facts.push([p.key, p.value]);
    return h(
      'div',
      { class: 'card' },
      h('h2', { style: 'font-size:18px' }, basename(f.name)),
      h('p', { class: 'sub' }, s.format === 'rom' ? `${s.kind} (${s.formatName}) for the ${s.arch}` : `${s.formatName} ${s.kind.toLowerCase()} for ${s.arch}`),
      h('dl', { class: 'facts' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
    );
  }

  private composition(): HTMLElement {
    const f = store.file!;
    const total = num(f.summary.fileSize);
    const byFamily = new Map<Family, number>();
    for (const [kind, bytes] of f.composition) byFamily.set(familyOf(kind), (byFamily.get(familyOf(kind)) ?? 0) + num(bytes));
    const fams = FAMILIES.filter((x) => (byFamily.get(x.id) ?? 0) > 0);
    const bar = h('div', { class: 'stack-bar', role: 'img', 'aria-label': 'File composition by region family' });
    for (const fam of fams) {
      const bytes = byFamily.get(fam.id)!;
      const seg = h('div', { class: `fam-${fam.id}`, style: `flex:${bytes} 1 0` });
      seg.addEventListener('pointermove', (e) =>
        tooltip.show(e.clientX, e.clientY, h('div', { class: 't-value' }, `${formatSize(bytes)} · ${percent(bytes, total)}`), h('div', null, fam.label)),
      );
      seg.addEventListener('pointerleave', () => tooltip.hide());
      bar.appendChild(seg);
    }
    const rows = [...f.composition].map(([kind, bytes]) =>
      h(
        'tr',
        null,
        h('td', null, swatch(kind), KIND_LABELS[kind]),
        h('td', { class: 'right' }, formatSize(bytes)),
        h('td', { class: 'right muted' }, percent(num(bytes), total)),
      ),
    );
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'What the file is made of'),
      h('p', { class: 'sub' }, 'Every byte is attributed to the structure that owns it'),
      bar,
      h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Region kind'), h('th', { class: 'right' }, 'Bytes'), h('th', { class: 'right' }, 'Share'))), h('tbody', null, rows)),
    );
  }

  /** A ROM's code: how it was found, and what emulators add (a code/data log, label files). */
  private romCard(): HTMLElement {
    const s = store.file!.summary;
    const found = s.properties.find((p) => p.key === 'Code found')?.value ?? '';
    const open = h('button', { class: 'btn small' }, 'Open the code');
    open.addEventListener('click', () => store.setView('code'));
    const logInput = h('input', { type: 'file', accept: '.cdl,application/octet-stream', style: 'display:none' });
    logInput.addEventListener('change', async () => {
      const file = logInput.files?.[0];
      logInput.value = '';
      if (file) await loadCodeLog(file);
    });
    const loadLog = h('button', { class: 'btn small', type: 'button', title: 'A .cdl file from FCEUX’s or Mesen’s code/data logger' }, store.codeLog ? 'Load another log…' : 'Load code/data log…');
    loadLog.addEventListener('click', () => logInput.click());
    const labelInput = h('input', { type: 'file', accept: '.mlb,.nl,.sym,.txt', style: 'display:none' });
    labelInput.addEventListener('change', async () => {
      const file = labelInput.files?.[0];
      labelInput.value = '';
      if (file) await importLabels(file);
    });
    const importBtn = h('button', { class: 'btn small', type: 'button', title: 'Mesen’s .mlb, FCEUX’s .nl, a .sym (RGBDS, WLA DX, no$gba): the names become your notes' }, 'Import labels…');
    importBtn.addEventListener('click', () => labelInput.click());
    const exports = h('span', { class: 'btn-row' });
    void store.api.labelFormats().then((formats) => {
      for (const f of formats) {
        const b = h('button', { class: 'btn small', type: 'button', title: 'Your notes’ names and comments, for the emulator’s debugger' }, `Export ${LABEL_NAMES[f]}`);
        b.addEventListener('click', () => void exportLabels(f));
        exports.appendChild(b);
      }
    });
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Code'),
      h('p', { class: 'sub' }, found),
      h('p', { class: 'secondary' }, 'A ROM mixes code with graphics, tables and text. Code is followed from the reset and interrupt vectors, each call and branch in turn; what nothing reaches stays unexplored until you name it.'),
      open,
      h('h3', { class: 'card-sub' }, 'From emulators'),
      h('p', { class: 'secondary' }, 'Play the game with FCEUX’s or Mesen’s code/data logger on, then load its log (or drop the .cdl here): code only reached through jump tables is followed too, data the game read isn’t taken for code, and an NES game’s switched banks go where they ran.'),
      h('p', { class: 'sub' }, logLine(store.codeLog)),
      h('div', { class: 'btn-row' }, loadLog, importBtn, exports, logInput, labelInput),
    );
  }

  private dwarfCard(): HTMLElement {
    const f = store.file!;
    const d = f.dwarf;
    if (!d) {
      const btn = h('button', { class: 'btn' }, 'Add debug file…');
      btn.addEventListener('click', () => window.dispatchEvent(new CustomEvent('binviz:attach-debug')));
      const hint =
        f.summary.format === 'mach-o'
          ? 'On macOS, DWARF usually lives in a .dSYM bundle (…/Contents/Resources/DWARF/<name>) or in the object files listed by the debug map.'
          : f.summary.format === 'pe'
            ? f.summary.debugLink
              ? `Its debug info is in a PDB (${f.summary.debugLink}). Add it to see its functions, source lines and compilation units.`
              : 'No debug info. MSVC builds keep it in a PDB (this image names none); MinGW and the Rust gnu toolchain put DWARF in the binary.'
            : f.summary.debugLink
              ? `Debug info was split into ${f.summary.debugLink.split(' ')[0]}. Add it to see source mapping.`
              : 'No DWARF sections in this file. Rebuild with -g, or add a separate debug file.';
      return h('div', { class: 'card' }, h('h2', null, 'Debug information'), h('p', { class: 'sub' }, 'None found'), h('p', { class: 'secondary' }, hint), btn);
    }
    const debugBytes = d.sections.reduce((a, s) => a + num(s.size), 0);
    const facts: [string, string][] = [
      ['Source', d.source === 'embedded' ? 'Embedded in this file' : d.source],
      ['DWARF versions', d.versions.join(', ')],
      ['Compilation units', formatCount(d.unitCount)],
      ['Source files', formatCount(f.sourceFiles.length)],
      ['Languages', d.languages.join(', ') || '—'],
      ['Debug sections', `${d.sections.length} · ${formatSize(debugBytes)}`],
    ];
    if (d.splitUnits > 0) facts.push(['Split units', `${d.splitUnits} (.dwo files not loaded)`]);
    const open = (label: string, view: 'dwarf' | 'sources') => {
      const b = h('button', { class: 'btn small' }, label);
      b.addEventListener('click', () => store.setView(view));
      return b;
    };
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Debug information'),
      h('p', { class: 'sub' }, 'DWARF maps addresses to source lines, functions, types and variables'),
      h('dl', { class: 'facts' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
      d.producers.length
        ? h('details', { style: 'margin-top:8px' }, h('summary', { class: 'secondary' }, `Producers (${d.producers.length})`), h('ul', { class: 'mono', style: 'margin:6px 0;padding-left:18px' }, d.producers.slice(0, 20).map((p) => h('li', null, p))))
        : null,
      h('div', { style: 'display:flex;gap:6px;margin-top:10px' }, open('Browse DWARF', 'dwarf'), open('Browse sources', 'sources')),
    );
  }

  // --- File map ------------------------------------------------------------

  private fileMapCard(): HTMLElement {
    this.canvas = h('canvas', { role: 'img', 'aria-label': 'File map: each cell is a slice of the file, coloured by what it contains' });
    this.statuses = [];
    const toggle = h('div', { class: 'tabs' });
    for (const { mode, label, title } of MAP_MODES) {
      const b = h('button', { class: `tab${this.mapMode === mode ? ' active' : ''}`, type: 'button', title }, label);
      b.addEventListener('click', () => {
        this.mapMode = mode;
        toggle.querySelectorAll('.tab').forEach((t) => t.classList.toggle('active', t === b));
        keyHost.replaceChildren(this.mapKey());
        if (mode === 'coverage' && this.statuses.length === 0) void this.loadCoverage();
        this.drawMap();
      });
      toggle.appendChild(b);
    }
    const keyHost = h('div', null, this.mapKey());
    this.canvas.addEventListener('pointermove', (e) => this.mapHover(e));
    this.canvas.addEventListener('pointerleave', () => {
      this.hover = -1;
      tooltip.hide();
      this.drawMap();
    });
    this.canvas.addEventListener('click', () => {
      if (this.hover < 0) return;
      const [start] = this.bucketRange(this.hover);
      void store.select({ offset: BigInt(start) }, { view: 'hex' });
    });
    return h(
      'div',
      { class: 'card' },
      h('div', { style: 'display:flex;align-items:baseline;gap:12px;flex-wrap:wrap' }, h('div', { style: 'flex:1' }, h('h2', null, 'File map'), h('p', { class: 'sub' }, 'The whole file, left to right, top to bottom. Click a cell to open it in the hex view.')), toggle),
      keyHost,
      h('div', { class: 'filemap', style: 'margin-top:10px' }, this.canvas),
    );
  }

  private mapKey(): HTMLElement {
    if (this.mapMode === 'regions') {
      const present = new Set<Family>(store.file!.composition.map(([k]) => familyOf(k)));
      return legend(present);
    }
    if (this.mapMode === 'coverage') {
      const more = h('span', { class: 'link', style: 'font-size:12px' }, 'Sections and unexplored gaps…');
      more.addEventListener('click', () => {
        store.viewState.layout = 'coverage';
        store.setView('layout');
      });
      return h('div', { style: 'display:flex;align-items:center;gap:12px;flex-wrap:wrap' }, statusLegend(), more);
    }
    const ramp = entropyRamp();
    return h('div', { class: 'ramp' }, h('span', null, 'Entropy: 0 (uniform)'), h('div', { class: 'bar', style: `background:linear-gradient(90deg,${ramp.join(',')})` }), h('span', null, '1 (random: compressed or encrypted)'));
  }

  private geometry() {
    const width = this.canvas!.parentElement!.clientWidth || 800;
    const cell = 9;
    const cols = Math.max(16, Math.floor(width / cell));
    const size = num(store.file!.summary.fileSize);
    const rows = Math.max(1, Math.min(40, Math.ceil(size / cols / 16)));
    return { width, cell, cols, rows };
  }

  private async loadMap() {
    if (!this.canvas || !store.file) return;
    const { cols, rows } = this.geometry();
    const buckets = cols * rows;
    const [kinds, entropy] = await Promise.all([store.api.fileMap(buckets), store.api.entropyMap(buckets)]);
    this.kinds = kinds;
    this.entropy = entropy;
    this.buckets = buckets;
    this.statuses = [];
    if (this.mapMode === 'coverage') await this.loadCoverage();
    this.drawMap();
  }

  private async loadCoverage() {
    const f = store.file;
    const buckets = this.buckets;
    if (!f || !buckets) return;
    const statuses = await store.api.coverageMap(buckets);
    if (store.file !== f || this.buckets !== buckets) return;
    this.statuses = statuses;
    this.drawMap();
  }

  private bucketRange(i: number): [number, number] {
    const size = num(store.file!.summary.fileSize);
    return [Math.floor((i * size) / this.buckets), Math.floor(((i + 1) * size) / this.buckets)];
  }

  private drawMap() {
    const c = this.canvas;
    if (!c || !this.buckets || !store.file) return;
    const { width, cell, cols, rows } = this.geometry();
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(width * dpr);
    c.height = Math.round(rows * cell * dpr);
    c.style.height = `${rows * cell}px`;
    const g = c.getContext('2d')!;
    g.scale(dpr, dpr);
    const colors = familyColors();
    const status = statusColors();
    const n = Math.min(this.buckets, this.kinds.length);
    const size = cell - 1;
    for (let i = 0; i < n; i++) {
      const x = (i % cols) * cell;
      const y = Math.floor(i / cols) * cell;
      g.fillStyle =
        this.mapMode === 'regions'
          ? colors[familyOf(this.kinds[i])]
          : this.mapMode === 'entropy'
            ? entropyColor(this.entropy[i] ?? 0)
            : status[this.statuses[i] ?? 'padding'];
      g.fillRect(x, y, size, size);
    }
    const ink = getComputedStyle(document.documentElement).getPropertyValue('--sel-ring').trim();
    const ring = (i: number) => {
      if (i < 0 || i >= n) return;
      g.strokeStyle = ink;
      g.lineWidth = 1.5;
      g.strokeRect((i % cols) * cell - 0.5, Math.floor(i / cols) * cell - 0.5, size + 1, size + 1);
    };
    ring(this.hover);
    const sel = store.selection.offset;
    if (sel !== undefined) {
      const total = num(store.file.summary.fileSize);
      ring(Math.min(n - 1, Math.floor((num(sel) * this.buckets) / Math.max(1, total))));
    }
  }

  private mapHover(e: PointerEvent) {
    const c = this.canvas!;
    const { cell, cols } = this.geometry();
    const r = c.getBoundingClientRect();
    const col = Math.floor((e.clientX - r.left) / cell);
    const row = Math.floor((e.clientY - r.top) / cell);
    const i = row * cols + col;
    if (col < 0 || col >= cols || i < 0 || i >= this.buckets) {
      tooltip.hide();
      return;
    }
    if (i !== this.hover) {
      this.hover = i;
      this.drawMap();
    }
    const [start, end] = this.bucketRange(i);
    const kind = this.kinds[i];
    const st = this.statuses[i];
    tooltip.show(
      e.clientX,
      e.clientY,
      h('div', { class: 't-value' }, `${hex(start)} – ${hex(end)}`),
      h('div', { class: 't-row' }, h('span', { class: 't-key', style: `background:${familyColors()[familyOf(kind)]}` }), `Mostly ${KIND_LABELS[kind].toLowerCase()}`),
      this.mapMode === 'coverage' && st ? h('div', { class: 't-row' }, h('span', { class: 't-key', style: `background:${statusColors()[st]}` }), STATUS_LABELS[st]) : '',
      h('div', { class: 'muted' }, `Entropy ${(this.entropy[i] ?? 0).toFixed(2)} · ${formatSize(end - start)} per cell`),
    );
  }

  // --- File ↔ memory mapping ------------------------------------------------

  private mappingCard(host: HTMLElement): HTMLElement {
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'From file to memory'),
      h('p', { class: 'sub' }, 'Left: the file as stored on disk. Right: the address space after loading. Bands show where each section’s bytes end up. Heights are square-root scaled so small regions stay visible.'),
      host,
    );
  }

  private async drawMapping(host: HTMLElement) {
    const f = store.file!;
    let roots = await store.api.regions();
    // Mach-O nests everything inside segments; show what the segments contain.
    if (roots.length < 12 && roots.some((r) => r.name.startsWith('Segment ') && r.childCount > 0)) {
      const expanded: RegionInfo[] = [];
      for (const r of roots) {
        if (r.name.startsWith('Segment ') && r.childCount > 0 && r.childCount <= 64) expanded.push(...(await store.api.regions(r.id)));
        else expanded.push(r);
      }
      roots = expanded;
    }
    if (store.file !== f) return;
    const size = num(f.summary.fileSize);

    // File column: top-level regions, merging runs of tiny ones.
    interface Item { start: number; end: number; label: string; kind: RegionKind; region?: RegionInfo; count: number }
    const fileItems: Item[] = [];
    for (const r of roots) {
      const it: Item = { start: num(r.start), end: num(r.end), label: r.name, kind: r.kind, region: r, count: 1 };
      const prev = fileItems[fileItems.length - 1];
      const tiny = (x: Item) => x.end - x.start < size / 400;
      if (roots.length > 48 && prev && tiny(prev) && tiny(it) && prev.end === it.start) {
        prev.end = it.end;
        prev.count++;
        prev.label = `${prev.count} small regions`;
        if (familyOf(prev.kind) !== familyOf(it.kind)) prev.kind = 'metadata';
        prev.region = undefined;
      } else fileItems.push(it);
    }

    // Memory column: mapped segments (or loaded sections for objects).
    interface MemItem { start: bigint; end: bigint; label: string; kind: RegionKind; perms: string; fileBacked: bigint }
    const segs = f.segments.filter((s) => s.mapped && s.memSize > 0n);
    // A segment's colour follows its protection: what the loader lets the CPU do with it.
    const permKind = (perms: string, fileSize: bigint): RegionKind =>
      perms.includes('x') ? 'code' : perms.includes('w') ? (fileSize === 0n ? 'bss' : 'data') : perms.includes('r') ? 'rodata' : 'padding';
    let memItems: MemItem[] = segs.map((s) => ({
      start: s.address,
      end: s.address + s.memSize,
      label: s.name,
      kind: f.summary.format === 'pe' || f.summary.format === 'xbe' ? (f.sections.find((x) => x.address === s.address)?.kind ?? permKind(s.perms, s.fileSize)) : permKind(s.perms, s.fileSize),
      perms: s.perms,
      fileBacked: s.fileSize < s.memSize ? s.fileSize : s.memSize,
    }));
    if (memItems.length === 0) {
      memItems = f.sections.filter((s) => s.loaded && s.size > 0n).map((s) => ({ start: s.address, end: s.address + s.size, label: s.name, kind: s.kind, perms: s.perms, fileBacked: s.fileOffset !== undefined ? s.size : 0n }));
    }
    memItems.sort((a, b) => (a.start < b.start ? -1 : a.start > b.start ? 1 : 0));
    if (fileItems.length === 0 || memItems.length === 0) {
      host.replaceChildren(emptyState('Nothing is mapped into memory', 'This file has no loadable segments or sections.'));
      return;
    }

    const width = host.clientWidth || 900;
    const colW = Math.min(260, Math.max(150, width * 0.3));
    const xF = 0;
    const xM = width - colW;
    const minH = 18;
    const gapH = 18;

    // Heights: minimum + sqrt-scaled share of the remaining space.
    const layoutCol = <T>(items: T[], sizeOf: (t: T) => number, extra: number) => {
      const target = Math.max(420, items.length * minH * 1.6 + extra);
      const sq = items.map((t) => Math.sqrt(Math.max(1, sizeOf(t))));
      // Keep giant reservations (__PAGEZERO, huge bss) from flattening everything else.
      const cap = quantile(sq, 0.5) * 4;
      const capped = sq.map((v) => Math.min(v, cap || v));
      const sum = capped.reduce((a, b) => a + b, 0);
      const k = (target - extra - items.length * minH) / sum;
      return capped.map((v) => minH + v * k);
    };
    const fileH = layoutCol(fileItems, (i) => i.end - i.start, 0);
    const gaps = memItems.map((m, i) => (i > 0 && m.start > memItems[i - 1].end ? 1 : 0) as number);
    const memH = layoutCol(memItems, (m) => num(m.end - m.start), gaps.reduce((a, b) => a + b, 0) * gapH);
    const top = 28;
    const fileY: number[] = [];
    let y = top;
    for (const hgt of fileH) {
      fileY.push(y);
      y += hgt;
    }
    const fileBottom = y;
    const memY: number[] = [];
    y = top;
    memItems.forEach((_, i) => {
      if (gaps[i]) y += gapH;
      memY.push(y);
      y += memH[i];
    });
    const height = Math.max(fileBottom, y) + 12;

    const offY = (off: number) => {
      let lo = 0;
      let hi = fileItems.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (fileItems[mid].start <= off) lo = mid;
        else hi = mid - 1;
      }
      const it = fileItems[lo];
      const t = it.end > it.start ? Math.min(1, Math.max(0, (off - it.start) / (it.end - it.start))) : 0;
      return fileY[lo] + t * fileH[lo];
    };
    const addrY = (a: bigint) => {
      for (let i = 0; i < memItems.length; i++) {
        const m = memItems[i];
        if (a < m.start) return memY[i] - (gaps[i] ? gapH / 2 : 0);
        if (a <= m.end) {
          const span = m.end - m.start;
          return memY[i] + (span > 0n ? num(((a - m.start) * 10000n) / span) / 10000 : 0) * memH[i];
        }
      }
      return memY[memY.length - 1] + memH[memH.length - 1];
    };

    const NS = 'http://www.w3.org/2000/svg';
    const svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    svg.setAttribute('height', String(height));
    svg.setAttribute('role', 'img');
    svg.setAttribute('aria-label', 'File to memory mapping diagram');
    const el = (tag: string, attrs: Record<string, string | number>, parent: Element = svg) => {
      const e = document.createElementNS(NS, tag);
      for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
      parent.appendChild(e);
      return e;
    };
    const text = (x: number, yy: number, s: string, cls = '', anchor = 'start', parent: Element = svg) => {
      const t = el('text', { x, y: yy, class: cls, 'text-anchor': anchor }, parent);
      t.textContent = s;
      return t;
    };
    text(xF, 16, 'File (offsets)', 'col-title');
    text(xM, 16, 'Memory (virtual addresses)', 'col-title');

    // Bands first so boxes sit on top of them.
    const bandLayer = el('g', {});
    const x0 = xF + colW;
    const x1 = xM;
    const mid = (x0 + x1) / 2;
    // Faint whole-segment bands: headers and other unnamed bytes are mapped too.
    if (f.summary.format !== 'pe' && f.summary.format !== 'xbe') {
      for (const s of segs.filter((s) => s.fileSize > 0n)) {
        const yf0 = offY(num(s.fileOffset));
        const yf1 = Math.max(yf0 + 1.5, offY(num(s.fileOffset + s.fileSize)));
        const ym0 = addrY(s.address);
        const ym1 = Math.max(ym0 + 1.5, addrY(s.address + s.fileSize));
        el('path', {
          d: `M${x0},${yf0} C${mid},${yf0} ${mid},${ym0} ${x1},${ym0} L${x1},${ym1} C${mid},${ym1} ${mid},${yf1} ${x0},${yf1} Z`,
          fill: 'var(--axis)',
          opacity: 0.35,
        }, bandLayer);
      }
    }
    const sections = f.sections.filter((s) => s.loaded && s.fileOffset !== undefined && s.fileSize > 0n && !s.compressed && s.size > 0n);
    const bandSources = sections.length > 0 && sections.length <= 160
      ? sections.map((s) => ({ label: s.name, kind: s.kind, fo: num(s.fileOffset!), fs: num(s.fileSize < s.size ? s.fileSize : s.size), addr: s.address, size: s.size }))
      : segs.filter((s) => s.fileSize > 0n).map((s) => ({ label: s.name, kind: memItems.find((m) => m.start === s.address)?.kind ?? ('data' as RegionKind), fo: num(s.fileOffset), fs: num(s.fileSize), addr: s.address, size: s.memSize }));
    for (const b of bandSources) {
      const yf0 = offY(b.fo);
      const yf1 = Math.max(yf0 + 1.5, offY(b.fo + b.fs));
      const ym0 = addrY(b.addr);
      const ym1 = Math.max(ym0 + 1.5, addrY(b.addr + (BigInt(b.fs) < b.size ? BigInt(b.fs) : b.size)));
      const path = el('path', {
        d: `M${x0},${yf0} C${mid},${yf0} ${mid},${ym0} ${x1},${ym0} L${x1},${ym1} C${mid},${ym1} ${mid},${yf1} ${x0},${yf1} Z`,
        class: `band fam-${familyOf(b.kind)}`,
        fill: 'var(--fam)',
      }, bandLayer);
      path.addEventListener('pointermove', (e) =>
        tooltip.show(
          (e as PointerEvent).clientX,
          (e as PointerEvent).clientY,
          h('div', { class: 't-value' }, b.label),
          h('div', null, `file ${hex(b.fo)}..${hex(b.fo + b.fs)} → memory ${fmtAddr(b.addr)}..${fmtAddr(b.addr + b.size)}`),
          h('div', { class: 'muted' }, `${formatSize(b.fs)} of file data${BigInt(b.fs) < b.size ? `, plus ${formatSize(b.size - BigInt(b.fs))} zero-filled` : ''}`),
        ),
      );
      path.addEventListener('pointerleave', () => tooltip.hide());
      path.addEventListener('click', () => void store.select({ address: b.addr }, { view: 'hex' }));
    }

    const box = (x: number, yy: number, w: number, hh: number, kind: RegionKind, label: string, sub: string, onClick: () => void, tip: () => Node[], fade = 0) => {
      const g = el('g', { class: `box fam-${familyOf(kind)}`, tabindex: 0 });
      el('rect', { x, y: yy, width: w, height: hh, class: 'fill', fill: 'color-mix(in srgb, var(--fam) var(--tint-b), var(--surface))', rx: 3 }, g);
      el('rect', { x, y: yy + 1, width: 3, height: Math.max(1, hh - 2), fill: 'var(--fam)', rx: 1.5 }, g);
      if (fade > 0) {
        el('rect', { x: x + 3, y: yy + hh * (1 - fade), width: w - 3, height: hh * fade, fill: 'var(--surface)', opacity: 0.6 }, g);
      }
      if (hh >= 14) {
        const t = text(x + 9, yy + Math.min(hh / 2 + 4, 14), fit(label, w - 90), '', 'start', g);
        t.setAttribute('font-weight', '500');
        if (w > 150) text(x + w - 6, yy + Math.min(hh / 2 + 4, 14), sub, 'dim', 'end', g);
      }
      g.addEventListener('click', onClick);
      g.addEventListener('keydown', (e) => (e as KeyboardEvent).key === 'Enter' && onClick());
      g.addEventListener('pointermove', (e) => tooltip.show((e as PointerEvent).clientX, (e as PointerEvent).clientY, ...tip()));
      g.addEventListener('pointerleave', () => tooltip.hide());
    };

    fileItems.forEach((it, i) => {
      box(
        xF, fileY[i], colW, fileH[i], it.kind, it.label, formatSize(it.end - it.start),
        () => void store.select({ offset: BigInt(it.start) }, { view: 'hex' }),
        () => [h('div', { class: 't-value' }, it.label), h('div', null, `offset ${hex(it.start)}..${hex(it.end)}`), h('div', { class: 'muted' }, `${formatSize(it.end - it.start)} · ${KIND_LABELS[it.kind]}${it.region?.value ? ' · ' + it.region.value : ''}`)],
      );
    });
    memItems.forEach((m, i) => {
      if (gaps[i]) {
        const gy = memY[i] - gapH / 2;
        el('line', { x1: xM + 12, y1: gy, x2: xM + colW - 12, y2: gy, class: 'gap-mark' });
        text(xM + colW / 2, gy + 3.5, `+${formatSize(m.start - memItems[i - 1].end)} unmapped`, 'dim', 'middle').setAttribute('font-size', '10');
      }
      const span = m.end - m.start;
      const zeroFill = span > 0n && m.fileBacked < span ? num(((span - m.fileBacked) * 1000n) / span) / 1000 : 0;
      box(
        xM, memY[i], colW, memH[i], m.kind, m.label, `${m.perms} ${formatSize(span)}`,
        () => void store.select({ address: m.start }, { view: m.perms.includes('x') ? 'code' : 'hex' }),
        () => [h('div', { class: 't-value' }, m.label), h('div', null, `${fmtAddr(m.start)}..${fmtAddr(m.end)} (${m.perms})`), h('div', { class: 'muted' }, `${formatSize(span)}${zeroFill > 0 ? ` · ${formatSize(span - m.fileBacked)} zero-filled at load time` : ''}`)],
        zeroFill,
      );
    });

    host.replaceChildren(svg);
  }
}

function quantile(values: number[], q: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
}

/** Truncates a label to roughly fit `px` pixels of 11px text. */
function fit(label: string, px: number): string {
  const max = Math.max(4, Math.floor(px / 6.2));
  return label.length > max ? label.slice(0, max - 1) + '…' : label;
}

const LABEL_NAMES: Record<LabelFormat, string> = { mlb: 'for Mesen', nl: 'for FCEUX', sym: '.sym', 'no-cash': 'for no$gba' };

function logLine(s: LogSummary | null): string {
  if (!s) return 'No code/data log loaded.';
  const pct = (n: bigint) => percent(num(n), num(s.bytes));
  const parts = [`${s.format === 'fceux' ? 'FCEUX' : s.format === 'mesen' ? 'Mesen' : 'Mesen 2'} log: ${formatSize(s.code)} of code (${pct(s.code)}), ${formatSize(s.data)} of data (${pct(s.data)})`];
  if (s.pagesPlaced > 0) parts.push(`${s.pagesPlaced} PRG pages placed where they ran`);
  if (s.crcMatches === false) parts.push('made for another version of the ROM');
  return parts.join(' · ');
}

/** Loads a code/data log for the open ROM, and says what it covers. */
export async function loadCodeLog(file: File) {
  const s = await store.loadCodeLog(file.name, file);
  if (s) toast(logLine(s), s.crcMatches === false ? 'error' : 'info');
}

/** Imports an emulator's label file into the notes. */
export async function importLabels(file: File) {
  try {
    const r = await store.importLabels(file.name, await file.text());
    if (!r) return;
    const extra = [r.skipped ? `${formatCount(r.skipped)} for places that can’t be placed (a switched bank’s address without its bank)` : '', r.directives ? `${formatCount(r.directives)} directives` : ''].filter(Boolean);
    toast(`Imported ${formatCount(r.labels.length)} labels from ${file.name}${extra.length ? `; skipped ${extra.join(' and ')}` : ''}`);
  } catch (e) {
    toast(e instanceof Error ? e.message : String(e), 'error');
  }
}

async function exportLabels(format: LabelFormat) {
  try {
    const files = await store.exportLabels(format);
    if (store.annotations.length === 0) toast('No notes to export yet: name functions and places first (N, or the inspector).', 'error');
    for (const f of files) downloadText(f.name, f.text);
  } catch (e) {
    toast(e instanceof Error ? e.message : String(e), 'error');
  }
}
