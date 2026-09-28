// Map: where each source file (or compilation unit) ended up in the binary,
// and how much of the code and data has been mapped out while reverse
// engineering it.
import { KIND_LABELS, STATUSES, STATUS_LABELS, familyColors, familyOf, statusColors } from '../colors';
import { store, type MapTab } from '../store';
import type { Attribution, AttributedRange, AttributionMode, Contributor, Coverage, Gap, MapStatus, Section, StatusBytes } from '../types';
import { emptyState, famClass, toast, tooltip } from '../ui';
import { basename, debounce, formatCount, formatSize, h, hex, num, percent } from '../util';
import { VList } from '../vlist';
import { View } from './base';

const CONTENT = new Set(['code', 'rodata', 'data', 'bss', 'tls']);

export class MapView extends View {
  private tab: MapTab = 'files';
  private attribution = new Map<AttributionMode, Attribution | null>();
  private selected = new Map<AttributionMode, number>();
  private coverage: Coverage | null = null;
  private body!: HTMLElement;
  private tabs = new Map<MapTab, HTMLButtonElement>();
  /** Canvas painters, re-run when the theme changes. */
  private painters: (() => void)[] = [];
  private generation = 0;

  constructor() {
    super('map', true);
    window.addEventListener('themechange', () => this.visible && this.painters.forEach((p) => p()));
    window.addEventListener(
      'resize',
      debounce(() => this.visible && this.body && this.renderBody(), 250),
    );
    store.on('annotations', () => {
      this.coverage = null;
      if (this.visible && this.tab === 'coverage' && this.body) this.renderBody();
    });
    store.on('intent', () => {
      const m = store.intent.map;
      if (!m || store.view !== 'map') return;
      this.tab = m.tab;
      if (this.body) this.renderBody();
    });
  }

  protected render() {
    this.attribution.clear();
    this.selected.clear();
    this.coverage = null;
    const f = store.file!;
    if (!f.dwarf && this.tab !== 'coverage') this.tab = 'coverage';
    const bar = h('div', { class: 'tabs' });
    this.tabs.clear();
    const labels: [MapTab, string][] = [
      ['files', 'By source file'],
      ['units', 'By compilation unit'],
      ['coverage', 'Reverse-engineering coverage'],
    ];
    for (const [t, label] of labels) {
      const b = h('button', { class: 'tab', type: 'button', disabled: t !== 'coverage' && !f.dwarf, title: t !== 'coverage' && !f.dwarf ? 'Needs DWARF line tables' : '' }, label);
      b.addEventListener('click', () => {
        this.tab = t;
        this.renderBody();
      });
      this.tabs.set(t, b);
      bar.appendChild(b);
    }
    this.body = h('div', { class: 'map-body' });
    this.el.replaceChildren(h('div', { class: 'toolbar' }, bar), this.body);
    this.renderBody();
  }

  private renderBody() {
    for (const [t, b] of this.tabs) b.classList.toggle('active', t === this.tab);
    this.painters = [];
    const gen = ++this.generation;
    tooltip.hide();
    if (this.tab === 'coverage') void this.renderCoverage(gen);
    else void this.renderAttribution(this.tab === 'files' ? 'file' : 'unit', gen);
  }

  // --- Attribution ------------------------------------------------------------

