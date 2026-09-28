// Sources: every file in the line tables, with where its code and data
// ended up and its text, each line marked with the code it produced. Or the
// same by compilation unit: what each one (a crate, a .cpp and its headers)
// adds to the binary.
import { store } from '../store';
import type { Attribution, AttributionMode, Contributor, LineRange, SourceFile } from '../types';
import { emptyState } from '../ui';
import { basename, debounce, formatCount, formatSize, h, hex, num, percent } from '../util';
import { VList } from '../vlist';
import { fingerprint, renderFootprint } from './attribution';
import { View } from './base';

type Mode = 'files' | 'units';

interface LineInfo {
  ranges: LineRange[];
  stmt: boolean;
}

/** A row of the list: a source file, or a compilation unit. */
interface Item {
  id: number;
  name: string;
  path: string;
  dir: string;
  size: number;
  contributor?: Contributor;
  file?: SourceFile;
}

const isMode = (s: string | undefined): s is Mode => s === 'files' || s === 'units';

export class SourcesView extends View {
  private mode: Mode = 'files';
  private attribution = new Map<AttributionMode, Attribution | null>();
  private items: Item[] = [];
  private shown: number[] = [];
  private list!: VList;
  private listNote!: HTMLElement;
  private modeTabs = new Map<Mode, HTMLButtonElement>();
  private options!: HTMLElement;
  private filterInput!: HTMLInputElement;
  /** Files by path, units by size (what each adds), unless changed. */
  private sortBy: Record<Mode, 'path' | 'size'> = { files: 'path', units: 'size' };
  /** What is picked in each mode (a file id, a unit id). */
  private picked = new Map<Mode, number>();
  private right!: HTMLElement;
  // The file shown (files mode).
  private file = -1;
  private text?: string[];
  private lines = new Map<number, LineInfo>();
  /** Rows shown: every line when text is loaded, else only lines with code. */
  private rows: number[] = [];
  private codeList!: VList;
  private header!: HTMLElement;
  private footprint!: HTMLElement;
  private body!: HTMLElement;
  private rangesPop!: HTMLElement;
  private currentLine = 0;
  private filter = '';
  private onlyWithCode = true;
  private codeLines = new Map<number, number>();
  /** Strip painters, re-run when the theme changes. */
  private painters: (() => void)[] = [];
  private generation = 0;

  constructor() {
    super('sources', true);
    store.on('sources', () => {
      if (!this.list) return;
      this.list.refresh();
      if (this.visible && this.mode === 'files' && this.file >= 0) void this.openFile(this.file, this.currentLine, false);
    });
    store.on('intent', () => {
      if (this.visible) this.consumeIntent();
    });
    window.addEventListener('themechange', () => this.visible && this.painters.forEach((p) => p()));
  }

  protected render() {
    const f = store.file!;
    this.file = -1;
    this.picked.clear();
    this.attribution.clear();
    this.codeLines.clear();
    this.items = [];
    if (!f.dwarf) {
      this.el.replaceChildren(emptyState('No line tables', 'Source mapping needs DWARF debug info. Add a debug file from the toolbar.'));
      return;
    }
    const want = store.viewState.sources;
    if (isMode(want)) this.mode = want;
    const tabs = h('div', { class: 'tabs', role: 'tablist' });
    this.modeTabs.clear();
    for (const [m, label, title] of [
      ['files', 'Files', 'Each source file: its text, and where its code and data are'],
      ['units', 'Units', 'Each compilation unit: what it adds to the binary'],
    ] as const) {
      const b = h('button', { class: 'tab', type: 'button', role: 'tab', title }, label);
      b.addEventListener('click', () => this.setMode(m, true));
      this.modeTabs.set(m, b);
      tabs.appendChild(b);
    }
    this.filterInput = h('input', { class: 'field small', type: 'search', placeholder: 'Filter', style: 'flex:1;min-width:0', 'aria-label': 'Filter' });
    this.filterInput.addEventListener(
      'input',
      debounce(() => {
        this.filter = this.filterInput.value.trim().toLowerCase();
        this.applyFilter();
      }, 100),
    );
    this.options = h('div', { class: 'src-options' });
    this.listNote = h('div', { class: 'src-note muted' });
    this.list = new VList({ rowHeight: 50, renderRow: (i) => this.itemRow(i) });
    this.right = h('div', { class: 'pane grow' });
    this.el.replaceChildren(
      h(
        'div',
        { class: 'split' },
        h('div', { class: 'pane side wider' }, h('div', { class: 'pane-head' }, tabs, this.filterInput), this.options, this.listNote, this.list.el),
        this.right,
      ),
    );
    void this.indexCodeLines();
    this.setMode(this.mode, false);
  }

