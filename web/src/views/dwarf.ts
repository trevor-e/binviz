// DWARF browser: compilation units, DIE trees, attributes and line tables.
import { store } from '../store';
import type { AttrInfo, DieDetails, DieSummary, LineProgramInfo, LineRow, Link, UnitInfo } from '../types';
import { LINE_FLAGS } from '../types';
import { emptyState } from '../ui';
import { basename, debounce, formatCount, formatSize, h, hex, icon, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';

interface TreeRow {
  die: DieSummary;
  depth: number;
  open: boolean;
}

const LINE_PAGE = 500;

export class DwarfView extends View {
  private units: UnitInfo[] = [];
  private shownUnits: number[] = [];
  private unitList!: VList;
  private unit = -1;
  private tree: TreeRow[] = [];
  private treeList!: VList;
  private middle!: HTMLElement;
  private details!: HTMLElement;
  private selected?: { unit: number; offset: bigint };
  private mode: 'dies' | 'lines' = 'dies';
  private lineProgram?: LineProgramInfo;
  private linePages = new Map<number, LineRow[]>();
  private lineList?: VList;
  private searchResults: DieSummary[] | null = null;

  constructor() {
    super('dwarf', true);
    store.on('intent', () => {
      if (this.visible && store.intent.die) void this.consumeIntent();
    });
  }

  protected render() {
    const f = store.file!;
    this.unit = -1;
    this.selected = undefined;
    this.tree = [];
    this.searchResults = null;
    if (!f.dwarf) {
      const btn = h('button', { class: 'btn' }, 'Add debug file…');
      btn.addEventListener('click', () => window.dispatchEvent(new CustomEvent('binviz:attach-debug')));
      this.el.replaceChildren(emptyState('No DWARF debug information', 'Rebuild with debug info (-g), or add a separate debug file: a .dSYM’s DWARF file, a .debug file, or an unstripped copy.', btn));
      return;
    }
    const search = h('input', { class: 'field small', type: 'search', placeholder: 'Search DIEs by name', style: 'width:240px' });
    search.addEventListener('input', debounce(() => void this.search(search.value), 250));
    const tabs = h('div', { class: 'tabs' });
    for (const m of ['dies', 'lines'] as const) {
      const b = h('button', { class: `tab${this.mode === m ? ' active' : ''}` }, m === 'dies' ? 'Debug info (DIEs)' : 'Line table');
      b.addEventListener('click', () => {
        this.mode = m;
        tabs.querySelectorAll('.tab').forEach((x) => x.classList.toggle('active', x === b));
        this.renderMiddle();
      });
      tabs.appendChild(b);
    }
    const unitFilter = h('input', { class: 'field small', type: 'search', placeholder: 'Filter units', style: 'width:100%' });
    unitFilter.addEventListener('input', debounce(() => this.filterUnits(unitFilter.value), 100));
    this.unitList = new VList({ rowHeight: 40, renderRow: (i) => this.unitRow(i) });
    this.treeList = new VList({ rowHeight: 24, renderRow: (i) => this.treeRow(i) });
    this.middle = h('div', { class: 'pane' });
    this.details = h('div', { class: 'pane' });
    const d = f.dwarf;
    this.el.replaceChildren(
      h('div', { class: 'toolbar' }, h('h3', null, 'DWARF'), h('span', { class: 'secondary' }, `${formatCount(d.unitCount)} units · v${d.versions.join('/')} · ${d.source === 'embedded' ? 'embedded' : 'from ' + d.source}`), tabs, h('span', { class: 'spacer' }), search),
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
    this.shownUnits = this.units.filter((u) => !n || (u.name ?? '').toLowerCase().includes(n) || (u.producer ?? '').toLowerCase().includes(n)).map((u) => u.index);
    this.unitList.setCount(this.shownUnits.length);
  }

  private unitRow(i: number): HTMLElement {
    const u = this.units[this.shownUnits[i]];
    const name = u.name ? basename(u.name.replace(/\\@\\.*$/, '')) : `unit ${u.index}`;
    const row = h(
      'div',
      { class: `list-row${u.index === this.unit ? ' selected' : ''}`, style: 'height:40px;flex-direction:column;align-items:flex-start;justify-content:center;gap:0', title: u.name ?? '' },
      h('span', { class: 'nm', style: 'max-width:100%' }, name),
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
    this.unitList.refresh();
    const [root, children] = await Promise.all([store.api.unitRoot(index), store.api.dieChildren(index)]);
    if (this.unit !== index) return;
    this.tree = root ? [{ die: root, depth: 0, open: true }, ...children.map((d) => ({ die: d, depth: 1, open: false }))] : [];
    if (andRender) this.renderMiddle();
    if (root && andRender) void this.selectDie(index, root.offset);
  }

  private renderMiddle() {
    if (this.mode === 'lines') {
      void this.renderLines();
      return;
    }
    const title = this.searchResults ? `${this.searchResults.length} matches` : 'DIE tree';
    const back = this.searchResults ? h('button', { class: 'btn small' }, 'Back to tree') : null;
    back?.addEventListener('click', () => {
      this.searchResults = null;
      this.renderMiddle();
    });
    this.middle.replaceChildren(h('div', { class: 'pane-head' }, h('strong', null, title), h('span', { class: 'spacer', style: 'flex:1' }), back), this.treeList.el);
    this.treeList.setCount(this.searchResults ? this.searchResults.length : this.tree.length);
  }

  private treeRow(i: number): HTMLElement {
    const row: TreeRow = this.searchResults ? { die: this.searchResults[i], depth: 0, open: false } : this.tree[i];
    const d = row.die;
    const selected = this.selected && this.selected.unit === d.unit && this.selected.offset === d.offset;
    const el = h(
      'div',
      { class: `tree-row${row.open ? ' open' : ''}${selected ? ' selected' : ''}`, style: `padding-left:${6 + row.depth * 14}px` },
      h('span', { class: 'twisty' }, d.hasChildren && !this.searchResults ? icon('chevron') : null),
      h('span', { class: 'label' }, h('span', { class: 'die-tag' }, d.tag.replace('DW_TAG_', '')), d.name ? h('span', { class: 'die-name' }, d.name) : null, d.detail ? h('span', { class: 'die-detail' }, d.detail) : null),
    );
    el.querySelector('.twisty')!.addEventListener('click', (e) => {
      e.stopPropagation();
      if (!this.searchResults) void this.toggle(i);
    });
    el.addEventListener('click', () => void this.selectDie(d.unit, d.offset));
    el.addEventListener('dblclick', () => !this.searchResults && void this.toggle(i));
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

  /** Expands the tree down to a DIE (following its parents) and selects it. */
  private async reveal(unit: number, offset: bigint) {
    if (this.unit !== unit || this.searchResults) {
      await this.selectUnit(unit, false);
      this.mode = 'dies';
      this.renderMiddle();
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

  private async search(q: string) {
    if (!q.trim()) {
      this.searchResults = null;
      this.renderMiddle();
      return;
    }
    this.mode = 'dies';
    this.searchResults = await store.api.dieSearch(q.trim(), 500);
    this.renderMiddle();
  }

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
    if (d.decl) {
      const decl = d.decl;
      const l = h('span', { class: 'link mono' }, `${basename(decl.path)}:${decl.line}`);
      l.addEventListener('click', () => store.openSource(decl.file, decl.line));
      facts.push(['Declared at', l]);
    }
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
    const bytesLink = this.bytesLink(d);
    facts.push(['Encoded at', h('span', null, h('span', { class: 'mono' }, `${d.section} + ${hex(d.byteStart)}..${hex(d.byteEnd)}`), ' ', bytesLink)]);
    facts.push(['Offset', h('span', { class: 'mono' }, `${hex(d.die.offset)} in unit ${d.die.unit}`)]);

    const attrs = h('table', { class: 'attrs' });
    for (const a of d.attributes) attrs.appendChild(this.attrRow(a));

    this.details.replaceChildren(
      h(
        'div',
        { class: 'scroll pad' },
        crumbs,
        h('div', { style: 'margin:8px 0 4px' }, h('span', { class: 'die-tag', style: 'font-size:12px' }, d.die.tag), ' ', h('strong', { class: 'mono', style: 'font-size:14px' }, d.die.name ?? '')),
        u?.name ? h('div', { class: 'muted', style: 'font-size:12px;margin-bottom:8px;overflow-wrap:anywhere' }, `in ${u.name}`) : null,
        h('dl', { class: 'facts', style: 'margin-bottom:12px' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
        h('h3', { style: 'font-size:11px;text-transform:uppercase;letter-spacing:.5px;color:var(--muted);margin:0 0 4px' }, `Attributes (${d.attributes.length})`),
        attrs,
      ),
    );
  }

  private bytesLink(d: DieDetails): HTMLElement | null {
    const f = store.file!;
    if (f.dwarf?.source !== 'embedded') return null;
    const name = d.section.replace(/^\./, '');
    const sec = f.sections.find((s) => (s.name === d.section || s.name === `__${name}`) && s.fileOffset !== undefined && !s.compressed);
    if (!sec || sec.fileOffset === undefined) return null;
    const off = sec.fileOffset + d.byteStart;
    const l = h('span', { class: 'link' }, 'show bytes');
    l.addEventListener('click', () => void store.select({ offset: off }, { view: 'hex' }));
    return l;
  }

  private attrRow(a: AttrInfo): HTMLElement {
    const value = h('td', { class: 'av' });
    if (a.link) {
      const link = h('span', { class: 'link' }, a.value);
      const target: Link = a.link;
      link.addEventListener('click', () => this.follow(target));
      value.appendChild(link);
    } else value.textContent = a.value;
    return h('tr', null, h('td', { class: 'an' }, a.name.replace('DW_AT_', '')), h('td', { class: 'af' }, a.form.replace('DW_FORM_', '')), value);
  }

  private follow(link: Link) {
    if (link.type === 'die') void this.reveal(link.unit, link.offset);
    else if (link.type === 'address') {
      const code = store.file!.sections.some((s) => s.kind === 'code' && s.loaded && link.address >= s.address && link.address < s.address + s.size);
      void store.select({ address: link.address }, { view: code ? 'code' : 'hex' });
    } else store.openSource(link.file, link.line);
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
