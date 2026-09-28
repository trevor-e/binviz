// DWARF browser: compilation units, the DIE tree (or a unit's DIEs by tag),
// each DIE with its attributes, the source behind it and its layout, line
// tables, and a check for everything that can't be read or doesn't add up.
import { store } from '../store';
import type { AttrInfo, CodeLine, DieDetails, DieSummary, DwarfCheck, DwarfProblem, LineProgramInfo, LineRow, Link, MemberLayout, SourceLoc, TagCount, UnitInfo } from '../types';
import { LINE_FLAGS } from '../types';
import { emptyState, toast } from '../ui';
import { basename, debounce, formatCount, formatSize, h, hex, icon, num, parseNumber } from '../util';
import { VList } from '../vlist';
import { View } from './base';

interface TreeRow {
  die: DieSummary;
  depth: number;
  open: boolean;
}

type Mode = 'dies' | 'lines' | 'problems';

const LINE_PAGE = 500;
const LIST_PAGE = 200;
/** Kinds offered by the tag filter, before the unit's own tags. */
const KINDS: [string, string][] = [
  ['', 'Tree: all DIEs'],
  ['functions', 'Functions'],
  ['variables', 'Variables & parameters'],
  ['types', 'Types'],
  ['scopes', 'Namespaces & blocks'],
];
/** `0x1a2b` or `<0x1a2b>`: a .debug_info offset, as llvm-dwarfdump prints them. */
const OFFSET = /^<?0x[0-9a-f]+>?$/i;

export class DwarfView extends View {
  private units: UnitInfo[] = [];
  private shownUnits: number[] = [];
  private unitList!: VList;
  private unit = -1;
  private tree: TreeRow[] = [];
  private treeList!: VList;
  private middle!: HTMLElement;
  private details!: HTMLElement;
  private tabs!: HTMLElement;
  private problemTab!: HTMLButtonElement;
  private selected?: { unit: number; offset: bigint };
  private mode: Mode = 'dies';
  private lineProgram?: LineProgramInfo;
  private linePages = new Map<number, LineRow[]>();
  private lineList?: VList;
  private searchResults: DieSummary[] | null = null;
  private searchQuery = '';
  private searchedAll = false;
  /** The tag filter; empty shows the tree. */
  private tagFilter = '';
  private nameFilter = '';
  private listTotal = 0;
  private listPages = new Map<number, DieSummary[]>();
  private listPending = new Set<number>();
  private listGen = 0;
  private tagCounts: TagCount[] = [];
  private check: DwarfCheck | null = null;
  private checking = false;
  private loadProblems: DwarfProblem[] = [];
  private problemList?: VList;

  constructor() {
    super('dwarf', true);
    store.on('intent', () => {
      if (this.visible && store.intent.die) void this.consumeIntent();
    });
    store.on('sources', () => {
      if (this.visible && this.selected) void this.selectDie(this.selected.unit, this.selected.offset);
    });
  }