  protected onState() {
    const want = store.viewState.sources;
    if (isMode(want) && want !== this.mode) this.setMode(want, false);
  }

  /** Shows files or units; `record` makes it a place in history. */
  private setMode(mode: Mode, record: boolean) {
    const f = store.file;
    if (!f?.dwarf) return;
    this.mode = mode;
    if (record) store.setViewState('sources', mode);
    for (const [m, b] of this.modeTabs) {
      b.classList.toggle('active', m === mode);
      b.setAttribute('aria-selected', String(m === mode));
    }
    this.filterInput.placeholder = mode === 'files' ? 'Filter source files' : 'Filter units';
    this.renderOptions();
    this.buildItems();
    if (mode === 'files') this.renderFilePane();
    else this.right.replaceChildren(h('div', { class: 'empty-state' }, 'Reading the line tables…'));
    void this.loadAttribution(mode === 'files' ? 'file' : 'unit').then(() => {
      if (this.mode !== mode || store.file !== f) return;
      this.buildItems();
      if (this.consumeIntent()) return;
      // The unit picked before, else the selection's, else the largest.
      const id = this.picked.get(mode) ?? (mode === 'units' ? store.selection.inspection?.unit : undefined);
      if (mode === 'units') {
        const first = this.items[this.shown[0]];
        const item = this.items.find((it) => it.id === id) ?? first;
        if (item) this.pickUnit(item);
        else this.right.replaceChildren(emptyState('Nothing to attribute', 'The line tables and variables in the debug info don’t cover any loaded section.'));
      } else if (this.file >= 0) this.showFileFootprint();
    });
    if (mode === 'files') {
      if (!this.consumeIntent() && this.file < 0) {
        const src = store.selection.inspection?.source;
        if (src) void this.openFile(src.file, src.line, true);
      }
    }
  }

  private renderOptions() {
    const sort = h(
      'select',
      { class: 'field small', 'aria-label': 'Sort' },
      h('option', { value: 'path', selected: this.sortBy[this.mode] === 'path' }, this.mode === 'files' ? 'By path' : 'By name'),
      h('option', { value: 'size', selected: this.sortBy[this.mode] === 'size' }, 'By size'),
    );
    sort.addEventListener('change', () => {
      this.sortBy[this.mode] = sort.value === 'size' ? 'size' : 'path';
      this.applyFilter();
    });
    const parts: Node[] = [];
    if (this.mode === 'files') {
      const onlyCode = h('input', { type: 'checkbox', checked: this.onlyWithCode, title: 'Hide files that produced no code (e.g. only declarations)' });
      onlyCode.addEventListener('change', () => {
        this.onlyWithCode = onlyCode.checked;
        this.applyFilter();
      });
      parts.push(h('label', { class: 'secondary' }, onlyCode, 'Only files with code'));
    }
    parts.push(h('span', { class: 'spacer' }), sort);
    this.options.replaceChildren(...parts);
  }

  private async loadAttribution(mode: AttributionMode) {
    if (this.attribution.has(mode)) return;
    const file = store.file;
    const attr = (await store.api.attribution(mode)) ?? null;
    if (store.file !== file) return;
    this.attribution.set(mode, attr);
  }

  /** Counts lines with code per file (for filtering and the file list). */
  private async indexCodeLines() {
    const counts = await store.api.fileLineCounts();
    counts.forEach((n, id) => this.codeLines.set(id, n));
    if (this.mode === 'files') this.applyFilter();
  }

  private buildItems() {
    const f = store.file!;
    const attr = this.attribution.get(this.mode === 'files' ? 'file' : 'unit');
    if (this.mode === 'files') {
      const byId = new Map((attr?.contributors ?? []).map((c) => [c.id, c]));
      this.items = f.sourceFiles.map((sf) => {
        const c = byId.get(sf.id);
        return { id: sf.id, name: sf.name, path: sf.path, dir: sf.dir, size: c ? num(c.code + c.data) : 0, contributor: c, file: sf };
      });
    } else {
      this.items = (attr?.contributors ?? []).map((c) => ({ id: c.id, name: c.name, path: c.path, dir: c.path, size: num(c.code + c.data), contributor: c }));
    }
    this.applyFilter();
    // How much of the code the line tables account for.
    if (attr) {
      const codeTotal = f.sections.filter((s) => s.kind === 'code' && s.loaded).reduce((a, s) => a + num(s.size), 0);
      const codeAttributed = attr.sections.filter((s) => f.sections[s.section]?.kind === 'code').reduce((a, s) => a + num(s.attributed), 0);
      this.listNote.textContent = `Line tables cover ${percent(codeAttributed, codeTotal)} of the code`;
    } else this.listNote.textContent = '';
  }

