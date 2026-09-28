// The search box in the top bar: one query across addresses, file offsets,
// symbols, imports/exports, sections, source lines, DWARF names, notes,
// strings and byte patterns, with results grouped by kind.
import { store, type ViewName } from './store';
import type { HitKind, SearchHit, SearchResults } from './types';
import { debounce, formatCount, h, hex } from './util';

const GROUPS: Record<HitKind, string> = {
  address: 'Address',
  offset: 'File offset',
  symbol: 'Symbols',
  import: 'Imports',
  export: 'Exports',
  section: 'Sections & segments',
  source: 'Source',
  dwarf: 'Debug info',
  note: 'Your notes',
  string: 'Strings',
  bytes: 'Byte matches',
};

const PER_KIND = 6;
const ALL = 300;

export class SearchPalette {
  readonly el: HTMLFormElement;
  readonly input: HTMLInputElement;
  private readonly panel: HTMLElement;
  private results: SearchResults | null = null;
  private rows: { hit: SearchHit; el: HTMLElement }[] = [];
  private active = -1;
  private seq = 0;
  /** "Show all" for one kind. */
  private expanded: HitKind | null = null;
  private prepared: string | null = null;

  constructor() {
    this.input = h('input', {
      type: 'search',
      placeholder: 'Search addresses, symbols, strings, bytes…',
      'aria-label': 'Search the binary',
      role: 'combobox',
      'aria-expanded': 'false',
      'aria-controls': 'search-panel',
      'aria-autocomplete': 'list',
      spellcheck: 'false',
      autocomplete: 'off',
    });
    this.panel = h('div', { class: 'search-panel', id: 'search-panel', role: 'listbox' });
    this.panel.hidden = true;
    this.el = h('form', { class: 'goto search' }, this.input, h('kbd', { class: 'search-kbd' }, 'Ctrl K'), this.panel);
    this.el.addEventListener('submit', (e) => {
      e.preventDefault();
      this.activate(this.rows[Math.max(0, this.active)]?.hit);
    });
    const run = debounce(() => void this.run(), 90);
    this.input.addEventListener('input', () => {
      this.expanded = null;
      run();
    });
    this.input.addEventListener('focus', () => {
      this.prepare();
      if (this.input.value.trim()) {
        if (this.results) this.open();
        else void this.run();
      }
    });
    this.input.addEventListener('keydown', (e) => this.onKey(e));
    document.addEventListener('pointerdown', (e) => {
      if (!this.el.contains(e.target as Node)) this.close();
    });
    store.on('file', () => {
      this.results = null;
      this.close();
    });
    store.on('annotations', () => {
      if (!this.panel.hidden) void this.run();
    });
  }

  focus(text?: string) {
    if (text !== undefined) {
      this.input.value = text;
      this.expanded = null;
      void this.run();
    }
    this.input.focus();
    this.input.select();
  }

  /** Builds the string and DWARF name indexes in the background on first use. */
  private prepare() {
    const f = store.file;
    if (!f || this.prepared === f.sha256) return;
    this.prepared = f.sha256;
    void store.api.prepareSearch().catch(() => {});
  }

  private open() {
    this.panel.hidden = false;
    this.input.setAttribute('aria-expanded', 'true');
  }

  close() {
    this.panel.hidden = true;
    this.input.setAttribute('aria-expanded', 'false');
  }

  private async run() {
    const q = this.input.value.trim();
    const mine = ++this.seq;
    if (!q || !store.file) {
      this.results = null;
      this.close();
      return;
    }
    const slow = setTimeout(() => {
      if (mine === this.seq) {
        this.panel.replaceChildren(h('div', { class: 'search-empty' }, 'Searching…'));
        this.open();
      }
    }, 160);
    try {
      const res = await store.api.search(q, this.expanded ? ALL : PER_KIND, this.expanded ?? undefined);
      if (mine !== this.seq) return;
      this.results = res;
      this.render();
    } catch (e) {
      if (mine === this.seq) this.panel.replaceChildren(h('div', { class: 'search-empty' }, e instanceof Error ? e.message : String(e)));
    } finally {
      clearTimeout(slow);
    }
  }