  protected render() {
    const f = store.file!;
    this.unit = -1;
    this.selected = undefined;
    this.tree = [];
    this.searchResults = null;
    this.tagFilter = '';
    this.nameFilter = '';
    this.check = null;
    this.loadProblems = [];
    if (this.mode === 'problems') this.mode = 'dies';
    if (!f.dwarf) {
      const btn = h('button', { class: 'btn' }, 'Add debug file…');
      btn.addEventListener('click', () => window.dispatchEvent(new CustomEvent('binviz:attach-debug')));
      this.el.replaceChildren(emptyState('No DWARF debug information', 'Rebuild with debug info (-g), or add a separate debug file: a .dSYM’s DWARF file, a .debug file, or an unstripped copy.', btn));
      return;
    }
    const search = h('input', { class: 'field small', type: 'search', placeholder: 'Search DIEs, or 0x… offset', style: 'width:240px', 'aria-label': 'Search DIEs by name, or go to a .debug_info offset' });
    search.addEventListener('input', debounce(() => void this.search(search.value), 250));
    search.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && OFFSET.test(search.value.trim())) void this.goToOffset(search.value.trim());
    });
    this.tabs = h('div', { class: 'tabs' });
    const tab = (m: Mode, label: string) => {
      const b = h('button', { class: `tab${this.mode === m ? ' active' : ''}`, type: 'button' }, label);
      b.addEventListener('click', () => this.setMode(m));
      b.dataset.mode = m;
      this.tabs.appendChild(b);
      return b;
    };
    tab('dies', 'Debug info (DIEs)');
    tab('lines', 'Line table');
    this.problemTab = tab('problems', 'Problems');
    const unitFilter = h('input', { class: 'field small', type: 'search', placeholder: 'Filter units', style: 'width:100%' });
    unitFilter.addEventListener('input', debounce(() => this.filterUnits(unitFilter.value), 100));
    this.unitList = new VList({ rowHeight: 40, renderRow: (i) => this.unitRow(i) });
    this.treeList = new VList({ rowHeight: 24, renderRow: (i) => this.dieRowAt(i) });
    this.middle = h('div', { class: 'pane' });
    this.details = h('div', { class: 'pane' });
    const d = f.dwarf;
    this.el.replaceChildren(
      h('div', { class: 'toolbar' }, h('h3', null, 'DWARF'), h('span', { class: 'secondary' }, `${formatCount(d.unitCount)} units · v${d.versions.join('/')} · ${d.source === 'embedded' ? 'embedded' : 'from ' + d.source}`), this.tabs, h('span', { class: 'spacer' }), search),
      h('div', { class: 'dwarf-cols' }, h('div', { class: 'pane' }, h('div', { class: 'pane-head' }, unitFilter), this.unitList.el), this.middle, this.details),
    );
    this.details.replaceChildren(h('div', { class: 'pad muted' }, 'Select a DIE to see its attributes.'));
    void store.api.dwarfUnits().then((units) => {
      if (store.file !== f) return;
      this.units = units;
      this.filterUnits('');
      if (store.intent.die) void this.consumeIntent();
      else if (units.length) void this.selectUnit(this.preferredUnit());
    });
    void store.api.dwarfLoadProblems().then((p) => {
      if (store.file !== f) return;
      this.loadProblems = p;
      this.updateProblemTab();
    });
  }

  private setMode(m: Mode) {
    this.mode = m;
    this.tabs.querySelectorAll<HTMLElement>('.tab').forEach((x) => x.classList.toggle('active', x.dataset.mode === m));
    this.renderMiddle();
  }

  /** The first unit that isn't from the standard library, if any. */
  private preferredUnit(): number {
    const sel = store.selection.inspection?.unit;
    if (sel !== undefined) return sel;
    const own = this.units.find((u) => u.name && !/\/rustc\/|[\\/]library[\\/]|\/usr\/|crt|mingw/i.test(u.name));
    return own?.index ?? 0;
  }

  private filterUnits(q: string) {
    const n = q.trim().toLowerCase();
    this.shownUnits = this.units.filter((u) => !n || (u.name ?? '').toLowerCase().includes(n) || (u.producer ?? '').toLowerCase().includes(n) || (u.language ?? '').toLowerCase().includes(n)).map((u) => u.index);
    this.unitList.setCount(this.shownUnits.length);
  }

  private unitRow(i: number): HTMLElement {
    const u = this.units[this.shownUnits[i]];
    const name = u.name ? basename(u.name.replace(/\\@\\.*$/, '')) : `unit ${u.index}`;
    const problems = this.check?.byUnit.find((p) => p.unit === u.index);
    const row = h(
      'div',
      { class: `list-row unit-row${u.index === this.unit ? ' selected' : ''}`, title: `${u.name ?? ''}${u.producer ? '\n' + u.producer : ''}` },
      h('span', { class: 'nm', style: 'max-width:100%' }, problems ? h('span', { class: `sev-dot ${problems.errors ? 'error' : 'warning'}`, title: `${problems.errors} errors, ${problems.warnings} warnings` }) : null, name),
      h('span', { class: 'muted', style: 'font-size:11px' }, `${u.language ?? u.kind} · v${u.version} · ${formatSize(u.size)}${u.codeSize > 0n ? ` · ${formatSize(u.codeSize)} code` : ''}`),
    );
    row.addEventListener('click', () => void this.selectUnit(u.index));
    return row;
  }

  private async selectUnit(index: number, andRender = true) {
    this.unit = index;
    this.searchResults = null;
    this.lineProgram = undefined;
    this.linePages.clear();
    this.tagCounts = [];
    this.resetListing();
    this.unitList.refresh();
    const [root, children] = await Promise.all([store.api.unitRoot(index), store.api.dieChildren(index)]);
    if (this.unit !== index) return;
    this.tree = root ? [{ die: root, depth: 0, open: true }, ...children.map((d) => ({ die: d, depth: 1, open: false }))] : [];
    if (andRender) this.renderMiddle();
    if (root && andRender) void this.selectDie(index, root.offset);
    void store.api.tagCounts(index).then((c) => {
      if (this.unit !== index) return;
      this.tagCounts = c;
      if (this.mode === 'dies' && !this.searchResults) this.renderMiddle();
    });
  }

  // --- The middle pane: tree, listing or search results ------------------------

  private renderMiddle() {
    if (this.mode === 'lines') {
      void this.renderLines();
      return;
    }
    if (this.mode === 'problems') {
      this.renderProblems();
      return;
    }
    const head = h('div', { class: 'pane-head', style: 'flex-wrap:wrap' });
    if (this.searchResults) {
      const back = h('button', { class: 'btn small', type: 'button' }, 'Back to tree');
      back.addEventListener('click', () => {
        this.searchResults = null;
        this.renderMiddle();
      });
      const all = h('button', { class: 'btn small', type: 'button', title: 'Every DIE with a matching name, locals and parameters included (reads all of them)' }, 'Include locals');
      all.addEventListener('click', () => void this.search(this.searchQuery, true));
      head.append(h('strong', null, `${formatCount(this.searchResults.length)} match${this.searchResults.length === 1 ? '' : 'es'}`), h('span', { class: 'spacer', style: 'flex:1' }), ...(this.searchedAll ? [] : [all]), back);
    } else {
      const select = h('select', { class: 'field small', 'aria-label': 'Show DIEs by tag', style: 'max-width:220px' });
      for (const [value, label] of KINDS) select.appendChild(h('option', { value, selected: value === this.tagFilter }, label));
      if (this.tagCounts.length) {
        const group = h('optgroup', { label: 'Tags in this unit' });
        for (const t of this.tagCounts) group.appendChild(h('option', { value: t.tag, selected: t.tag === this.tagFilter }, `${t.tag.replace('DW_TAG_', '')} (${formatCount(t.count)})`));
        select.appendChild(group);
      }
      select.addEventListener('change', () => {
        this.tagFilter = select.value;
        this.resetListing();
        this.renderMiddle();
      });
      head.appendChild(select);
      if (this.tagFilter) {
        const name = h('input', { class: 'field small', type: 'search', placeholder: 'Name contains', value: this.nameFilter, style: 'width:130px', 'aria-label': 'Only DIEs whose name contains' });
        name.addEventListener('input', debounce(() => {
          this.nameFilter = name.value;
          this.resetListing();
          this.fetchListing(0, true);
        }, 200));
        head.append(name, h('span', { class: 'muted listing-count' }, this.listTotal ? `${formatCount(this.listTotal)} DIEs` : ''));
      }
    }
    this.middle.replaceChildren(head, this.treeList.el);
    if (this.searchResults) this.treeList.setCount(this.searchResults.length);
    else if (this.tagFilter) this.fetchListing(0, true);
    else this.treeList.setCount(this.tree.length);
  }

  private resetListing() {
    this.listPages.clear();
    this.listPending.clear();
    this.listTotal = 0;
    this.listGen++;
  }

  private fetchListing(page: number, reset = false) {
    if (this.listPages.has(page) || this.listPending.has(page) || this.unit < 0) return;
    this.listPending.add(page);
    const gen = this.listGen;
    void store.api.listDies(this.unit, this.tagFilter, this.nameFilter, page * LIST_PAGE, LIST_PAGE).then((res) => {
      if (gen !== this.listGen) return;
      this.listPending.delete(page);
      this.listPages.set(page, res.dies);
      this.listTotal = res.total;
      const count = this.middle.querySelector('.listing-count');
      if (count) count.textContent = `${formatCount(res.total)} DIE${res.total === 1 ? '' : 's'}`;
      if (reset) this.treeList.setCount(res.total);
      else this.treeList.refresh();
    });
  }

  private dieRowAt(i: number): HTMLElement {
    if (this.searchResults) return this.dieRow(this.searchResults[i], 0, false, i);
    if (this.tagFilter) {
      const page = Math.floor(i / LIST_PAGE);
      const d = this.listPages.get(page)?.[i % LIST_PAGE];
      if (!d) {
        this.fetchListing(page);
        return h('div', { class: 'tree-row muted' }, '…');
      }
      return this.dieRow(d, 0, false, i);
    }
    const row = this.tree[i];
    return this.dieRow(row.die, row.depth, row.open, i, true);
  }

  private dieRow(d: DieSummary, depth: number, open: boolean, i: number, tree = false): HTMLElement {
    const selected = this.selected && this.selected.unit === d.unit && this.selected.offset === d.offset;
    const el = h(
      'div',
      { class: `tree-row${open ? ' open' : ''}${selected ? ' selected' : ''}`, style: `padding-left:${6 + depth * 14}px`, title: `${d.tag}${d.scope ? `\nin ${d.scope}` : ''}\n.debug_info ${hex(d.sectionOffset)}` },
      h('span', { class: 'twisty' }, tree && d.hasChildren ? icon('chevron') : null),
      h(
        'span',
        { class: 'label' },
        h('span', { class: 'die-tag' }, d.tag.replace('DW_TAG_', '')),
        d.scope ? h('span', { class: 'die-scope' }, `${d.scope}::`) : null,
        d.name ? h('span', { class: 'die-name' }, d.name) : null,
        d.detail ? h('span', { class: 'die-detail' }, d.detail) : null,
      ),
      h('span', { class: 'die-off' }, hex(d.sectionOffset)),
    );
    if (tree) {
      el.querySelector('.twisty')!.addEventListener('click', (e) => {
        e.stopPropagation();
        void this.toggle(i);
      });
      el.addEventListener('dblclick', () => void this.toggle(i));
    }
    el.addEventListener('click', () => void (tree ? this.selectDie(d.unit, d.offset) : this.reveal(d.unit, d.offset, false)));
    return el;
  }

  private async toggle(i: number) {
    const row = this.tree[i];
    if (!row.die.hasChildren) return;
    if (row.open) {
      let end = i + 1;
      while (end < this.tree.length && this.tree[end].depth > row.depth) end++;
      this.tree.splice(i + 1, end - i - 1);
      row.open = false;
    } else {
      const kids = await store.api.dieChildren(row.die.unit, row.die.offset);
      row.open = true;
      this.tree.splice(i + 1, 0, ...kids.map((d) => ({ die: d, depth: row.depth + 1, open: false })));
    }
    this.treeList.setCount(this.tree.length);
  }

  private async selectDie(unit: number, offset: bigint) {
    this.selected = { unit, offset };
    this.treeList.refresh();
    const d = await store.api.die(unit, offset);
    if (!d || this.selected.unit !== unit || this.selected.offset !== offset) return;
    this.renderDetails(d);
  }

  /**
   * Shows a DIE: in the tree (expanding its parents) when `inTree`, otherwise
   * just its details, leaving a listing or search results in place.
   */
  private async reveal(unit: number, offset: bigint, inTree = true) {
    if (!inTree) {
      if (this.unit !== unit) {
        this.unit = unit;
        this.unitList.refresh();
      }
      await this.selectDie(unit, offset);
      return;
    }
    if (this.unit !== unit || this.searchResults || this.tagFilter || this.mode !== 'dies') {
      this.tagFilter = '';
      this.searchResults = null;
      await this.selectUnit(unit, false);
      this.setMode('dies');
    }
    const d = await store.api.die(unit, offset);
    if (!d) return;
    for (const p of d.parents) {
      const i = this.tree.findIndex((r) => r.die.offset === p.offset);
      if (i >= 0 && !this.tree[i].open) await this.toggle(i);
    }
    const i = this.tree.findIndex((r) => r.die.offset === offset);
    this.selected = { unit, offset };
    if (i >= 0) this.treeList.scrollToIndex(i, 'center');
    this.treeList.refresh();
    this.renderDetails(d);
  }

  private async consumeIntent() {
    const target = store.intent.die;
    // Units still loading: leave the intent for render() to pick up.
    if (!target || this.units.length === 0) return;
    store.intent = {};
    await this.reveal(target.unit, target.offset);
  }

  private async search(q: string, all = false) {
    const query = q.trim();
    if (!query || OFFSET.test(query)) {
      if (this.searchResults) {
        this.searchResults = null;
        this.renderMiddle();
      }
      return;
    }
    this.searchQuery = query;
    this.searchedAll = all;
    if (this.mode !== 'dies') this.setMode('dies');
    this.searchResults = all ? await store.api.dieSearchAll(query, 1000) : await store.api.dieSearch(query, 500);
    this.renderMiddle();
  }

  private async goToOffset(text: string) {
    const off = parseNumber(text.replace(/[<>]/g, ''));
    if (off === undefined) return;
    const at = await store.api.dieAtOffset(off);
    if (!at) {
      toast(`No unit in .debug_info covers ${hex(off)}`, 'error');
      return;
    }
    await this.reveal(at[0], at[1]);
  }

  // --- Details -----------------------------------------------------------------

  private renderDetails(d: DieDetails) {
    const u = this.units[d.die.unit];
    const crumbs = h('div', { class: 'crumbs' });
    for (const p of d.parents) {
      const c = h('span', { class: 'link' }, p.name ?? p.tag.replace('DW_TAG_', ''));
      c.addEventListener('click', () => void this.reveal(p.unit, p.offset));
      crumbs.append(c, h('span', { class: 'muted' }, '›'));
    }
    const facts: [string, Node | string][] = [];
    if (d.typeName) facts.push(['Type', h('span', { class: 'mono' }, d.typeName)]);
    if (d.byteSize !== undefined) facts.push(['Size', `${num(d.byteSize)} bytes`]);
    if (d.decl) facts.push(['Declared at', this.sourceLink(d.decl)]);
    if (d.callSite) facts.push(['Inlined at', this.sourceLink(d.callSite)]);
    if (d.ranges.length) {
      const list = h('div', { style: 'display:flex;flex-direction:column;gap:2px' });
      for (const [a, b] of d.ranges.slice(0, 16)) {
        const l = h('span', { class: 'link mono' }, `${hex(a)}..${hex(b)}`);
        l.addEventListener('click', () => void store.select({ address: a }, { view: 'code' }));
        list.appendChild(h('span', null, l, h('span', { class: 'muted' }, ` (${num(b - a)} bytes)`)));
      }
      if (d.ranges.length > 16) list.appendChild(h('span', { class: 'muted' }, `… ${d.ranges.length - 16} more`));
      facts.push(['Code', list]);
    }
    if (d.childCount) facts.push(['Children', formatCount(d.childCount)]);
    const bytesLink = this.bytesLink(d.section, d.byteStart, 'show bytes');
    facts.push(['Encoded at', h('span', null, h('span', { class: 'mono' }, `${d.section} + ${hex(d.byteStart)}..${hex(d.byteEnd)}`), ' ', bytesLink)]);
    facts.push(['Offset', h('span', { class: 'mono' }, `${hex(d.die.offset)} in unit ${d.die.unit}`)]);

    const sources = h('div', { class: 'die-sources' });
    const snippets: [string, SourceLoc][] = [];
    if (d.decl) snippets.push(['Declaration', d.decl]);
    if (d.callSite) snippets.push(['Call site', d.callSite]);
    for (const [label, loc] of snippets) void this.snippet(sources, label, loc);

    const attrs = h('table', { class: 'attrs' });
    for (const a of d.attributes) attrs.appendChild(this.attrRow(a, d.section));

    const heading = (text: string) => h('h3', { class: 'die-h' }, text);
    this.details.replaceChildren(
      h(
        'div',
        { class: 'scroll pad' },
        crumbs,
        h('div', { style: 'margin:8px 0 4px' }, h('span', { class: 'die-tag', style: 'font-size:12px' }, d.die.tag), ' ', h('strong', { class: 'mono', style: 'font-size:14px' }, d.die.name ?? '')),
        u?.name ? h('div', { class: 'muted', style: 'font-size:12px;margin-bottom:8px;overflow-wrap:anywhere' }, `in ${u.name}`) : null,
        h('dl', { class: 'facts', style: 'margin-bottom:12px' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
        sources,
        d.codeLines.length ? [heading(`Source lines of its code (${d.codeLines.length}${d.codeLines.length >= 400 ? '+' : ''})`), this.codeLines(d.codeLines)] : null,
        d.layout.length ? [heading(`Layout (${d.layout.length} members)`), this.layoutTable(d.layout, d.byteSize, d.tailPadding)] : null,
        heading(`Attributes (${d.attributes.length})`),
        attrs,
      ),
    );
  }

  private sourceLink(loc: SourceLoc): HTMLElement {
    const l = h('span', { class: 'link mono', title: loc.path }, `${basename(loc.path)}:${loc.line}${loc.column ? ':' + loc.column : ''}`);
    l.addEventListener('click', () => store.openSource(loc.file, loc.line));
    return l;
  }

  /** A few lines of source around `loc`, once the text is loaded (or embedded). */
  private async snippet(host: HTMLElement, label: string, loc: SourceLoc) {
    const text = await store.sourceText(loc.file);
    const box = h('div', { class: 'die-snippet' }, h('div', { class: 'die-snippet-head' }, h('span', null, label), ' ', this.sourceLink(loc)));
    if (text === undefined || loc.line === 0) {
      box.appendChild(h('div', { class: 'muted', style: 'font-size:12px;padding:4px 8px' }, loc.line === 0 ? 'No line number.' : 'Load the source folder (toolbar) to see the code here.'));
    } else {
      const lines = text.split(/\r?\n/);
      const pre = h('div', { class: 'code-snippet' });
      for (let l = Math.max(1, loc.line - 2); l <= Math.min(lines.length, loc.line + 3); l++) {
        pre.appendChild(h('span', { class: l === loc.line ? 'hl' : '' }, `${String(l).padStart(5)}  ${lines[l - 1] ?? ''}\n`));
      }
      box.appendChild(pre);
    }
    host.appendChild(box);
  }

  private codeLines(lines: CodeLine[]): HTMLElement {
    const table = h('table', { class: 'attrs code-lines' });
    for (const l of lines.slice(0, 200)) {
      const where = h('span', { class: 'link mono', title: l.path }, `${basename(l.path)}:${l.line}`);
      where.addEventListener('click', () => store.openSource(l.file, l.line));
      const first = h('span', { class: 'link mono' }, hex(l.first));
      first.addEventListener('click', () => void store.select({ address: l.first }, { view: 'code' }));
      table.appendChild(h('tr', null, h('td', null, where), h('td', { class: 'num' }, `${formatCount(num(l.bytes))} B`), h('td', null, first), h('td', { class: 'muted' }, l.rows > 1 ? `${l.rows} ranges` : '')));
    }
    if (lines.length > 200) table.appendChild(h('tr', null, h('td', { class: 'muted', colspan: '4' }, `… ${lines.length - 200} more`)));
    return table;
  }

  private layoutTable(layout: MemberLayout[], size?: bigint, tail?: bigint): HTMLElement {
    const table = h('table', { class: 'attrs layout' }, h('tr', null, h('th', null, 'Offset'), h('th', null, 'Size'), h('th', null, 'Member'), h('th', null, 'Type')));
    for (const m of layout) {
      if (m.hole > 0n) table.appendChild(h('tr', { class: 'hole' }, h('td', { colspan: '4' }, `${num(m.hole)} byte${m.hole === 1n ? '' : 's'} of padding`)));
      const at = m.bitOffset !== undefined && m.bitSize !== undefined ? `${hex(m.bitOffset / 8n)}:${num(m.bitOffset % 8n)}` : m.offset !== undefined ? hex(m.offset) : m.kind;
      const size = m.bitSize !== undefined ? `${num(m.bitSize)} bits` : m.size !== undefined ? String(num(m.size)) : '';
      const name = h('span', { class: 'link mono' }, m.name ?? (m.kind === 'base' ? m.typeName : '(anonymous)'));
      name.addEventListener('click', () => void this.reveal(m.unit, m.die));
      table.appendChild(
        h(
          'tr',
          { class: m.kind === 'static' ? 'muted' : '' },
          h('td', { class: 'mono' }, at),
          h('td', { class: 'num' }, size),
          h('td', null, m.kind === 'base' ? h('span', { class: 'muted' }, 'base ') : null, name, m.artificial ? h('span', { class: 'muted' }, ' (compiler)') : null),
          h('td', { class: 'mono' }, m.typeName),
        ),
      );
    }
    if (tail !== undefined && tail > 0n) table.appendChild(h('tr', { class: 'hole' }, h('td', { colspan: '4' }, `${num(tail)} byte${tail === 1n ? '' : 's'} of padding at the end${size !== undefined ? ` (size ${num(size)})` : ''}`)));
    return table;
  }

  private bytesLink(section: string, start: bigint, label: string): HTMLElement | null {
    const f = store.file!;
    if (f.dwarf?.source !== 'embedded') return null;
    const name = section.replace(/^\./, '');
    const sec = f.sections.find((s) => (s.name === section || s.name === `__${name}`) && s.fileOffset !== undefined && !s.compressed);
    if (!sec || sec.fileOffset === undefined) return null;
    const off = sec.fileOffset + start;
    const l = h('span', { class: 'link' }, label);
    l.addEventListener('click', () => void store.select({ offset: off }, { view: 'hex' }));
    return l;
  }

  private attrRow(a: AttrInfo, section: string): HTMLElement {
    const value = h('td', { class: 'av' });
    if (a.link) {
      const link = h('span', { class: 'link' }, a.value);
      const target: Link = a.link;
      link.addEventListener('click', () => this.follow(target));
      value.appendChild(link);
    } else value.textContent = a.value;
    const form = h('td', { class: 'af', title: a.byteEnd > a.byteStart ? `${num(a.byteEnd - a.byteStart)} bytes at ${section} + ${hex(a.byteStart)}` : 'Stored in the abbreviation, not in the DIE' }, a.form.replace('DW_FORM_', ''));
    const bytes = a.byteEnd > a.byteStart ? this.bytesLink(section, a.byteStart, '') : null;
    if (bytes) {
      form.classList.add('link');
      form.addEventListener('click', () => bytes.click());
    }
    return h('tr', null, h('td', { class: 'an' }, a.name.replace('DW_AT_', '')), form, value);
  }

  private follow(link: Link) {
    if (link.type === 'die') void this.reveal(link.unit, link.offset);
    else if (link.type === 'address') {
      const code = store.file!.sections.some((s) => s.kind === 'code' && s.loaded && link.address >= s.address && link.address < s.address + s.size);
      void store.select({ address: link.address }, { view: code ? 'code' : 'hex' });
    } else store.openSource(link.file, link.line);
  }

  // --- Problems --------------------------------------------------------------

  private updateProblemTab() {
    const c = this.check;
    const errors = c ? c.errors : this.loadProblems.length;
    const warnings = c ? c.warnings : 0;
    this.problemTab.replaceChildren('Problems', errors + warnings > 0 ? h('span', { class: `tab-count ${errors ? 'error' : 'warning'}` }, formatCount(errors + warnings)) : c ? h('span', { class: 'tab-count ok' }, '0') : '');
  }

  private problems(): DwarfProblem[] {
    return this.check ? this.check.problems : this.loadProblems;
  }

  private renderProblems() {
    const c = this.check;
    const run = h('button', { class: 'btn small', type: 'button', disabled: this.checking }, this.checking ? 'Checking…' : c ? 'Check again' : 'Check all DWARF');
    run.addEventListener('click', () => void this.runCheck());
    const summary = c
      ? `${formatCount(c.errors)} error${c.errors === 1 ? '' : 's'}, ${formatCount(c.warnings)} warning${c.warnings === 1 ? '' : 's'} in ${formatCount(c.units)} units · ${formatCount(num(c.dies))} DIEs · ${formatCount(num(c.lineRows))} line rows read`
      : this.loadProblems.length
        ? `${this.loadProblems.length} unit${this.loadProblems.length === 1 ? '' : 's'} couldn't be read at all. Check everything for the rest.`
        : 'Reads every unit, DIE, attribute and line program, and lists what can’t be read or doesn’t add up.';
    const head = h('div', { class: 'pane-head', style: 'flex-wrap:wrap' }, run, h('span', { class: 'muted', style: 'font-size:12px' }, summary));
    const list = this.problems();
    if (list.length === 0) {
      this.middle.replaceChildren(head, c ? emptyState('No problems', 'Everything reads, and every reference resolves.') : h('div'));
      return;
    }
    this.problemList = new VList({ rowHeight: 44, renderRow: (i) => this.problemRow(list[i]) });
    this.middle.replaceChildren(head, this.problemList.el, c?.truncated ? h('div', { class: 'pad muted' }, 'Only the first problems are listed.') : '');
    this.problemList.setCount(list.length);
  }

  private async runCheck() {
    if (this.checking) return;
    this.checking = true;
    this.renderMiddle();
    try {
      this.check = await store.api.dwarfCheck();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
    this.checking = false;
    this.updateProblemTab();
    this.unitList.refresh();
    if (this.mode === 'problems') this.renderMiddle();
  }

  private problemRow(p: DwarfProblem): HTMLElement {
    const where =
      p.unit !== undefined && p.die !== undefined
        ? `unit ${p.unit} · ${p.tag ? p.tag.replace('DW_TAG_', '') + ' ' : ''}${hex(p.offset ?? p.die)}`
        : p.unit !== undefined
          ? `unit ${p.unit} · ${p.section} ${hex(p.offset ?? 0n)}`
          : `${p.section} ${hex(p.offset ?? 0n)}`;
    const row = h(
      'div',
      { class: 'problem-row', title: p.message },
      h('div', { class: 'problem-top' }, h('span', { class: `sev ${p.severity}` }, p.severity), h('span', { class: 'muted' }, p.area), h('span', { class: 'mono muted' }, where)),
      h('div', { class: 'problem-msg' }, p.message),
    );
    row.addEventListener('click', () => {
      if (p.unit !== undefined && p.die !== undefined) void this.reveal(p.unit, p.die);
      else if (p.unit !== undefined && p.area === 'line program') {
        void this.selectUnit(p.unit, false).then(() => this.setMode('lines'));
      } else if (p.offset !== undefined) {
        const link = this.bytesLink(p.section, p.offset, '');
        link?.click();
      }
    });
    return row;
  }

  // --- Line table -------------------------------------------------------------

  private async renderLines() {
    if (this.unit < 0) {
      this.middle.replaceChildren(emptyState('Select a unit'));
      return;
    }
    const unit = this.unit;
    this.lineProgram ??= await store.api.lineProgram(unit);
    if (this.unit !== unit || this.mode !== 'lines') return;
    const p = this.lineProgram;
    if (!p) {
      this.middle.replaceChildren(emptyState('No line program', 'This unit has no DW_AT_stmt_list.'));
      return;
    }
    const files = store.file!.sourceFiles;
    const header = h(
      'details',
      { class: 'pad', style: 'border-bottom:1px solid var(--border);flex:none' },
      h('summary', null, h('strong', null, `Line program v${p.version}`), h('span', { class: 'secondary' }, ` · ${formatCount(p.rowCount)} rows · ${p.files.length} files`)),
      h(
        'dl',
        { class: 'facts', style: 'margin-top:8px;font-size:12px' },
        ...[
          ['Offset', hex(p.offset)],
          ['min_inst_length', String(p.minimumInstructionLength)],
          ['default_is_stmt', String(p.defaultIsStmt)],
          ['line_base / line_range', `${p.lineBase} / ${p.lineRange}`],
          ['opcode_base', String(p.opcodeBase)],
        ].flatMap(([k, v]) => [h('dt', null, k), h('dd', { class: 'mono' }, v)]),
      ),
      h('div', { class: 'mono', style: 'font-size:12px;margin-top:6px' }, ...p.files.map((fe) => h('div', { class: 'secondary' }, `${String(fe.index).padStart(3)}: `, fe.file !== undefined ? files[fe.file]?.path ?? fe.path : fe.path))),
    );
    const cols = 'display:grid;grid-template-columns:18ch minmax(0,1fr) 6ch 5ch 11ch';
    this.lineList = new VList({ rowHeight: 22, renderRow: (i) => this.lineRow(i, cols) });
    this.middle.replaceChildren(
      header,
      h('div', { class: 'list-row', style: `${cols};font-weight:600;color:var(--text-2);cursor:default;border-bottom:1px solid var(--border)` }, h('span', null, 'Address'), h('span', null, 'File'), h('span', null, 'Line'), h('span', null, 'Col'), h('span', null, 'Flags')),
      this.lineList.el,
    );
    this.lineList.setCount(p.rowCount);
  }

  private lineRow(i: number, cols: string): HTMLElement {
    const page = Math.floor(i / LINE_PAGE);
    const rows = this.linePages.get(page);
    if (!rows) {
      const unit = this.unit;
      if (!this.linePages.has(-1 - page)) {
        this.linePages.set(-1 - page, []);
        void store.api.lineRows(unit, page * LINE_PAGE, LINE_PAGE).then((r) => {
          if (this.unit !== unit) return;
          this.linePages.set(page, r);
          this.lineList?.refresh();
        });
      }
      return h('div', { class: 'list-row muted' }, '…');
    }
    const r = rows[i % LINE_PAGE];
    if (!r) return h('div', { class: 'list-row' });
    const f = r.file !== undefined ? store.file!.sourceFiles[r.file] : undefined;
    const flags = [r.flags & LINE_FLAGS.isStmt ? 'stmt' : '', r.flags & LINE_FLAGS.prologueEnd ? 'prologue_end' : '', r.flags & LINE_FLAGS.epilogueBegin ? 'epilogue' : '', r.flags & LINE_FLAGS.endSequence ? 'end_seq' : ''].filter(Boolean).join(' ');
    const sel = store.selection.address;
    const el = h(
      'div',
      { class: `list-row${sel === r.address ? ' selected' : ''}`, style: `${cols};${r.flags & LINE_FLAGS.endSequence ? 'border-bottom:1px solid var(--grid)' : ''}` },
      h('span', { class: 'addr' }, hex(r.address)),
      h('span', { class: 'nm', title: f?.path ?? '' }, f ? f.name : `file ${r.fileIndex}`),
      h('span', { class: 'mono' }, String(r.line)),
      h('span', { class: 'mono muted' }, String(r.column)),
      h('span', { class: 'muted', style: 'font-size:11px' }, flags),
    );
    el.addEventListener('click', () => !(r.flags & LINE_FLAGS.endSequence) && void store.select({ address: r.address }, { origin: 'dwarf' }));
    return el;
  }

  protected onSelection() {
    this.lineList?.refresh();
  }
}
