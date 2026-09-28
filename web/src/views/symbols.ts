// Symbols, imports and exports.
import { store } from '../store';
import type { Export, FoundString, Import, Sym, SymbolQuery } from '../types';
import { emptyState } from '../ui';
import { debounce, formatCount, h, hex, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';

const PAGE = 200;
const COLS = 'grid-template-columns:19ch 9ch 9ch 8ch 8ch minmax(0,1fr)';

type Tab = 'symbols' | 'imports' | 'exports' | 'strings';

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

  constructor() {
    super('symbols', true);
    // Names given in the inspector show up here.
    store.on('annotations', () => {
      if (this.visible && this.tab === 'symbols' && this.list) this.update({});
    });
  }

  protected render() {
    const tabs = h('div', { class: 'tabs' });
    for (const t of ['symbols', 'imports', 'exports', 'strings'] as const) {
      const b = h('button', { class: `tab${this.tab === t ? ' active' : ''}` }, t[0].toUpperCase() + t.slice(1));
      b.addEventListener('click', () => {
        this.tab = t;
        tabs.querySelectorAll('.tab').forEach((x) => x.classList.toggle('active', x === b));
        this.renderBody();
      });
      tabs.appendChild(b);
    }
    this.body = h('div', { style: 'display:flex;flex-direction:column;flex:1;min-height:0' });
    this.el.replaceChildren(h('div', { class: 'toolbar' }, tabs), this.body);
    void Promise.all([store.api.imports(), store.api.exports()]).then(([i, e]) => {
      this.imports = i;
      this.exports = e;
      if (this.tab !== 'symbols') this.renderBody();
    });
    this.renderBody();
  }

  private renderBody() {
    if (this.tab === 'symbols') this.renderSymbols();
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
      h('span', { class: 'addr' }, s.defined ? hex(s.address) : '—'),
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
      h('span', { class: 'addr' }, s.address !== undefined ? hex(s.address) : `@${hex(s.offset)}`),
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