  private applyFilter() {
    const q = this.filter;
    const indexed = this.codeLines.size > 0;
    const keep = (it: Item) => {
      if (q && !it.path.toLowerCase().includes(q) && !it.name.toLowerCase().includes(q)) return false;
      if (this.mode === 'files' && this.onlyWithCode && indexed && (this.codeLines.get(it.id) ?? 1) === 0 && it.size === 0) return false;
      return true;
    };
    const order = (a: Item, b: Item) =>
      this.sortBy[this.mode] === 'size'
        ? b.size - a.size || a.path.localeCompare(b.path)
        : this.mode === 'files'
          ? ownFirst(a.path) - ownFirst(b.path) || a.path.localeCompare(b.path)
          : a.name.localeCompare(b.name);
    this.shown = this.items
      .map((it, i) => [it, i] as const)
      .filter(([it]) => keep(it))
      .sort(([a], [b]) => order(a, b))
      .map(([, i]) => i);
    this.list?.setCount(this.shown.length);
  }

  private itemRow(i: number): HTMLElement {
    const it = this.items[this.shown[i]];
    const picked = this.mode === 'files' ? it.id === this.file : it.id === this.picked.get('units');
    const f = store.file!;
    let sub: string;
    if (this.mode === 'files') {
      const n = this.codeLines.get(it.id);
      const loaded = store.sources.has(it.id);
      sub = `${n !== undefined ? `${formatCount(n)} lines with code · ` : ''}${loaded ? 'source loaded · ' : it.file?.embeddedSource ? 'embedded source · ' : ''}${it.dir}`;
    } else sub = it.path;
    const row = h(
      'div',
      { class: `contrib${picked ? ' selected' : ''}`, title: it.path },
      h('div', { class: 'c-top' }, h('span', { class: 'c-name' }, it.name), h('span', { class: 'c-size' }, it.size > 0 ? formatSize(it.size) : '')),
      h('div', { class: 'c-dir' }, sub),
      it.contributor ? fingerprint(it.contributor, f.sections) : h('div', { class: 'fp' }),
    );
    row.addEventListener('click', () => {
      if (this.mode === 'files') void this.openFile(it.id, 0, true);
      else this.pickUnit(it);
    });
    return row;
  }

  /** Reveals what an intent asks for; true if there was one to act on. */
  private consumeIntent(): boolean {
    const { source, contributor } = store.intent;
    if (source) {
      store.intent = {};
      if (this.mode !== 'files') this.setMode('files', false);
      void this.openFile(source.file, source.line, true);
      return true;
    }
    if (contributor !== undefined) {
      // Units are listed once their attribution is read.
      if (this.mode === 'units' && !this.attribution.has('unit')) return false;
      store.intent = {};
      const want = contributor.toLowerCase();
      const it =
        this.items.find((x) => x.name.toLowerCase() === want || x.path.toLowerCase() === want || basename(x.path).toLowerCase() === want) ??
        this.items.find((x) => x.path.toLowerCase().includes(want));
      if (!it) return false;
      if (this.mode === 'files') void this.openFile(it.id, 0, true);
      else this.pickUnit(it);
      return true;
    }
    return false;
  }

  // --- Units ------------------------------------------------------------------------