  private async renderAttribution(mode: AttributionMode, gen: number) {
    const f = store.file!;
    if (!f.dwarf) {
      this.body.replaceChildren(emptyState('No debug information', 'Attributing bytes to source files needs DWARF line tables. Add a debug file from the toolbar.'));
      return;
    }
    let attr = this.attribution.get(mode);
    if (attr === undefined) {
      this.body.replaceChildren(h('div', { class: 'empty-state' }, 'Reading the line tables…'));
      attr = (await store.api.attribution(mode)) ?? null;
      this.attribution.set(mode, attr);
      if (gen !== this.generation) return;
    }
    if (!attr || attr.contributors.length === 0) {
      this.body.replaceChildren(emptyState('Nothing to attribute', 'The line tables and variables in the debug info don’t cover any loaded section.'));
      return;
    }
    const list = attr.contributors;
    const intent = store.intent.map;
    if (intent?.name) {
      const want = intent.name.toLowerCase();
      const hit = list.find((c) => c.name.toLowerCase() === want || basename(c.path).toLowerCase() === want) ?? list.find((c) => c.name.toLowerCase().includes(want));
      if (hit) this.selected.set(mode, hit.id);
      store.intent = {};
    }
    let shown = list.map((_, i) => i);
    const selId = () => this.selected.get(mode) ?? list[0].id;
    const detail = h('div', { class: 'pane grow scroll' });
    const vlist = new VList({
      rowHeight: 50,
      renderRow: (i) => {
        const c = list[shown[i]];
        const row = h(
          'div',
          { class: `contrib${c.id === selId() ? ' selected' : ''}`, title: c.path },
          h('div', { class: 'c-top' }, h('span', { class: 'c-name' }, c.name), h('span', { class: 'c-size' }, formatSize(c.code + c.data))),
          h('div', { class: 'c-dir' }, mode === 'file' ? dirOf(c.path) : c.path),
          fingerprint(c, f.sections),
        );
        row.addEventListener('click', () => {
          this.selected.set(mode, c.id);
          vlist.refresh();
          void this.renderContributor(mode, c, detail, gen);
        });
        return row;
      },
    });
    const filter = h('input', { class: 'field', type: 'search', placeholder: mode === 'file' ? 'Filter source files' : 'Filter units', 'aria-label': 'Filter' });
    filter.addEventListener(
      'input',
      debounce(() => {
        const q = filter.value.trim().toLowerCase();
        shown = list.map((_, i) => i).filter((i) => !q || list[i].path.toLowerCase().includes(q) || list[i].name.toLowerCase().includes(q));
        vlist.setCount(shown.length);
      }, 80),
    );
    const codeTotal = f.sections.filter((s) => s.kind === 'code' && s.loaded).reduce((a, s) => a + num(s.size), 0);
    const codeAttributed = attr.sections.filter((s) => f.sections[s.section]?.kind === 'code').reduce((a, s) => a + num(s.attributed), 0);
    const head = h(
      'div',
      { class: 'pane-head', style: 'flex-direction:column;align-items:stretch' },
      filter,
      h('div', { class: 'secondary', style: 'font-size:12px' }, `${formatCount(list.length)} ${mode === 'file' ? 'source files' : 'units'} · line tables cover ${percent(codeAttributed, codeTotal)} of the code`),
    );
    this.body.replaceChildren(h('div', { class: 'split' }, h('div', { class: 'pane side wider' }, head, vlist.el), detail));
    vlist.setCount(shown.length);
    const sel = list.find((c) => c.id === selId()) ?? list[0];
    const idx = list.indexOf(sel);
    if (idx > 0) requestAnimationFrame(() => vlist.scrollToIndex(idx, 'center'));
    void this.renderContributor(mode, sel, detail, gen);
  }

