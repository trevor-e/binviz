// Symbols, Objective-C classes, imports, exports and strings.
import { store } from '../store';
import type { Export, FoundString, Import, ObjcCounts, ObjcEntry, ObjcKind, Sym, SymbolQuery } from '../types';
import { emptyState } from '../ui';
import { debounce, fmtAddr, formatCount, h, hex, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';
import { selectorUses } from './objc';

const PAGE = 200;
const COLS = 'grid-template-columns:19ch 9ch 9ch 8ch 8ch minmax(0,1fr)';

type Tab = 'symbols' | 'classes' | 'imports' | 'exports' | 'strings';
const TABS: Tab[] = ['symbols', 'classes', 'imports', 'exports', 'strings'];
const isTab = (s: string | undefined): s is Tab => TABS.includes(s as Tab);

const OBJC_KINDS: [ObjcKind | '', string][] = [
  ['', 'All'],
  ['class', 'Classes'],
  ['category', 'Categories'],
  ['protocol', 'Protocols'],
];

/** `1 class`, `2 categories` */
function count(n: number, noun: string): string {
  if (n === 1) return `1 ${noun}`;
  if (noun === 'class') return `${formatCount(n)} classes`;
  if (noun === 'category') return `${formatCount(n)} categories`;
  return `${formatCount(n)} ${noun}s`;
}

export function describeObjc(c: ObjcCounts): string {
  const swift = c.swiftClasses ? ` (${formatCount(c.swiftClasses)} Swift)` : '';
  return `${count(c.classes, 'class')}${swift}, ${count(c.categories, 'category')}, ${count(c.protocols, 'protocol')}; ${count(c.methods, 'method')}`;
}

export class SymbolsView extends View {
  private tab: Tab = 'symbols';
  private query: SymbolQuery = { filter: '', kind: '', sort: 'address', descending: false, definedOnly: false };
  private pages = new Map<number, Sym[]>();
  private pending = new Set<number>();
  private list!: VList;
  private count!: HTMLElement;
  private body!: HTMLElement;
  private generation = 0;
  private imports: Import[] = [];
  private exports: Export[] = [];
  private stringFilter = '';
  private strPages = new Map<number, FoundString[]>();
  private strPending = new Set<number>();
  private strGeneration = 0;
  private strList?: VList;
  private strCount?: HTMLElement;
  private objc?: ObjcCounts;
  private objcEntries?: ObjcEntry[];
  private objcFilter = '';
  private objcKind: ObjcKind | '' = '';
  private objcSelected?: ObjcEntry;

  constructor() {
    super('symbols', true);
    // Names given in the inspector show up here.
    store.on('annotations', () => {
      if (this.visible && this.tab === 'symbols' && this.list) this.update({});
    });
  }

  protected render() {
    const tabs = h('div', { class: 'tabs' });
    const tab = (t: Tab, label: string) => {
      const b = h('button', { class: `tab${this.tab === t ? ' active' : ''}`, title: t === 'classes' ? 'Objective-C classes, categories and protocols' : '' }, label);
      b.addEventListener('click', () => {
        this.showTab(t);
        store.setViewState('symbols', t);
      });
      b.dataset.tab = t;
      return b;
    };
    tabs.append(tab('symbols', 'Symbols'), tab('imports', 'Imports'), tab('exports', 'Exports'), tab('strings', 'Strings'));
    this.body = h('div', { style: 'display:flex;flex-direction:column;flex:1;min-height:0' });
    this.el.replaceChildren(h('div', { class: 'toolbar' }, tabs), this.body);
    const file = store.file;
    this.objc = undefined;
    this.objcEntries = undefined;
    this.objcSelected = undefined;
    const want = store.viewState.symbols;
    if (isTab(want)) this.tab = want;
    // Classes wait for the Objective-C metadata to be read.
    const classes = this.tab === 'classes';
    if (classes) this.tab = 'symbols';
    void Promise.all([store.api.imports(), store.api.exports()]).then(([i, e]) => {
      this.imports = i;
      this.exports = e;
      if (this.tab === 'imports' || this.tab === 'exports') this.renderBody();
    });
    // Objective-C classes get a tab when the binary has any.
    void store.api.objcCounts().then((c) => {
      if (store.file !== file || c.classes + c.categories + c.protocols === 0) return;
      this.objc = c;
      tabs.firstElementChild!.after(tab('classes', 'Classes'));
      if (classes && this.tab === 'symbols') this.showTab('classes');
    });
    this.el.querySelectorAll<HTMLElement>('.tabs .tab').forEach((x) => x.classList.toggle('active', x.dataset.tab === this.tab));
    this.renderBody();
  }

  protected onState() {
    const want = store.viewState.symbols;
    if (isTab(want) && want !== this.tab) this.showTab(want);
  }

  /** Shows a tab: `classes` only when the binary has Objective-C classes. */
  showTab(t: Tab) {
    if (t === 'classes' && !this.objc) return;
    this.tab = t;
    this.el.querySelectorAll<HTMLElement>('.tabs .tab').forEach((x) => x.classList.toggle('active', x.dataset.tab === t));
    this.renderBody();
  }

  private renderBody() {
    if (this.tab === 'symbols') this.renderSymbols();
    else if (this.tab === 'classes') void this.renderClasses();
    else if (this.tab === 'imports') this.renderImports();
    else if (this.tab === 'exports') this.renderExports();
    else this.renderStrings();
  }

  private renderSymbols() {
    const filter = h('input', { class: 'field small', type: 'search', placeholder: 'Filter by name', value: this.query.filter ?? '', style: 'width:260px' });
    filter.addEventListener('input', debounce(() => this.update({ filter: filter.value }), 120));
    const kind = h('select', { class: 'field small' }, ...['', 'function', 'data', 'label', 'section', 'file', 'tls', 'unknown', 'undefined'].map((k) => h('option', { value: k, selected: this.query.kind === k }, k || 'All kinds')));
    kind.addEventListener('change', () => this.update({ kind: kind.value }));
    const defined = h('input', { type: 'checkbox', checked: !!this.query.definedOnly });
    defined.addEventListener('change', () => this.update({ definedOnly: defined.checked }));
    this.count = h('span', { class: 'secondary' });
    const head = h('div', { class: 'list-row', style: `display:grid;${COLS};font-weight:600;color:var(--text-2);cursor:default;border-bottom:1px solid var(--border);background:var(--surface)` });
    for (const [label, sort] of [['Address', 'address'], ['Size', 'size'], ['Kind', ''], ['Binding', ''], ['Source', ''], ['Name', 'name']] as const) {
      const active = sort && this.query.sort === sort;
      const cell = h('span', { style: sort ? 'cursor:pointer' : '' }, label, active ? (this.query.descending ? ' ↓' : ' ↑') : '');
      if (sort) cell.addEventListener('click', () => this.update(this.query.sort === sort ? { descending: !this.query.descending } : { sort, descending: false }));
      head.appendChild(cell);
    }
    this.list = new VList({ rowHeight: 24, renderRow: (i) => this.symRow(i) });
    this.body.replaceChildren(
      h('div', { class: 'toolbar' }, filter, kind, h('label', { class: 'secondary', style: 'display:flex;gap:4px;align-items:center' }, defined, 'Defined only'), h('span', { class: 'spacer' }), this.count),
      head,
      this.list.el,
    );
    this.update({});
  }

  private update(q: Partial<SymbolQuery>) {
    this.query = { ...this.query, ...q };
    this.pages.clear();
    this.pending.clear();
    this.generation++;
    this.fetchPage(0, true);
  }

  private fetchPage(page: number, reset = false) {
    if (this.pages.has(page) || this.pending.has(page)) return;
    this.pending.add(page);
    const gen = this.generation;
    void store.api.symbols({ ...this.query, offset: page * PAGE, limit: PAGE }).then((res) => {
      if (gen !== this.generation) return;
      this.pending.delete(page);
      this.pages.set(page, res.symbols);
      this.count.textContent = `${formatCount(res.total)} of ${formatCount(store.file?.summary.symbolCount ?? 0)} symbols`;
      if (reset) this.list.setCount(res.total);
      else this.list.refresh();
    });
  }

  private symRow(i: number): HTMLElement {
    const page = Math.floor(i / PAGE);
    const s = this.pages.get(page)?.[i % PAGE];
    if (!s) {
      this.fetchPage(page);
      return h('div', { class: 'list-row muted' }, '…');
    }
    const sel = store.selection.address;
    const selected = sel !== undefined && s.defined && sel >= s.address && sel < s.address + (s.size > 0n ? s.size : 1n);
    const row = h(
      'div',
      { class: `list-row${selected ? ' selected' : ''}`, style: `display:grid;${COLS}`, title: s.demangled ? s.name : '' },
      h('span', { class: 'addr' }, s.defined ? fmtAddr(s.address) : '—'),
      h('span', { class: 'mono secondary' }, s.size > 0n ? `${num(s.size)}${s.sizeInferred ? '~' : ''}` : ''),
      h('span', { class: 'secondary' }, s.kind),
      h('span', { class: 'secondary' }, s.binding),
      h('span', { class: 'muted' }, s.source),
      h('span', { class: 'nm' }, s.demangled ?? s.name),
    );
    row.addEventListener('click', () => s.defined && void store.select({ address: s.address }, { origin: 'symbols' }));
    row.addEventListener('dblclick', () => s.defined && void store.select({ address: s.address }, { view: s.kind === 'function' ? 'code' : 'hex' }));
    return row;
  }

  private async renderClasses() {
    const file = store.file;
    this.objcEntries ??= await store.api.objcEntries();
    if (store.file !== file || this.tab !== 'classes') return;
    const entries = this.objcEntries;
    const filter = h('input', { class: 'field small', type: 'search', placeholder: 'Filter by name', value: this.objcFilter, style: 'width:220px' });
    const kind = h('select', { class: 'field small', 'aria-label': 'Kind' }, ...OBJC_KINDS.map(([k, label]) => h('option', { value: k, selected: this.objcKind === k }, label)));
    const shownCount = h('span', { class: 'secondary' });
    const detail = h('div', { class: 'pane grow scroll objc-detail' });
    let shown: ObjcEntry[] = [];
    const list = new VList({
      rowHeight: 24,
      renderRow: (i) => {
        const e = shown[i];
        const selected = this.objcSelected?.kind === e.kind && this.objcSelected.name === e.name;
        const row = h(
          'div',
          { class: `list-row objc-row${selected ? ' selected' : ''}`, title: e.kind === 'class' && e.base ? `${e.name} : ${e.base}` : e.name },
          h('span', { class: `objc-kind k-${e.kind}`, title: e.kind }, e.kind === 'class' ? 'C' : e.kind === 'category' ? '+' : 'P'),
          h('span', { class: 'nm' }, e.name),
          e.kind === 'class' && e.base ? h('span', { class: 'nm muted' }, `: ${e.base}`) : null,
          h('span', { class: 'spacer' }),
          h('span', { class: 'muted mono', title: 'Methods' }, formatCount(e.methods)),
        );
        row.addEventListener('click', () => {
          this.objcSelected = e;
          list.refresh();
          void this.showInterface(e, detail);
        });
        return row;
      },
    });
    const apply = () => {
      const f = this.objcFilter.toLowerCase();
      shown = entries.filter((e) => (!this.objcKind || e.kind === this.objcKind) && (!f || e.name.toLowerCase().includes(f) || (e.base ?? '').toLowerCase() === f));
      shownCount.textContent = shown.length === entries.length ? '' : `${formatCount(shown.length)} shown`;
      list.setCount(shown.length);
    };
    filter.addEventListener(
      'input',
      debounce(() => {
        this.objcFilter = filter.value;
        apply();
      }, 120),
    );
    kind.addEventListener('change', () => {
      this.objcKind = kind.value as ObjcKind | '';
      apply();
    });
    this.body.replaceChildren(
      h('div', { class: 'toolbar' }, filter, kind, h('span', { class: 'spacer' }), shownCount, this.objc ? h('span', { class: 'secondary' }, describeObjc(this.objc)) : null),
      h('div', { class: 'split' }, h('div', { class: 'pane side wider' }, list.el), detail),
    );
    apply();
    const first = this.objcSelected ?? shown[0];
    if (first) {
      this.objcSelected = first;
      list.refresh();
      void this.showInterface(first, detail);
    }
  }

  /** The selected class, category or protocol as its header would declare it. */
  private async showInterface(e: ObjcEntry, detail: HTMLElement) {
    const i = await store.api.objcInterface(e.kind, e.name);
    if (this.objcSelected !== e) return;
    if (!i) {
      detail.replaceChildren(h('div', { class: 'pad muted' }, `${e.name} could not be read.`));
      return;
    }
    const open = (address: bigint, view: 'code' | 'hex') => void store.select({ address }, { view });
    const head = h('div', { class: 'pane-head' }, h('span', { class: `objc-kind k-${e.kind}` }, e.kind === 'class' ? 'C' : e.kind === 'category' ? '+' : 'P'), h('b', { class: 'nm' }, i.name), h('span', { class: 'muted' }, e.kind === 'class' && e.swift ? 'Swift class' : e.kind), h('span', { class: 'spacer' }));
    const meta = h('span', { class: 'addr link', title: 'Its metadata' }, fmtAddr(i.address));
    meta.addEventListener('click', () => open(i.address, 'hex'));
    head.appendChild(meta);
    const lines = i.lines.map((l) => {
      const line = h('div', { class: 'objc-line' });
      const text = h('span', { class: `objc-text${l.address !== undefined ? ' link' : ''}` }, l.text);
      line.appendChild(text);
      if (l.address !== undefined) {
        const a = l.address;
        text.title = 'Open the implementation';
        text.addEventListener('click', () => open(a, 'code'));
      }
      if (l.selector) {
        const sel = l.selector;
        const senders = h('button', { class: 'btn tiny', type: 'button', title: `The functions that send ${sel}` }, 'Senders');
        const out = h('div', { class: 'objc-uses' });
        senders.addEventListener('click', () => {
          if (out.isConnected) {
            out.remove();
            return;
          }
          line.after(out);
          void this.fillSenders(sel, out);
        });
        line.append(h('span', { class: 'spacer' }), senders);
      }
      if (l.address !== undefined) {
        const a = l.address;
        const addr = h('span', { class: 'addr link' }, fmtAddr(a));
        addr.addEventListener('click', () => open(a, 'code'));
        line.appendChild(addr);
      }
      return line;
    });
    detail.replaceChildren(head, h('div', { class: 'objc-interface' }, ...lines));
  }

  private async fillSenders(selector: string, out: HTMLElement) {
    out.replaceChildren(h('div', { class: 'muted' }, store.xrefs === 'ready' ? 'Looking…' : 'Finding every reference in the code…'));
    await store.ensureXrefs();
    const uses = await store.api.objcSelector(selector);
    out.replaceChildren(...selectorUses(selector, uses));
  }

  private renderImports() {
    if (this.imports.length === 0) {
      this.body.replaceChildren(emptyState('No imports', 'This binary does not import symbols from other libraries (or is statically linked).'));
      return;
    }
    const cols = 'grid-template-columns:minmax(120px,0.35fr) 19ch 8ch minmax(0,1fr)';
    const list = new VList({
      rowHeight: 24,
      renderRow: (i) => {
        const imp = this.imports[i];
        const row = h('div', { class: 'list-row', style: `display:grid;${cols}` }, h('span', { class: 'nm secondary' }, imp.library), h('span', { class: 'addr' }, imp.address !== undefined ? hex(imp.address) : ''), h('span', { class: 'muted' }, imp.ordinal !== undefined ? `#${imp.ordinal}` : ''), h('span', { class: 'nm' }, imp.demangled ?? imp.name));
        if (imp.address !== undefined) {
          const a = imp.address;
          row.addEventListener('click', () => void store.select({ address: a }, { origin: 'symbols' }));
        }
        return row;
      },
    });
    const head = h('div', { class: 'list-row', style: `display:grid;${cols};font-weight:600;color:var(--text-2);cursor:default;border-bottom:1px solid var(--border)` }, h('span', null, 'Library'), h('span', null, 'Slot address'), h('span', null, 'Ordinal'), h('span', null, 'Name'));
    this.body.replaceChildren(h('div', { class: 'toolbar' }, h('span', { class: 'secondary' }, `${formatCount(this.imports.length)} imports from ${new Set(this.imports.map((i) => i.library)).size} libraries`)), head, list.el);
    list.setCount(this.imports.length);
  }

  private renderExports() {
    if (this.exports.length === 0) {
      this.body.replaceChildren(emptyState('No exports', 'Nothing is exported for other modules to link against.'));
      return;
    }
    const cols = 'grid-template-columns:19ch 8ch minmax(0,1fr) minmax(0,0.5fr)';
    const list = new VList({
      rowHeight: 24,
      renderRow: (i) => {
        const e = this.exports[i];
        const row = h('div', { class: 'list-row', style: `display:grid;${cols}` }, h('span', { class: 'addr' }, e.address ? hex(e.address) : ''), h('span', { class: 'muted' }, e.ordinal !== undefined ? `#${e.ordinal}` : ''), h('span', { class: 'nm' }, e.demangled ?? e.name), h('span', { class: 'nm secondary' }, e.forwarder ? `→ ${e.forwarder}` : ''));
        if (e.address) row.addEventListener('click', () => void store.select({ address: e.address }, { origin: 'symbols' }));
        return row;
      },
    });
    const head = h('div', { class: 'list-row', style: `display:grid;${cols};font-weight:600;color:var(--text-2);cursor:default;border-bottom:1px solid var(--border)` }, h('span', null, 'Address'), h('span', null, 'Ordinal'), h('span', null, 'Name'), h('span', null, 'Forwarded to'));
    this.body.replaceChildren(h('div', { class: 'toolbar' }, h('span', { class: 'secondary' }, `${formatCount(this.exports.length)} exports`)), head, list.el);
    list.setCount(this.exports.length);
  }

  private renderStrings() {
    const filter = h('input', { class: 'field small', type: 'search', placeholder: 'Filter strings', value: this.stringFilter, style: 'width:260px' });
    filter.addEventListener(
      'input',
      debounce(() => {
        this.stringFilter = filter.value;
        this.resetStrings();
      }, 120),
    );
    this.strCount = h('span', { class: 'secondary' });
    const cols = 'grid-template-columns:19ch 12ch 7ch minmax(0,1fr)';
    this.strList = new VList({ rowHeight: 24, renderRow: (i) => this.strRow(i, cols) });
    const head = h('div', { class: 'list-row', style: `display:grid;${cols};font-weight:600;color:var(--text-2);cursor:default;border-bottom:1px solid var(--border);background:var(--surface)` }, h('span', null, 'Address'), h('span', null, 'Section'), h('span', null, 'Kind'), h('span', null, 'Text'));
    this.body.replaceChildren(
      h('div', { class: 'toolbar' }, filter, h('span', { class: 'muted', style: 'font-size:12px' }, 'Printable ASCII and UTF-16 runs of 4+ characters in data sections'), h('span', { class: 'spacer' }), this.strCount),
      head,
      this.strList.el,
    );
    this.resetStrings();
  }

  private resetStrings() {
    this.strPages.clear();
    this.strPending.clear();
    this.strGeneration++;
    this.fetchStrings(0, true);
  }

  private fetchStrings(page: number, reset = false) {
    if (this.strPages.has(page) || this.strPending.has(page)) return;
    this.strPending.add(page);
    const gen = this.strGeneration;
    void store.api.strings(this.stringFilter, page * PAGE, PAGE).then((res) => {
      if (gen !== this.strGeneration) return;
      this.strPending.delete(page);
      this.strPages.set(page, res.strings);
      if (this.strCount) this.strCount.textContent = `${formatCount(res.total)} strings`;
      if (reset) this.strList?.setCount(res.total);
      else this.strList?.refresh();
    });
  }

  private strRow(i: number, cols: string): HTMLElement {
    const page = Math.floor(i / PAGE);
    const s = this.strPages.get(page)?.[i % PAGE];
    if (!s) {
      this.fetchStrings(page);
      return h('div', { class: 'list-row muted' }, '…');
    }
    const sec = s.section !== undefined ? store.file?.sections[s.section] : undefined;
    const row = h(
      'div',
      { class: 'list-row', style: `display:grid;${cols}`, title: s.text },
      h('span', { class: 'addr' }, s.address !== undefined ? fmtAddr(s.address) : `@${hex(s.offset)}`),
      h('span', { class: 'mono secondary' }, sec?.name ?? ''),
      h('span', { class: 'muted' }, s.wide ? 'UTF-16' : 'ASCII'),
      h('span', { class: 'nm' }, s.text),
    );
    const target = s.address !== undefined ? { address: s.address } : { offset: s.offset };
    row.addEventListener('click', () => void store.select(target, { origin: 'symbols' }));
    row.addEventListener('dblclick', () => void store.select(target, { view: 'hex' }));
    return row;
  }

  protected onSelection() {
    if (this.tab === 'symbols' && this.list) this.list.refresh();
  }
}