  private pickUnit(it: Item) {
    const c = it.contributor;
    this.picked.set('units', it.id);
    this.list.refresh();
    const at = this.shown.indexOf(this.items.indexOf(it));
    if (at >= 0) this.list.scrollToIndex(at, 'nearest');
    if (!c) return;
    const gen = ++this.generation;
    const host = h('div');
    const f = store.file!;
    // The unit's own source file, when it has one by that name.
    const own = f.sourceFiles.find((sf) => sf.path === c.path) ?? f.sourceFiles.find((sf) => sf.name === c.name);
    const actions = h('div', { class: 'btn-row' });
    if (own) {
      const b = h('button', { class: 'btn small', type: 'button' }, `Open ${own.name}`);
      b.addEventListener('click', () => store.openSource(own.id, 0));
      actions.appendChild(b);
    }
    this.right.replaceChildren(
      h(
        'div',
        { class: 'scroll' },
        h('div', { class: 'page' }, h('div', { class: 'card' }, h('h2', { class: 'mono', style: 'font-size:16px;overflow-wrap:anywhere' }, c.name), h('p', { class: 'sub mono', style: 'overflow-wrap:anywhere' }, c.path), actions, host)),
      ),
    );
    this.painters = [];
    void renderFootprint(host, 'unit', c, { compact: false, isCurrent: () => gen === this.generation && this.mode === 'units' }).then((p) => {
      if (gen === this.generation) this.painters = p;
    });
  }

  // --- Files ------------------------------------------------------------------------

  private renderFilePane() {
    this.header = h('div', { class: 'toolbar' });
    this.footprint = h('div', { class: 'src-footprint' });
    this.footprint.hidden = true;
    this.codeList = new VList({ rowHeight: 21, className: 'src-view', renderRow: (i) => this.lineRow(i) });
    this.rangesPop = h('div', { class: 'ranges-pop' });
    this.rangesPop.style.display = 'none';
    this.body = h('div', { style: 'display:flex;flex-direction:column;flex:1;min-height:0' }, this.codeList.el);
    this.right.replaceChildren(this.header, this.footprint, this.body, this.rangesPop);
    this.header.replaceChildren(h('span', { class: 'secondary' }, 'Pick a file. Lines that produced machine code are marked; click one to see its addresses.'));
    if (this.file >= 0) {
      const id = this.file;
      this.file = -1;
      void this.openFile(id, this.currentLine, true);
    }
  }

  private async openFile(id: number, line: number, scroll: boolean) {
    const sf = store.file?.sourceFiles[id];
    if (!sf) return;
    if (this.mode !== 'files') this.setMode('files', false);
    const changed = this.file !== id;
    this.file = id;
    this.picked.set('files', id);
    if (changed || !this.text) {
      const [ranges, text] = await Promise.all([store.api.fileLines(id), store.sourceText(id)]);
      if (this.file !== id) return;
      this.lines.clear();
      for (const r of ranges) {
        const li = this.lines.get(r.line) ?? { ranges: [], stmt: false };
        li.ranges.push(r);
        li.stmt ||= r.isStmt;
        this.lines.set(r.line, li);
      }
      this.codeLines.set(id, [...this.lines.keys()].filter((l) => l > 0).length);
      this.text = text?.split(/\r?\n/);
    }
    this.rows = this.text ? this.text.map((_, i) => i + 1) : [...this.lines.keys()].filter((l) => l > 0).sort((a, b) => a - b);
    this.renderHeader(sf);
    if (changed) this.showFileFootprint();
    this.codeList.setCount(this.rows.length);
    this.list.refresh();
    const fi = this.shown.findIndex((i) => this.items[i]?.id === id);
    if (fi >= 0 && changed) this.list.scrollToIndex(fi, 'nearest');
    this.currentLine = line;
    if (line > 0) {
      const idx = this.rows.indexOf(line);
      if (idx >= 0 && scroll) this.codeList.scrollToIndex(idx, 'center');
    }
    this.codeList.refresh();
    this.hideRanges();
  }

  /** Where the shown file's code and data are, once the attribution is read. */
  private showFileFootprint() {
    const c = this.attribution.get('file')?.contributors.find((x) => x.id === this.file);
    this.footprint.hidden = !c;
    if (!c) return;
    const gen = ++this.generation;
    const id = this.file;
    this.painters = [];
    void renderFootprint(this.footprint, 'file', c, { compact: true, isCurrent: () => gen === this.generation && this.file === id && this.mode === 'files' }).then((p) => {
      if (gen === this.generation) this.painters = p;
    });
  }

