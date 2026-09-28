// Code view: functions and their disassembly, interleaved with source lines.
import { store } from '../store';
import type { Disassembly, Instruction, SourceLoc } from '../types';
import { emptyState } from '../ui';
import { basename, debounce, formatSize, h, hex } from '../util';
import { VList } from '../vlist';
import { View } from './base';

type Row = { kind: 'src'; loc: SourceLoc } | { kind: 'ins'; ins: Instruction; index: number };
type Func = [bigint, bigint, string];

/** Functions are fetched from the worker a page at a time. */
const FPAGE = 200;

export class CodeView extends View {
  /** Code sections, listed instead when the binary has no function symbols. */
  private sectionList: Func[] | null = null;
  private shown: number[] = [];
  private fpages = new Map<number, Func[]>();
  private fpending = new Set<number>();
  private fgen = 0;
  private funcList!: VList;
  private asmList!: VList;
  private rows: Row[] = [];
  private current?: Disassembly;
  private header!: HTMLElement;
  private asmHost!: HTMLElement;
  private lines = new Map<number, string[]>();
  private loadingAt?: bigint;

  constructor() {
    super('code', true);
    store.on('sources', () => {
      this.lines.clear();
      if (this.visible && this.asmList) this.asmList.refresh();
    });
    // New names change the function list and branch targets; comments show inline.
    store.on('annotations', () => {
      if (!this.funcList || !store.file || this.sectionList) return;
      this.resetFunctions();
      const at = this.current?.function?.address ?? this.current?.start;
      this.current = undefined;
      if (at !== undefined) void this.load(store.selection.address ?? at);
    });
  }

  private filterText = '';

  protected render() {
    const f = store.file!;
    this.current = undefined;
    this.rows = [];
    this.lines.clear();
    const filter = h('input', { class: 'field', placeholder: 'Filter functions', type: 'search', 'aria-label': 'Filter functions' });
    filter.addEventListener('input', debounce(() => this.applyFilter(filter.value), 80));
    this.funcList = new VList({ rowHeight: 24, renderRow: (i) => this.funcRow(i) });
    this.header = h('div', { class: 'toolbar' });
    this.asmList = new VList({ rowHeight: 22, className: 'asm', renderRow: (i) => this.asmRow(i) });
    this.asmHost = h('div', { style: 'display:flex;flex-direction:column;flex:1;min-height:0' }, this.asmList.el);
    this.el.replaceChildren(
      h(
        'div',
        { class: 'split' },
        h('div', { class: 'pane side' }, h('div', { class: 'pane-head' }, filter), this.funcList.el),
        h('div', { class: 'pane grow' }, this.header, this.asmHost),
      ),
    );
    this.sectionList = null;
    this.filterText = '';
    void store.api.functionsPage('', 0, FPAGE).then((page) => {
      if (store.file !== f) return;
      if (page.total === 0) {
        // No function symbols: offer code sections instead.
        this.sectionList = f.sections.filter((s) => s.kind === 'code' && s.size > 0n).map((s) => [s.address, s.size, `section ${s.name}`]);
        this.applyFilter('');
      } else {
        this.fpages.clear();
        this.fpending.clear();
        this.fpages.set(0, page.functions);
        this.funcList.setCount(page.total);
      }
      this.onSelection();
    });
  }

  private applyFilter(q: string) {
    this.filterText = q;
    if (this.sectionList) {
      const needle = q.trim().toLowerCase();
      this.shown = [];
      this.sectionList.forEach((f, i) => {
        if (!needle || f[2].toLowerCase().includes(needle) || hex(f[0]).includes(needle)) this.shown.push(i);
      });
      this.funcList.setCount(this.shown.length);
      return;
    }
    this.resetFunctions();
  }

  /** Refetches the (filtered) function list from the first page. */
  private resetFunctions() {
    this.fpages.clear();
    this.fpending.clear();
    this.fgen++;
    this.fetchFunctions(0, true);
  }