  private async renderContributor(mode: AttributionMode, c: Contributor, host: HTMLElement, gen: number) {
    const f = store.file!;
    const ranges = await store.api.attributedRanges(mode, c.id);
    if (gen !== this.generation) return;
    this.painters = [];
    const own = new Set(c.sections.map((s) => s.section));
    const content = f.sections.filter((s) => s.loaded && s.size > 0n && CONTENT.has(s.kind));
    const shownSections = f.sections.filter((s) => own.has(s.index) || (content.length <= 12 && content.includes(s)));
    shownSections.sort((a, b) => (a.address < b.address ? -1 : a.address > b.address ? 1 : 0));
    const facts = h(
      'div',
      { class: 'tiles' },
      tile('Code', formatSize(c.code), `${formatCount(c.functions)} function${c.functions === 1 ? '' : 's'}`),
      tile('Data', formatSize(c.data), `${formatCount(c.variables)} global variable${c.variables === 1 ? '' : 's'}`),
      tile('Sections', String(own.size), [...own].map((i) => f.sections[i]?.name ?? '?').join(', ')),
    );
    const strips = h('div', { class: 'strips' });
    for (const s of shownSections) {
      const share = c.sections.find((x) => x.section === s.index);
      const bytes = share ? share.code + share.data : 0n;
      const inSection = ranges.filter((r) => r.section === s.index);
      const canvas = h('canvas', { class: 'strip', role: 'img', 'aria-label': `${s.name}: ${formatSize(bytes)} from ${c.name}` });
      strips.appendChild(
        h(
          'div',
          { class: `strip-row${bytes === 0n ? ' none' : ''}` },
          h('div', { class: 'strip-label' }, h('span', { class: `swatch ${famClass(s.kind)}` }), h('span', { class: 'mono' }, s.name), h('span', { class: 'muted' }, KIND_LABELS[s.kind].toLowerCase())),
          canvas,
          h('div', { class: 'strip-value' }, bytes > 0n ? `${formatSize(bytes)} · ${percent(num(bytes), num(s.size))}` : 'none'),
        ),
      );
      this.stripOfRanges(canvas, s, inSection);
    }
    const groups = groupRanges(ranges);
    const rows = groups.slice(0, 500).map((g) => {
      const sec = g.section !== undefined ? f.sections[g.section] : undefined;
      const tr = h(
        'tr',
        { class: 'clickable' },
        h('td', { class: 'mono', style: 'max-width:420px;overflow:hidden;text-overflow:ellipsis', title: g.label }, g.label),
        h('td', null, h('span', { class: `swatch ${sec ? famClass(sec.kind) : ''}` }), g.data ? 'variable' : 'code'),
        h('td', { class: 'mono' }, sec?.name ?? '—'),
        h('td', { class: 'mono' }, hex(g.start)),
        h('td', { class: 'right' }, formatSize(g.bytes), g.fragments > 1 ? h('span', { class: 'muted' }, ` in ${g.fragments}`) : null),
        h('td', { class: 'right muted' }, g.line ? String(g.line) : ''),
      );
      tr.addEventListener('click', () => go(g.start, !g.data));
      return tr;
    });
    host.replaceChildren(
      h(
        'div',
        { class: 'page' },
        h(
          'div',
          { class: 'card' },
          h('h2', { class: 'mono', style: 'font-size:16px;overflow-wrap:anywhere' }, c.name),
          h('p', { class: 'sub mono', style: 'overflow-wrap:anywhere' }, c.path),
          facts,
          mode === 'file' && f.sourceFiles[c.id]
            ? h('div', { style: 'margin-top:10px' }, linkBtn('Open source', () => store.openSource(c.id, 0)))
            : null,
        ),
        h(
          'div',
          { class: 'card' },
          h('h2', null, 'Where its bytes are'),
          h('p', { class: 'sub' }, `Each strip is a whole section, drawn to scale; marks are ${mode === 'file' ? 'this file’s' : 'this unit’s'} code (from the line tables) and global variables (from DWARF). Click a mark to jump there.`),
          strips,
        ),
        h(
          'div',
          { class: 'card' },
          h('h2', null, 'Functions and variables'),
          h('p', { class: 'sub' }, mode === 'file' ? 'Code is grouped by the function it was compiled into; inlined code counts toward the file it was written in.' : 'Code grouped by the function containing it.'),
          groups.length === 0
            ? h('div', { class: 'muted' }, 'No ranges.')
            : h(
                'div',
                { class: 'table-scroll' },
                h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Name'), h('th', null, 'Kind'), h('th', null, 'Section'), h('th', null, 'Address'), h('th', { class: 'right' }, 'Bytes'), h('th', { class: 'right' }, 'Line'))), h('tbody', null, rows)),
              ),
          groups.length > 500 ? h('p', { class: 'muted' }, `Showing 500 of ${formatCount(groups.length)}.`) : null,
        ),
      ),
    );
    host.scrollTop = 0;
    requestAnimationFrame(() => this.painters.forEach((p) => p()));
  }