  private renderHeader(sf: SourceFile) {
    const withCode = [...this.lines.keys()].filter((l) => l > 0).length;
    const origin = store.sourceOrigin.get(sf.id);
    const load = h('button', { class: 'btn small' }, 'Load source folder…');
    load.addEventListener('click', () => window.dispatchEvent(new CustomEvent('binviz:load-sources')));
    this.header.replaceChildren(
      h('div', { style: 'display:flex;flex-direction:column;min-width:0;flex:1' }, h('strong', { class: 'mono', style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, sf.name), h('span', { class: 'muted', style: 'font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap', title: sf.path }, sf.path)),
      h('span', { class: 'secondary' }, `${formatCount(withCode)} lines produced code`),
      this.text ? h('span', { class: 'chip', title: origin ?? '' }, origin === 'embedded in DWARF' ? 'embedded source' : 'source loaded') : load,
    );
    if (!this.text) {
      this.body.replaceChildren(
        h('div', { class: 'pad secondary', style: 'border-bottom:1px solid var(--border);flex:none;font-size:12px' }, 'The source text is not part of the binary. Load the project folder to see it; for now, here are the line numbers that produced code.'),
        this.codeList.el,
      );
    } else if (!this.body.contains(this.codeList.el) || this.body.children.length > 1) {
      this.body.replaceChildren(this.codeList.el);
    }
  }

  private lineRow(i: number): HTMLElement {
    const line = this.rows[i];
    const info = this.lines.get(line);
    const text = this.text?.[line - 1] ?? '';
    const selLoc = store.selection.inspection?.source;
    const current = (selLoc && selLoc.file === this.file && selLoc.line === line) || (!selLoc && line === this.currentLine);
    const count = info ? info.ranges.length : 0;
    const el = h(
      'div',
      { class: `src-line${info ? ' has-code' : ''}${current ? ' current' : ''}` },
      h('span', { class: 'gut' }, String(line)),
      h('span', { class: 'mark' }, info ? h('i', { class: info.stmt ? '' : 'weak', title: info.stmt ? 'Produced code' : 'Produced code (not a statement boundary)' }) : null),
      h('span', { class: 'count' }, count ? `${count}×` : ''),
      h('span', { class: 'txt' }, this.text ? text : info ? `(${info.ranges.length} address range${info.ranges.length === 1 ? '' : 's'})` : ''),
    );
    if (info) {
      el.addEventListener('click', () => {
        this.currentLine = line;
        this.showRanges(line, info);
        const first = [...info.ranges].sort((a, b) => Number(a.start - b.start))[0];
        void store.select({ address: first.start }, { origin: 'sources' });
      });
    }
    return el;
  }

  private showRanges(line: number, info: LineInfo) {
    const sorted = [...info.ranges].sort((a, b) => (a.start < b.start ? -1 : 1));
    const list = h('div', { style: 'display:flex;flex-wrap:wrap;gap:4px 12px' });
    for (const r of sorted.slice(0, 200)) {
      const l = h('span', { class: 'link mono', style: 'font-size:12px' }, `${hex(r.start)}..${hex(r.end)}`);
      l.addEventListener('click', () => void store.select({ address: r.start }, { view: 'code' }));
      list.appendChild(h('span', null, l, h('span', { class: 'muted', style: 'font-size:11px' }, ` ${num(r.end - r.start)}B${r.column ? ` col ${r.column}` : ''}`)));
    }
    this.rangesPop.replaceChildren(
      h('div', { style: 'display:flex;gap:8px;align-items:baseline;margin-bottom:6px' }, h('strong', null, `Line ${line}`), h('span', { class: 'secondary' }, `${sorted.length} address range${sorted.length === 1 ? '' : 's'} — click one to see its code`), h('span', { style: 'flex:1' }), closeButton(() => this.hideRanges())),
      list,
    );
    this.rangesPop.style.display = 'block';
  }

  private hideRanges() {
    if (this.rangesPop) this.rangesPop.style.display = 'none';
  }

  protected onSelection() {
    if (!this.codeList || this.mode !== 'files') return;
    const src = store.selection.inspection?.source;
    if (src && store.selection.origin !== 'sources' && src.line > 0) {
      if (src.file !== this.file) void this.openFile(src.file, src.line, true);
      else {
        const idx = this.rows.indexOf(src.line);
        if (idx >= 0) this.codeList.scrollToIndex(idx, 'center');
      }
    }
    this.codeList.refresh();
  }
}

function closeButton(onClick: () => void): HTMLElement {
  const b = h('button', { class: 'btn ghost small' }, 'Close');
  b.addEventListener('click', onClick);
  return b;
}

/** Sort key: the program's own files before toolchain/library files. */
function ownFirst(path: string): number {
  return /[\\/]rustc[\\/]|[\\/]library[\\/](core|std|alloc)|[\\/]usr[\\/]|mingw|[\\/]include[\\/]|crt/i.test(path) ? 1 : 0;
}