  private fetchFunctions(page: number, reset = false) {
    if (this.fpages.has(page) || this.fpending.has(page)) return;
    this.fpending.add(page);
    const gen = this.fgen;
    void store.api.functionsPage(this.filterText, page * FPAGE, FPAGE).then((res) => {
      if (gen !== this.fgen) return;
      this.fpending.delete(page);
      this.fpages.set(page, res.functions);
      if (reset) this.funcList.setCount(res.total);
      else this.funcList.refresh();
    });
  }

  private funcAt(i: number): Func | undefined {
    if (this.sectionList) return this.sectionList[this.shown[i]];
    const page = Math.floor(i / FPAGE);
    const got = this.fpages.get(page)?.[i % FPAGE];
    if (!got) this.fetchFunctions(page);
    return got;
  }

  private funcRow(i: number): HTMLElement {
    const fn = this.funcAt(i);
    if (!fn) return h('div', { class: 'list-row muted' }, '…');
    const [addr, , name] = fn;
    const cur = this.current?.function?.address ?? this.current?.start;
    const note = store.notesAt.get(addr);
    const row = h(
      'div',
      { class: `list-row${cur === addr ? ' selected' : ''}`, title: note?.comment ? `${name}\n${note.comment}` : name },
      h('span', { class: 'addr' }, hex(addr)),
      h('span', { class: 'nm' }, name),
      note?.reviewed ? h('span', { class: 'reviewed', title: 'Reviewed' }, '✓') : null,
    );
    row.addEventListener('click', () => void store.select({ address: addr }, { origin: 'code' }));
    return row;
  }

  protected onSelection() {
    const addr = store.selection.address;
    if (addr === undefined || !this.asmList) return;
    const cur = this.current;
    if (cur && addr >= cur.start && addr < cur.end) {
      this.revealInstruction(addr, store.selection.origin !== 'code');
      return;
    }
    void this.load(addr);
  }

  private async load(addr: bigint) {
    if (this.loadingAt === addr) return;
    this.loadingAt = addr;
    const d = await store.api.disassembleFunction(addr, 20000);
    if (this.loadingAt !== addr) return;
    this.loadingAt = undefined;
    this.current = d;
    this.rows = [];
    let last: SourceLoc | undefined;
    d.instructions.forEach((ins, index) => {
      const s = ins.source;
      if (s && (!last || last.file !== s.file || last.line !== s.line)) this.rows.push({ kind: 'src', loc: s });
      if (s) last = s;
      this.rows.push({ kind: 'ins', ins, index });
    });
    this.renderHeader(d);
    if (!d.supported) {
      this.asmHost.replaceChildren(emptyState(`No disassembler for ${store.file?.summary.arch}`, 'binviz decodes x86, x86-64, AArch64 and ARM. The hex view still shows these bytes.'));
      return;
    }
    if (d.instructions.length === 0) {
      this.asmHost.replaceChildren(emptyState('Not code', 'This address is not inside a code section with file bytes.'));
      return;
    }
    if (!this.asmHost.contains(this.asmList.el)) this.asmHost.replaceChildren(this.asmList.el);
    this.asmList.setCount(this.rows.length);
    this.funcList.refresh();
    const at = d.function?.address ?? d.start;
    const fi = this.sectionList
      ? this.shown.findIndex((i) => this.sectionList![i][0] === at)
      : ((await store.api.functionIndex(this.filterText, at)) ?? -1);
    if (fi >= 0 && this.current === d) this.funcList.scrollToIndex(fi, 'center');
    this.revealInstruction(addr, true);
  }