  /** A section strip with marks for attributed ranges. */
  private stripOfRanges(canvas: HTMLCanvasElement, s: Section, ranges: AttributedRange[]) {
    const span = num(s.size);
    const paint = () => {
      const w = canvas.clientWidth || 600;
      const g = setup(canvas, w, 18);
      const colors = familyColors();
      g.fillStyle = cssVar('--surface-2');
      roundRect(g, 0, 0, w, 18, 3);
      g.fillStyle = colors[familyOf(s.kind)];
      for (const r of ranges) {
        const x0 = ((num(r.start) - num(s.address)) / span) * w;
        const x1 = ((num(r.end) - num(s.address)) / span) * w;
        g.fillRect(Math.floor(x0), 2, Math.max(2, x1 - x0), 14);
      }
    };
    this.painters.push(paint);
    const find = (e: PointerEvent) => {
      const rect = canvas.getBoundingClientRect();
      const a = num(s.address) + ((e.clientX - rect.left) / rect.width) * span;
      const slack = (3 / rect.width) * span;
      let best: AttributedRange | undefined;
      let bestD = Infinity;
      for (const r of ranges) {
        const d = a < num(r.start) ? num(r.start) - a : a >= num(r.end) ? a - num(r.end) : 0;
        if (d <= slack && d < bestD) {
          best = r;
          bestD = d;
        }
      }
      return best;
    };
    canvas.addEventListener('pointermove', (e) => {
      const r = find(e);
      canvas.style.cursor = r ? 'pointer' : 'default';
      if (!r) return tooltip.hide();
      tooltip.show(
        e.clientX,
        e.clientY,
        h('div', { class: 't-value' }, r.label ?? hex(r.start)),
        h('div', null, `${hex(r.start)}..${hex(r.end)} · ${formatSize(r.end - r.start)}`),
        h('div', { class: 'muted' }, `${r.data ? 'global variable' : 'code'}${r.line ? `, line ${r.line}` : ''}`),
      );
    });
    canvas.addEventListener('pointerleave', () => tooltip.hide());
    canvas.addEventListener('click', (e) => {
      const r = find(e);
      if (r) go(r.start, !r.data);
    });
  }

  // --- Coverage ---------------------------------------------------------------

  private async renderCoverage(gen: number) {
    const f = store.file!;
    if (!this.coverage) {
      this.body.replaceChildren(h('div', { class: 'empty-state' }, 'Measuring coverage…'));
      this.coverage = await store.api.coverage(100);
      if (gen !== this.generation) return;
    }
    const cov = this.coverage;
    if (cov.sections.length === 0) {
      this.body.replaceChildren(emptyState('No code or data sections', 'Coverage is measured over loaded code and data sections.'));
      return;
    }
    const t = cov.totals;
    const total = sumStatus(t);
    const denom = total - num(t.padding);
    const named = num(t.reviewed) + num(t.annotated) + num(t.named) + num(t.structure);
    const mapped = named + num(t.recovered);
    const fn = cov.functions;

    const fileInput = h('input', { type: 'file', accept: '.json,.csv,.txt,.tsv,.map', style: 'display:none' });
    fileInput.addEventListener('change', async () => {
      const file = fileInput.files?.[0];
      fileInput.value = '';
      if (!file) return;
      const n = await store.importAnnotations(await file.text());
      toast(n > 0 ? `Imported ${formatCount(n)} annotations from ${file.name}` : `No addresses found in ${file.name}`, n > 0 ? 'info' : 'error');
    });
    const importBtn = btn('Import…', 'Import annotations: a binviz export, or a symbol list (CSV with an address column, nm output, "address name" lines)', () => fileInput.click());
    const exportBtn = btn('Export', 'Download your annotations as JSON', () => download(`${basename(f.name)}.binviz-notes.json`, store.exportAnnotations()));
    exportBtn.toggleAttribute('disabled', store.annotations.length === 0);

    const bar = h('div', { class: 'stack-bar tall', role: 'img', 'aria-label': 'Share of code and data bytes by coverage status' });
    for (const s of STATUSES) {
      const bytes = num(t[s.id]);
      if (bytes === 0) continue;
      const seg = h('div', { class: `st-${s.id}`, style: `flex:${bytes} 1 0;background:var(--st)` });
      seg.addEventListener('pointermove', (e) => tooltip.show(e.clientX, e.clientY, h('div', { class: 't-value' }, `${formatSize(bytes)} · ${percent(bytes, total)}`), h('div', null, s.label), h('div', { class: 'muted' }, s.description)));
      seg.addEventListener('pointerleave', () => tooltip.hide());
      bar.appendChild(seg);
    }

    const summary = h(
      'div',
      { class: 'card' },
      h(
        'div',
        { class: 'card-head' },
        h('div', { style: 'flex:1' }, h('h2', null, 'How much of this binary is mapped out'), h('p', { class: 'sub' }, 'Every byte of the loaded code and data sections, by the strongest thing that explains it. Name functions and mark them reviewed from the inspector.')),
        importBtn,
        exportBtn,
        fileInput,
      ),
      h(
        'div',
        { class: 'tiles' },
        tile('Mapped out', percent(mapped, denom), `${formatSize(mapped)} of ${formatSize(denom)} (padding excluded)`),
        tile('Named', percent(named, denom), 'symbols, your names and known structures'),
        tile('Functions', formatCount(fn.named + fn.recovered + fn.user), `${formatCount(fn.named)} named · ${formatCount(fn.recovered)} recovered · ${formatCount(fn.user)} yours`),
        tile('Your notes', formatCount(cov.annotations), `${formatCount(cov.reviewed)} marked reviewed`),
        tile('Unexplored', formatSize(t.unexplored), `in ${formatCount(cov.gapCount)} gap${cov.gapCount === 1 ? '' : 's'}`, 'warn'),
      ),
      bar,
      statusLegend(),
    );

    const fileMap = h('canvas', { role: 'img', 'aria-label': 'Coverage of the whole file' });
    const fileCard = h(
      'div',
      { class: 'card' },
      h('h2', null, 'Whole file'),
      h('p', { class: 'sub' }, 'The file left to right, top to bottom. Headers, tables and debug info count as format structure. Click a cell to open it in the hex view.'),
      h('div', { class: 'filemap' }, fileMap),
    );

    const strips = h('div', { class: 'strips' });
    const sections = [...cov.sections].sort((a, b) => num(b.size) - num(a.size)).slice(0, 40);
    sections.sort((a, b) => (a.address < b.address ? -1 : 1));
    for (const sc of sections) {
      const canvas = h('canvas', { class: 'strip', role: 'img', 'aria-label': `Coverage of ${sc.name}` });
      const top = STATUSES.map((s) => [s, num(sc.bytes[s.id])] as const)
        .filter(([, n]) => n > 0)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([s, n]) => `${percent(n, num(sc.size))} ${s.label.toLowerCase()}`)
        .join(' · ');
      strips.appendChild(
        h(
          'div',
          { class: 'strip-row' },
          h('div', { class: 'strip-label' }, h('span', { class: `swatch ${famClass(sc.kind)}` }), h('span', { class: 'mono' }, sc.name), h('span', { class: 'muted' }, formatSize(sc.size))),
          canvas,
          h('div', { class: 'strip-value' }, top),
        ),
      );
      this.stripOfStatus(canvas, sc.section, sc.address, sc.size, sc.kind === 'code', gen);
    }
    const sectionsCard = h(
      'div',
      { class: 'card' },
      h('h2', null, 'Sections'),
      h('p', { class: 'sub' }, 'Each strip is a whole section at scale. Click to jump there.'),
      strips,
      cov.sections.length > sections.length ? h('p', { class: 'muted' }, `Showing the ${sections.length} largest of ${cov.sections.length} sections.`) : null,
    );