  private render() {
    const res = this.results!;
    this.rows = [];
    this.active = -1;
    const body = h('div', { class: 'search-body' });
    const count = (k: HitKind) => res.counts.find((c) => c.kind === k)?.count ?? 0;
    if (this.expanded) {
      const back = h('button', { class: 'btn ghost small', type: 'button' }, '← All results');
      back.addEventListener('click', () => {
        this.expanded = null;
        void this.run();
        this.input.focus();
      });
      body.appendChild(h('div', { class: 'search-group-head' }, back, h('span', { class: 'spacer' }), `${GROUPS[this.expanded]} · ${formatCount(count(this.expanded))}`));
    }
    let last: HitKind | null = null;
    for (const hit of res.hits) {
      if (hit.kind !== last && !this.expanded) {
        last = hit.kind;
        const total = count(hit.kind);
        const shown = res.hits.filter((x) => x.kind === hit.kind).length;
        const head = h('div', { class: 'search-group-head' }, h('span', null, GROUPS[hit.kind]), h('span', { class: 'muted' }, formatCount(total) + (hit.kind === 'bytes' && total >= 10000 ? '+' : '')));
        if (total > shown) {
          const kind = hit.kind;
          const more = h('button', { class: 'link-btn', type: 'button' }, `Show all ${formatCount(Math.min(total, ALL))}`);
          more.addEventListener('click', () => {
            this.expanded = kind;
            void this.run();
            this.input.focus();
          });
          head.append(h('span', { class: 'spacer' }), more);
        }
        body.appendChild(head);
      }
      const idx = this.rows.length;
      const row = h(
        'div',
        { class: 'hit', role: 'option', id: `hit-${idx}` },
        h('div', { class: 'hit-main' }, h('span', { class: 'hit-label' }, highlight(hit.label, needleOf(res.query))), h('span', { class: 'hit-detail' }, hit.detail)),
        h('span', { class: 'hit-at' }, hit.address !== undefined ? hex(hit.address) : hit.offset !== undefined ? '@' + hex(hit.offset) : ''),
      );
      row.addEventListener('pointermove', () => this.setActive(idx, false));
      row.addEventListener('click', () => this.activate(hit));
      this.rows.push({ hit, el: row });
      body.appendChild(row);
    }
    if (res.hits.length === 0) {
      body.appendChild(h('div', { class: 'search-empty' }, `Nothing matches “${res.query}”.`));
    }
    const hints = h(
      'div',
      { class: 'search-hints' },
      hint('0x401000', 'address'),
      hint('@0x200', 'file offset'),
      hint('main+0x10', ''),
      hint('48 8b ?? 05', 'bytes'),
      hint('"text"', 'exact text'),
      hint('file.c:42', 'line'),
      h('span', { class: 'spacer' }),
      h('span', { class: 'muted' }, '↑↓ ↵ Esc'),
    );
    this.panel.replaceChildren(body, hints);
    this.open();
    if (this.rows.length > 0) this.setActive(0, false);
  }

  private setActive(i: number, scroll: boolean) {
    if (i === this.active) return;
    this.rows[this.active]?.el.classList.remove('active');
    this.active = i;
    const r = this.rows[i];
    if (!r) return;
    r.el.classList.add('active');
    this.input.setAttribute('aria-activedescendant', r.el.id);
    if (scroll) r.el.scrollIntoView({ block: 'nearest' });
  }

  private onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (this.panel.hidden) {
        if (this.results) this.open();
        return;
      }
      e.preventDefault();
      const n = this.rows.length;
      if (n === 0) return;
      this.setActive((this.active + (e.key === 'ArrowDown' ? 1 : n - 1)) % n, true);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (this.expanded) {
        this.expanded = null;
        void this.run();
      } else if (!this.panel.hidden) this.close();
      else this.input.blur();
    }
  }

  /** Jumps to a hit: code for functions, hex for data, the DWARF or source view for those. */
  private activate(hit: SearchHit | undefined) {
    const f = store.file;
    if (!hit || !f) return;
    this.close();
    this.input.blur();
    const inCode = (a: bigint) => f.sections.some((s) => s.kind === 'code' && s.loaded && a >= s.address && a < s.address + s.size);
    const view = (code: boolean): ViewName => {
      const v = store.view;
      if (v === 'hex' || v === 'layout') return v;
      return code ? 'code' : 'hex';
    };
    switch (hit.kind) {
      case 'dwarf':
        if (hit.unit !== undefined && hit.die !== undefined) store.openDie(hit.unit, hit.die);
        return;
      case 'source':
        if (hit.address === undefined) {
          if (hit.file !== undefined) store.openSource(hit.file, hit.line ?? 0);
          return;
        }
        break;
      case 'section':
        if (hit.offset !== undefined) {
          store.revealRegion(hit.offset);
          void store.select({ offset: hit.offset }, { origin: 'layout' });
        } else if (hit.address !== undefined) void store.select({ address: hit.address }, { view: 'hex' });
        return;
      case 'offset':
      case 'bytes':
      case 'string':
        if (hit.kind === 'string' && hit.address !== undefined) void store.select({ address: hit.address }, { view: view(false) });
        else if (hit.offset !== undefined) void store.select({ offset: hit.offset }, { view: view(false) });
        return;
      case 'import':
        if (hit.address === undefined) {
          store.setView('symbols');
          return;
        }
        break;
    }
    if (hit.address !== undefined) void store.select({ address: hit.address }, { view: view(inCode(hit.address)) });
    else if (hit.offset !== undefined) void store.select({ offset: hit.offset }, { view: 'hex' });
  }
}

function hint(example: string, label: string): HTMLElement {
  return h('span', { class: 'hint' }, h('code', null, example), label ? ` ${label}` : '');
}

/** The part of a query that text matches highlight. */
function needleOf(query: string): string {
  const q = query.trim();
  if (/^".*"?$/.test(q)) return q.replace(/^"|"$/g, '');
  return q.includes(':') && /:\d+$/.test(q) ? q.replace(/:\d+(:\d+)?$/, '') : q;
}

function highlight(label: string, needle: string): (Node | string)[] {
  if (!needle) return [label];
  const i = label.toLowerCase().indexOf(needle.toLowerCase());
  if (i < 0) return [label];
  return [label.slice(0, i), h('mark', null, label.slice(i, i + needle.length)), label.slice(i + needle.length)];
}
