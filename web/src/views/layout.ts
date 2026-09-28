// Layout view: the file as a tree of regions, down to fields and table entries.
import { familyOf } from '../colors';
import { store } from '../store';
import type { PathEntry, RegionInfo } from '../types';
import { legend } from '../ui';
import { formatSize, h, hex, icon, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';

const ENTRY_PAGE = 100;

interface Row {
  key: string;
  depth: number;
  type: 'region' | 'entries' | 'entry' | 'more';
  region?: RegionInfo;
  entry?: PathEntry;
  /** Region whose entries these are. */
  owner?: RegionInfo;
  next?: number;
  open: boolean;
}

export class LayoutView extends View {
  private rows: Row[] = [];
  private list!: VList;
  private selectedKey?: string;

  constructor() {
    super('layout', true);
    store.on('intent', () => {
      // Before the roots load, render() picks the intent up instead.
      if (this.visible && this.rows.length > 0 && store.intent.region !== undefined) void this.reveal(store.intent.region);
    });
  }

  protected render() {
    const f = store.file!;
    this.rows = [];
    this.list = new VList({ rowHeight: 24, renderRow: (i) => this.row(i) });
    const head = h(
      'div',
      { class: 'tree-row', style: 'font-weight:600;color:var(--text-2);cursor:default;border-bottom:1px solid var(--border);background:var(--surface)' },
      h('span', { class: 'twisty' }),
      h('span', { class: 'range', style: 'color:var(--text-2);font-family:var(--sans)' }, 'File range'),
      h('span', { class: 'size', style: 'font-family:var(--sans)' }, 'Size'),
      h('span', { class: 'label' }, 'Region = value'),
    );
    this.el.replaceChildren(
      h('div', { class: 'toolbar' }, h('h3', null, 'Layout'), h('span', { class: 'secondary' }, 'Every byte of the file, nested by containment. Expand headers and tables down to single fields.'), h('span', { class: 'spacer' }), legend(new Set(f.composition.map(([k]) => familyOf(k))))),
      head,
      this.list.el,
    );
    void store.api.regions().then((roots) => {
      if (store.file !== f) return;
      this.rows = roots.map((r) => ({ key: `r${r.id}`, depth: 0, type: 'region', region: r, open: false }));
      this.list.setCount(this.rows.length);
      if (store.intent.region !== undefined) void this.reveal(store.intent.region);
    });
  }

  private row(i: number): HTMLElement {
    const r = this.rows[i];
    const pad = `padding-left:${8 + r.depth * 16}px`;
    if (r.type === 'more') {
      const el = h('div', { class: 'tree-row', style: pad }, h('span', { class: 'twisty' }), h('span', { class: 'more' }, 'Load more entries…'));
      el.addEventListener('click', () => void this.loadEntries(i, r.owner!, r.next!, r.depth));
      return el;
    }
    if (r.type === 'entries') {
      const count = r.owner!.entryCount;
      const el = h(
        'div',
        { class: `tree-row${r.open ? ' open' : ''}`, style: pad },
        h('span', { class: 'twisty' }, icon('chevron')),
        h('span', { class: 'label' }, h('span', { class: 'secondary' }, count !== undefined ? `${count.toLocaleString()} entries` : 'Entries (decoded on demand)')),
      );
      el.addEventListener('click', () => void this.toggle(i));
      return el;
    }
    const info = (r.region ?? r.entry)!;
    const size = info.end - info.start;
    const canOpen = r.type === 'region' && this.hasChildren(r.region!);
    const el = h(
      'div',
      { class: `tree-row${r.open ? ' open' : ''}${this.selectedKey === r.key ? ' selected' : ''}`, style: pad, title: info.note ?? '' },
      h('span', { class: 'twisty' }, canOpen ? icon('chevron') : null),
      h('span', { class: 'range' }, `${hex(info.start)}..${hex(info.end)}`),
      h('span', { class: 'size' }, num(size) < 1024 ? String(num(size)) : formatSize(size)),
      h('span', { class: 'label' }, h('span', { class: `swatch fam-${familyOf(info.kind)}` }), h('span', { class: 'name' }, info.name), info.value ? h('span', { class: 'value' }, `= ${info.value}`) : null),
    );
    const twisty = el.querySelector('.twisty')!;
    twisty.addEventListener('click', (e) => {
      e.stopPropagation();
      if (canOpen) void this.toggle(i);
    });
    el.addEventListener('click', () => {
      this.selectedKey = r.key;
      void store.select({ offset: info.start }, { origin: 'layout' });
      this.list.refresh();
    });
    el.addEventListener('dblclick', () => canOpen && void this.toggle(i));
    return el;
  }

  private hasChildren(r: RegionInfo): boolean {
    return r.childCount > 0 || r.decoded;
  }

  private async toggle(i: number) {
    const r = this.rows[i];
    if (r.open) {
      let end = i + 1;
      while (end < this.rows.length && this.rows[end].depth > r.depth) end++;
      this.rows.splice(i + 1, end - i - 1);
      r.open = false;
      this.list.setCount(this.rows.length);
      return;
    }
    r.open = true;
    if (r.type === 'entries') {
      await this.loadEntries(i, r.owner!, 0, r.depth + 1);
      return;
    }
    const region = r.region!;
    const insert: Row[] = [];
    if (region.childCount > 0) {
      const kids = await store.api.regions(region.id);
      insert.push(...kids.map((k): Row => ({ key: `r${k.id}`, depth: r.depth + 1, type: 'region', region: k, open: false })));
    }
    if (region.decoded) {
      if (region.childCount === 0) {
        this.rows.splice(i + 1, 0, ...insert);
        this.list.setCount(this.rows.length);
        await this.loadEntries(i + insert.length, region, 0, r.depth + 1);
        return;
      }
      insert.push({ key: `e${region.id}`, depth: r.depth + 1, type: 'entries', owner: region, open: false });
    }
    this.rows.splice(i + 1, 0, ...insert);
    this.list.setCount(this.rows.length);
  }

  /** Inserts entries of `owner` after row `after`. */
  private async loadEntries(after: number, owner: RegionInfo, first: number, depth: number) {
    const entries = await store.api.regionEntries(owner.id, first, ENTRY_PAGE);
    let at = after + 1;
    if (this.rows[after]?.type === 'more') {
      this.rows.splice(after, 1);
      at = after;
    }
    const rows: Row[] = entries.map((e, k) => ({ key: `r${owner.id}.${first + k}`, depth, type: 'entry', entry: e, open: false }));
    const total = owner.entryCount;
    if (entries.length === ENTRY_PAGE && (total === undefined || first + ENTRY_PAGE < total)) rows.push({ key: `m${owner.id}.${first}`, depth, type: 'more', owner, next: first + ENTRY_PAGE, open: false });
    this.rows.splice(at, 0, ...rows);
    this.list.setCount(this.rows.length);
  }

  /** Expands the tree along the path to `offset` and selects the innermost region. */
  private async reveal(offset: bigint) {
    store.intent = {};
    const ins = await store.api.inspectOffset(offset);
    const ids = ins.path.map((p) => p.id).filter((id): id is number => id !== undefined);
    let index = -1;
    for (const id of ids) {
      index = this.rows.findIndex((r) => r.type === 'region' && r.region!.id === id);
      if (index < 0) break;
      const r = this.rows[index];
      if (!r.open && r.region!.childCount > 0 && id !== ids[ids.length - 1]) await this.toggle(index);
    }
    if (index >= 0) {
      this.selectedKey = this.rows[index].key;
      this.list.scrollToIndex(index, 'center');
      this.list.refresh();
    }
  }

  protected onSelection() {
    if (!this.list || store.selection.origin === 'layout') return;
    const off = store.selection.offset;
    if (off === undefined) return;
    // Highlight the deepest loaded row containing the offset.
    let best = -1;
    let bestDepth = -1;
    this.rows.forEach((r, i) => {
      const info = r.region ?? r.entry;
      if (info && off >= info.start && off < info.end && r.depth >= bestDepth) {
        best = i;
        bestDepth = r.depth;
      }
    });
    if (best >= 0) {
      this.selectedKey = this.rows[best].key;
      this.list.scrollToIndex(best, 'nearest');
    }
    this.list.refresh();
  }
}