    const gapRows = cov.gaps.map((g) => {
      const sec = f.sections[g.section];
      const tr = h(
        'tr',
        { class: 'clickable' },
        h('td', { class: 'mono' }, `${hex(g.start)}..${hex(g.end)}`),
        h('td', { class: 'right' }, formatSize(g.end - g.start)),
        h('td', { class: 'mono' }, sec?.name ?? '?'),
        h('td', { class: 'mono', style: 'max-width:280px;overflow:hidden;text-overflow:ellipsis', title: g.after ?? '' }, g.after ?? '—'),
        h('td', null, g.hint),
        h('td', { class: 'mono muted' }, g.preview),
      );
      tr.addEventListener('click', () => goGap(g, sec));
      return tr;
    });
    const gapsCard = h(
      'div',
      { class: 'card' },
      h('h2', null, 'Largest unexplored gaps'),
      h('p', { class: 'sub' }, 'Runs of code or data nothing accounts for yet: good places to look next. “Looks like” is a guess from the bytes.'),
      cov.gaps.length === 0
        ? h('div', { class: 'muted' }, 'No unexplored bytes. Everything is named, recovered, structure or padding.')
        : h(
            'div',
            { class: 'table-scroll' },
            h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Range'), h('th', { class: 'right' }, 'Size'), h('th', null, 'Section'), h('th', null, 'After'), h('th', null, 'Looks like'), h('th', null, 'First bytes'))), h('tbody', null, gapRows)),
          ),
      cov.gapCount > cov.gaps.length ? h('p', { class: 'muted' }, `Showing the ${cov.gaps.length} largest of ${formatCount(cov.gapCount)} gaps.`) : null,
    );

