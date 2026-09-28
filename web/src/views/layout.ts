// Layout: the file's parts. Its regions as a tree, down to single fields
// and table entries; its segments and sections, as the loader sees them;
// and how much of its code and data has been mapped out.
import { familyOf } from '../colors';
import { store } from '../store';
import type { PathEntry, RegionInfo } from '../types';
import { legend } from '../ui';
import { formatSize, h, hex, icon, num } from '../util';
import { VList } from '../vlist';
import { type Panel, View } from './base';
import { CoveragePanel } from './coverage';
import { SectionsPanel } from './sections';

type Tab = 'regions' | 'sections' | 'coverage';

const TABS: { tab: Tab; label: string; blurb: string }[] = [
  { tab: 'regions', label: 'Regions', blurb: 'Every byte of the file, nested by containment. Expand headers and tables down to single fields.' },
  { tab: 'sections', label: 'Sections', blurb: 'Segments and sections: where each part of the file goes in memory.' },
  { tab: 'coverage', label: 'Coverage', blurb: 'How much of the code and data is mapped out, and the largest stretches nothing explains yet.' },
];

const isTab = (s: string | undefined): s is Tab => TABS.some((t) => t.tab === s);

export class LayoutView extends View {
  private tab: Tab = 'regions';
  private readonly regions = new RegionTree();
  private readonly panels: Record<Tab, Panel> = { regions: this.regions, sections: new SectionsPanel(), coverage: new CoveragePanel() };
  private tabs = new Map<Tab, HTMLButtonElement>();
  private blurb = h('span', { class: 'secondary' });
  private body = h('div', { class: 'layout-body' });

  constructor() {
    super('layout', true);
    store.on('intent', () => {
      if (this.visible && store.intent.region !== undefined) {
        this.showTab('regions');
        void this.regions.reveal(store.intent.region);
      }
    });
  }

  protected render() {
    for (const p of Object.values(this.panels)) p.reset();
    const bar = h('div', { class: 'tabs', role: 'tablist' });
    this.tabs.clear();
    for (const t of TABS) {
      const b = h('button', { class: 'tab', type: 'button', role: 'tab', title: t.blurb }, t.label);
      b.addEventListener('click', () => {
        this.showTab(t.tab);
        store.setViewState('layout', t.tab);
      });
      this.tabs.set(t.tab, b);
      bar.appendChild(b);
    }
    this.el.replaceChildren(h('div', { class: 'toolbar' }, h('h3', null, 'Layout'), bar, this.blurb), this.body);
    const want = store.viewState.layout;
    this.showTab(isTab(want) ? want : this.tab, true);
  }

  protected onState() {
    const want = store.viewState.layout;
    if (isTab(want) && want !== this.tab) this.showTab(want);
  }

  private showTab(tab: Tab, force = false) {
    if (tab === this.tab && !force && this.body.firstChild === this.panels[tab].el) return;
    this.tab = tab;
    for (const [t, b] of this.tabs) {
      b.classList.toggle('active', t === tab);
      b.setAttribute('aria-selected', String(t === tab));
    }
    this.blurb.textContent = TABS.find((t) => t.tab === tab)!.blurb;
    const panel = this.panels[tab];
    this.body.replaceChildren(panel.el);
    panel.show();
    panel.onSelection?.();
  }

  protected onSelection() {
    this.panels[this.tab].onSelection?.();
  }
}

// --- Regions ------------------------------------------------------------------------

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

/** The file as a tree of regions, down to fields and table entries. */
class RegionTree implements Panel {
  readonly el = h('div', { class: 'layout-tree' });
  private rows: Row[] = [];
  private list!: VList;
  private selectedKey?: string;
  private stale = true;

  reset() {
    this.stale = true;
  }

  show() {
    if (!this.stale) return;
    this.stale = false;
    const f = store.file!;
    this.rows = [];
    this.list = new VList({ rowHeight: 24, renderRow: (i) => this.row(i) });
    const head = h(
      'div',
      { class: 'tree-row tree-head' },
      h('span', { class: 'twisty' }),
      h('span', { class: 'range' }, 'File range'),
      h('span', { class: 'size' }, 'Size'),
      h('span', { class: 'label' }, 'Region = value'),
    );
    this.el.replaceChildren(h('div', { class: 'layout-legend' }, legend(new Set(f.composition.map(([k]) => familyOf(k))))), head, this.list.el);
    void store.api.regions().then((roots) => {
      if (store.file !== f) return;
      this.rows = roots.map((r) => ({ key: `r${r.id}`, depth: 0, type: 'region', region: r, open: false }));
      this.list.setCount(this.rows.length);
      if (store.intent.region !== undefined) void this.reveal(store.intent.region);
      else this.onSelection();
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
  async reveal(offset: bigint) {
    // Still loading: the roots pick the intent up.
    if (this.rows.length === 0) return;
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

  onSelection() {
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
