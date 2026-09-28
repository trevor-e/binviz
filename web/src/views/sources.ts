// Sources: every file in the line tables, with the code each line produced.
import { store } from '../store';
import type { LineRange, SourceFile } from '../types';
import { emptyState } from '../ui';
import { debounce, formatCount, h, hex, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';

interface LineInfo {
  ranges: LineRange[];
  stmt: boolean;
}

export class SourcesView extends View {
  private files: SourceFile[] = [];
  private shown: number[] = [];
  private fileList!: VList;
  private file = -1;
  private text?: string[];
  private lines = new Map<number, LineInfo>();
  /** Rows shown: every line when text is loaded, else only lines with code. */
  private rows: number[] = [];
  private codeList!: VList;
  private header!: HTMLElement;
  private body!: HTMLElement;
  private rangesPop!: HTMLElement;
  private currentLine = 0;
  private filter = '';
  private onlyWithCode = true;
  private codeLines = new Map<number, number>();

  constructor() {
    super('sources', true);
    store.on('sources', () => {
      if (!this.fileList) return;
      this.fileList.refresh();
      if (this.visible && this.file >= 0) void this.openFile(this.file, this.currentLine, false);
    });
    store.on('intent', () => {
      if (this.visible && store.intent.source) this.consumeIntent();
    });
  }

  protected render() {
    const f = store.file!;
    this.file = -1;
    this.files = f.sourceFiles;
    this.codeLines.clear();
    if (!f.dwarf) {
      this.el.replaceChildren(emptyState('No line tables', 'Source mapping needs DWARF debug info. Add a debug file from the toolbar.'));
      return;
    }
    const filter = h('input', { class: 'field small', type: 'search', placeholder: 'Filter files', style: 'flex:1' });
    filter.addEventListener('input', debounce(() => {
      this.filter = filter.value.trim().toLowerCase();
      this.applyFilter();
    }, 100));
    const onlyCode = h('input', { type: 'checkbox', checked: this.onlyWithCode, title: 'Hide files that produced no code (e.g. only declarations)' });
    onlyCode.addEventListener('change', () => {
      this.onlyWithCode = onlyCode.checked;
      this.applyFilter();
    });
    this.fileList = new VList({ rowHeight: 38, renderRow: (i) => this.fileRow(i) });
    this.header = h('div', { class: 'toolbar' });
    this.codeList = new VList({ rowHeight: 21, className: 'src-view', renderRow: (i) => this.lineRow(i) });
    this.rangesPop = h('div', { class: 'ranges-pop' });
    this.rangesPop.style.display = 'none';
    this.body = h('div', { style: 'display:flex;flex-direction:column;flex:1;min-height:0' }, this.codeList.el);
    this.el.replaceChildren(
      h(
        'div',
        { class: 'split' },
        h(
          'div',
          { class: 'pane side wide' },
          h('div', { class: 'pane-head' }, filter),
          h('label', { class: 'secondary', style: 'display:flex;gap:6px;align-items:center;padding:4px 10px;font-size:12px' }, onlyCode, 'Only files with code'),
          this.fileList.el,
        ),
        h('div', { class: 'pane grow' }, this.header, this.body, this.rangesPop),
      ),
    );
    this.header.replaceChildren(h('span', { class: 'secondary' }, 'Pick a file. Lines that produced machine code are marked; click one to see its addresses.'));
    void this.indexCodeLines().then(() => {
      this.applyFilter();
      if (store.intent.source) this.consumeIntent();
      else if (this.file < 0) {
        const src = store.selection.inspection?.source;
        if (src) void this.openFile(src.file, src.line, true);
      }
    });
    this.applyFilter();
  }

  /** Counts lines with code per file (for filtering and the file list). */
  private async indexCodeLines() {
    const counts = await store.api.fileLineCounts();
    counts.forEach((n, id) => this.codeLines.set(id, n));
    this.fileList?.refresh();
  }

  private applyFilter() {
    const q = this.filter;
    const indexed = this.codeLines.size > 0;
    this.shown = this.files
      .filter((f) => (!q || f.path.toLowerCase().includes(q)) && (!this.onlyWithCode || !indexed || (this.codeLines.get(f.id) ?? 1) > 0))
      .sort((a, b) => ownFirst(a) - ownFirst(b) || a.path.localeCompare(b.path))
      .map((f) => f.id);
    this.fileList?.setCount(this.shown.length);
  }

  private fileRow(i: number): HTMLElement {
    const f = this.files[this.shown[i]];
    const loaded = store.sources.has(f.id);
    const n = this.codeLines.get(f.id);
    const row = h(
      'div',
      { class: `list-row${f.id === this.file ? ' selected' : ''}`, style: 'height:38px;flex-direction:column;align-items:flex-start;justify-content:center;gap:0', title: f.path },
      h('span', { class: 'nm', style: 'max-width:100%;font-weight:500' }, f.name),
      h('span', { class: 'muted', style: 'font-size:11px;max-width:100%;overflow:hidden;text-overflow:ellipsis' }, `${n !== undefined ? `${n} lines with code · ` : ''}${loaded ? 'source loaded · ' : f.embeddedSource ? 'embedded source · ' : ''}${f.dir}`),
    );
    row.addEventListener('click', () => void this.openFile(f.id, 0, true));
    return row;
  }

  private consumeIntent() {
    const s = store.intent.source;
    store.intent = {};
    if (s) void this.openFile(s.file, s.line, true);
  }

  private async openFile(id: number, line: number, scroll: boolean) {
    const sf = this.files[id];
    if (!sf) return;
    const changed = this.file !== id;
    this.file = id;
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
    this.codeList.setCount(this.rows.length);
    this.fileList.refresh();
    const fi = this.shown.indexOf(id);
    if (fi >= 0 && changed) this.fileList.scrollToIndex(fi, 'nearest');
    this.currentLine = line;
    if (line > 0) {
      const idx = this.rows.indexOf(line);
      if (idx >= 0 && scroll) this.codeList.scrollToIndex(idx, 'center');
    }
    this.codeList.refresh();
    this.hideRanges();
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
    this.rangesPop.style.display = 'none';
  }

  protected onSelection() {
    if (!this.codeList) return;
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
function ownFirst(f: SourceFile): number {
  return /[\\/]rustc[\\/]|[\\/]library[\\/](core|std|alloc)|[\\/]usr[\\/]|mingw|[\\/]include[\\/]|crt/i.test(f.path) ? 1 : 0;
}