  private renderHeader(d: Disassembly) {
    const fn = d.function;
    const name = fn ? (fn.demangled ?? fn.name) : `${hex(d.start)}`;
    const btn = (label: string, onClick: () => void) => {
      const b = h('button', { class: 'btn small' }, label);
      b.addEventListener('click', onClick);
      return b;
    };
    const count = d.instructions.length;
    const note = store.notesAt.get(fn?.address ?? d.start);
    this.header.replaceChildren(
      h('h3', { class: 'mono', style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:60%', title: fn?.name ?? '' }, name),
      ...(note?.reviewed ? [h('span', { class: 'chip ok' }, 'Reviewed')] : []),
      h('span', { class: 'secondary mono' }, `${hex(d.start)}..${hex(d.end)} · ${formatSize(d.end - d.start)} · ${count}${d.truncated ? '+' : ''} instructions`),
      h('span', { class: 'spacer' }),
      btn('Hex', () => void store.select({ address: d.start }, { view: 'hex' })),
    );
    if (store.file?.dwarf) {
      this.header.appendChild(
        btn('DWARF', async () => {
          const at = (await store.api.functionDieAt(d.start)) ?? (await store.api.dieAt(d.start));
          if (at) store.openDie(at[0], at[1]);
        }),
      );
    }
  }

  private revealInstruction(addr: bigint, scroll: boolean) {
    const idx = this.rows.findIndex((r) => r.kind === 'ins' && addr >= r.ins.address && addr < r.ins.address + BigInt(r.ins.len));
    if (idx >= 0 && scroll) this.asmList.scrollToIndex(idx, 'center');
    this.asmList.refresh();
  }

  private sourceLines(file: number): string[] | undefined {
    const cached = this.lines.get(file);
    if (cached) return cached;
    const text = store.sources.get(file);
    if (text === undefined) return undefined;
    const lines = text.split(/\r?\n/);
    this.lines.set(file, lines);
    return lines;
  }

  private asmRow(i: number): HTMLElement {
    const r = this.rows[i];
    if (r.kind === 'src') {
      const lines = this.sourceLines(r.loc.file);
      const text = r.loc.line === 0 ? '(no line: compiler-generated code)' : (lines?.[r.loc.line - 1] ?? '');
      const row = h(
        'div',
        { class: `src-row${r.loc.line === 0 ? ' inline-note' : ''}`, title: r.loc.path },
        h('span', { class: 'ln' }, `${basename(r.loc.path)}:${r.loc.line}`),
        h('span', { class: 'txt' }, text),
      );
      row.addEventListener('click', () => store.openSource(r.loc.file, r.loc.line));
      return row;
    }
    const ins = r.ins;
    const sel = store.selection.address;
    const selected = sel !== undefined && sel >= ins.address && sel < ins.address + BigInt(ins.len);
    const mnCls = ins.flow === 'call' ? ' call' : ins.flow === 'return' ? ' ret' : '';
    const row = h(
      'div',
      { class: `asm-row${selected ? ' selected' : ''}` },
      h('span', { class: 'addr' }, hex(ins.address)),
      h('span', { class: 'bytes' }, ins.bytes),
      h('span', { class: `mn${mnCls}` }, ins.mnemonic),
      h('span', { class: 'ops' }, ins.operands),
    );
    const note = store.notesAt.get(ins.address);
    if (ins.target !== undefined) {
      const t = ins.target;
      // Jumps inside the function read better as offsets than as its (long) name.
      const cur = this.current;
      const local = cur && t >= cur.start && t < cur.end;
      const label = local ? `<+${hex(t - cur.start)}>` : ins.targetSymbol ? `<${ins.targetSymbol}>` : '';
      const link = h('span', { class: 'tgt', title: `${hex(t)}${ins.targetSymbol ? ' ' + ins.targetSymbol : ''}` }, label);
      link.addEventListener('click', (e) => {
        e.stopPropagation();
        void store.select({ address: t }, { view: store.file && this.isCode(t) ? 'code' : 'hex' });
      });
      row.appendChild(link);
    }
    if (note?.comment) row.appendChild(h('span', { class: 'cmt', title: note.comment }, `; ${note.comment.split('\n')[0]}`));
    row.addEventListener('click', () => void store.select({ address: ins.address }, { origin: 'code' }));
    return row;
  }

  private isCode(address: bigint): boolean {
    return store.file!.sections.some((s) => s.kind === 'code' && s.loaded && address >= s.address && address < s.address + s.size);
  }
}