    this.body.replaceChildren(h('div', { class: 'scroll' }, h('div', { class: 'page' }, summary, fileCard, sectionsCard, gapsCard)));
    this.fileCoverage(fileMap, gen);
  }

  /** Whole-file coverage grid, like the overview's file map. */
  private fileCoverage(canvas: HTMLCanvasElement, gen: number) {
    const size = num(store.file!.summary.fileSize);
    const cell = 9;
    const width = () => canvas.parentElement!.clientWidth || 800;
    const cols = Math.max(16, Math.floor(width() / cell));
    const rows = Math.max(1, Math.min(24, Math.ceil(size / cols / 16)));
    const buckets = cols * rows;
    let data: MapStatus[] = [];
    let hover = -1;
    const paint = () => {
      const w = width();
      const g = setup(canvas, w, rows * cell);
      const colors = statusColors();
      data.forEach((st, i) => {
        g.fillStyle = colors[st];
        g.fillRect((i % cols) * cell, Math.floor(i / cols) * cell, cell - 1, cell - 1);
      });
      if (hover >= 0) {
        g.strokeStyle = cssVar('--sel-ring');
        g.lineWidth = 1.5;
        g.strokeRect((hover % cols) * cell - 0.5, Math.floor(hover / cols) * cell - 0.5, cell, cell);
      }
    };
    const range = (i: number) => [Math.floor((i * size) / buckets), Math.floor(((i + 1) * size) / buckets)];
    const at = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect();
      const c = Math.floor((e.clientX - r.left) / cell);
      const i = Math.floor((e.clientY - r.top) / cell) * cols + c;
      return c >= 0 && c < cols && i >= 0 && i < data.length ? i : -1;
    };
    canvas.addEventListener('pointermove', (e) => {
      const i = at(e);
      if (i !== hover) {
        hover = i;
        paint();
      }
      if (i < 0) return tooltip.hide();
      const [s, en] = range(i);
      tooltip.show(e.clientX, e.clientY, h('div', { class: 't-value' }, `${hex(s)} – ${hex(en)}`), h('div', { class: 't-row' }, h('span', { class: 't-key', style: `background:${statusColors()[data[i]]}` }), `Mostly ${STATUS_LABELS[data[i]].toLowerCase()}`), h('div', { class: 'muted' }, `${formatSize(en - s)} per cell`));
    });
    canvas.addEventListener('pointerleave', () => {
      hover = -1;
      tooltip.hide();
      paint();
    });
    canvas.addEventListener('click', (e) => {
      const i = at(e);
      if (i >= 0) void store.select({ offset: BigInt(range(i)[0]) }, { view: 'hex' });
    });
    this.painters.push(paint);
    void store.api.coverageMap(buckets).then((d) => {
      if (gen !== this.generation) return;
      data = d;
      paint();
    });
  }

  /** A section strip coloured by coverage status. */
  private stripOfStatus(canvas: HTMLCanvasElement, section: number, address: bigint, size: bigint, code: boolean, gen: number) {
    let data: MapStatus[] = [];
    const paint = () => {
      const w = canvas.clientWidth || 600;
      const g = setup(canvas, w, 18);
      const colors = statusColors();
      const n = data.length;
      if (n === 0) return;
      // Runs of one status as single rects, so thin strips stay crisp.
      let start = 0;
      for (let i = 1; i <= n; i++) {
        if (i === n || data[i] !== data[start]) {
          g.fillStyle = colors[data[start]];
          const x0 = Math.round((start / n) * w);
          const x1 = Math.round((i / n) * w);
          g.fillRect(x0, 0, Math.max(1, x1 - x0), 18);
          start = i;
        }
      }
    };
    const addrAt = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect();
      const t = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
      return { t, a: address + BigInt(Math.floor(t * num(size))) };
    };
    canvas.addEventListener('pointermove', (e) => {
      if (data.length === 0) return;
      const { t, a } = addrAt(e);
      const st = data[Math.min(data.length - 1, Math.floor(t * data.length))];
      tooltip.show(e.clientX, e.clientY, h('div', { class: 't-value' }, hex(a)), h('div', { class: 't-row' }, h('span', { class: 't-key', style: `background:${statusColors()[st]}` }), STATUS_LABELS[st]));
    });
    canvas.addEventListener('pointerleave', () => tooltip.hide());
    canvas.addEventListener('click', (e) => go(addrAt(e).a, code));
    this.painters.push(paint);
    requestAnimationFrame(() => {
      const buckets = Math.max(64, Math.min(2048, Math.round(canvas.clientWidth || 600)));
      void store.api.coverageStrip(section, buckets).then((d) => {
        if (gen !== this.generation) return;
        data = d;
        paint();
      });
    });
  }
}

// --- Helpers --------------------------------------------------------------------

function go(address: bigint, code: boolean) {
  void store.select({ address }, { view: code ? 'code' : 'hex' });
}

function goGap(g: Gap, sec: Section | undefined) {
  if (sec?.kind === 'code' || g.offset === undefined) go(g.start, sec?.kind === 'code');
  else void store.select({ address: g.start }, { view: 'hex' });
}

function dirOf(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i > 0 ? path.slice(0, i) : '';
}

/** A tiny bar of a contributor's bytes per section, coloured by section kind. */
function fingerprint(c: Contributor, sections: Section[]): HTMLElement {
  const bar = h('div', { class: 'fp' });
  for (const s of c.sections) {
    const bytes = num(s.code + s.data);
    const sec = sections[s.section];
    if (bytes === 0 || !sec) continue;
    bar.appendChild(h('span', { class: famClass(sec.kind), style: `flex:${bytes} 1 0`, title: `${sec.name}: ${formatSize(bytes)}` }));
  }
  return bar;
}

interface Group {
  label: string;
  data: boolean;
  start: bigint;
  bytes: bigint;
  section?: number;
  line: number;
  fragments: number;
}

function groupRanges(ranges: AttributedRange[]): Group[] {
  const groups = new Map<string, Group>();
  for (const r of ranges) {
    const label = r.label ?? hex(r.start);
    const key = `${r.data ? 'd' : 'c'}:${label}`;
    const g = groups.get(key);
    if (g) {
      g.bytes += r.end - r.start;
      g.fragments++;
      if (r.start < g.start) g.start = r.start;
      if (r.line && (!g.line || r.line < g.line)) g.line = r.line;
    } else {
      groups.set(key, { label, data: r.data, start: r.start, bytes: r.end - r.start, section: r.section, line: r.line, fragments: 1 });
    }
  }
  return [...groups.values()].sort((a, b) => (a.start < b.start ? -1 : a.start > b.start ? 1 : 0));
}

function sumStatus(t: StatusBytes): number {
  return STATUSES.reduce((a, s) => a + num(t[s.id]), 0);
}

function tile(label: string, value: string, sub: string, tone = ''): HTMLElement {
  return h('div', { class: `tile ${tone}` }, h('div', { class: 'tile-label' }, label), h('div', { class: 'tile-value' }, value), h('div', { class: 'tile-sub' }, sub));
}

function statusLegend(): HTMLElement {
  return h(
    'div',
    { class: 'legend' },
    STATUSES.map((s) => h('span', { title: s.description }, h('span', { class: `swatch st-${s.id}` }), s.label)),
  );
}

function btn(label: string, title: string, onClick: () => void): HTMLButtonElement {
  const b = h('button', { class: 'btn small', type: 'button', title }, label);
  b.addEventListener('click', onClick);
  return b;
}

function linkBtn(label: string, onClick: () => void): HTMLButtonElement {
  const b = h('button', { class: 'btn small', type: 'button' }, label);
  b.addEventListener('click', onClick);
  return b;
}

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const a = h('a', { href: url, download: name });
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Sizes a canvas for the device pixel ratio and returns a context in CSS pixels. */
function setup(canvas: HTMLCanvasElement, w: number, hgt: number): CanvasRenderingContext2D {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(hgt * dpr);
  canvas.style.height = `${hgt}px`;
  const g = canvas.getContext('2d')!;
  g.scale(dpr, dpr);
  g.clearRect(0, 0, w, hgt);
  return g;
}

function roundRect(g: CanvasRenderingContext2D, x: number, y: number, w: number, hgt: number, r: number) {
  g.beginPath();
  g.roundRect(x, y, w, hgt, r);
  g.fill();
}
