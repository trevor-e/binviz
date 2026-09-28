// Text in old games: found by relative search, then read, searched and
// dumped with a table file (which also becomes the hex view's text column).
import { store } from '../store';
import type { RelativeSearch, TableText, TextEncoding } from '../types';
import { basename, formatCount, h, hex } from '../util';
import { VList } from '../vlist';
import { View } from './base';

/** What the right-hand list shows. */
type Listing = { kind: 'strings' | 'found'; items: TableText[]; what: string };

export class TextView extends View {
  private found: RelativeSearch | null = null;
  private word = '';
  private width = 1;
  private results!: HTMLElement;
  private right!: HTMLElement;
  private listing: Listing | null = null;
  private draft: string | null = null;

  constructor() {
    super('text', true);
    store.on('table', () => {
      if (this.visible) this.renderTable();
    });
  }

  protected render() {
    const word = h('input', { class: 'field small', type: 'search', placeholder: 'A word the game shows: SWORD', value: this.word, style: 'flex:1;min-width:0', spellcheck: 'false', 'aria-label': 'Word' });
    const width = h(
      'select',
      { class: 'field small', 'aria-label': 'Bytes per character' },
      h('option', { value: '1', selected: this.width === 1 }, '1 byte'),
      h('option', { value: '2', selected: this.width === 2 }, '2 bytes'),
    );
    const go = h('button', { class: 'btn small primary', type: 'submit' }, 'Search');
    const form = h('form', { class: 'toolbar' }, word, width, go);
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      this.word = word.value.trim();
      this.width = Number(width.value);
      void this.search();
    });
    this.results = h('div', { class: 'text-results' });
    const left = h(
      'div',
      { class: 'pane side wider scroll' },
      h('div', { class: 'pad' }, h('h3', null, 'Relative search'), h('p', { class: 'muted' }, 'Old games rarely store text as ASCII, but most keep the alphabet in order, starting anywhere. Type a word the game shows: it is found by the spacing of its letters, whatever the encoding.')),
      form,
      this.results,
    );
    this.right = h('div', { class: 'pane grow' });
    this.el.replaceChildren(h('div', { class: 'split' }, left, this.right));
    this.showFound();
    this.renderTable();
  }

  private async search() {
    if (!this.word) return;
    this.results.replaceChildren(h('div', { class: 'pad muted' }, 'Searching…'));
    try {
      this.found = await store.api.relativeSearch(this.word, this.width, 50);
    } catch (e) {
      this.found = null;
      this.results.replaceChildren(h('div', { class: 'pad' }, e instanceof Error ? e.message : String(e)));
      return;
    }
    this.showFound();
  }

  private showFound() {
    const f = this.found;
    if (!f) {
      this.results.replaceChildren();
      return;
    }
    if (f.encodings.length === 0) {
      this.results.replaceChildren(h('div', { class: 'pad muted' }, `“${f.word}” is nowhere in this file, in any encoding that keeps the alphabet in order. Try another word, or 2-byte characters.`));
      return;
    }
    const digits = f.width * 2;
    this.results.replaceChildren(
      h('div', { class: 'pad muted' }, `${formatCount(f.total)} ${f.total === 1 ? 'hit' : 'hits'} in ${f.encodings.length} ${f.encodings.length === 1 ? 'encoding' : 'encodings'}`),
      ...f.encodings.map((e) => this.encoding(e, digits)),
    );
  }

  private encoding(e: TextEncoding, digits: number): HTMLElement {
    const use = h('button', { class: 'btn tiny', type: 'button', title: `A table with ${e.letter === '0' ? 'the digits' : 'the alphabet'} from $${e.first.toString(16).toUpperCase().padStart(digits, '0')}` }, 'Use as table');
    use.addEventListener('click', () => void this.useEncoding(e));
    const hits = e.hits.map((hit) => {
      const before = [...hit.preview].slice(0, hit.at).join('');
      const word = [...hit.preview].slice(hit.at, hit.at + [...(this.found?.word ?? '')].length).join('');
      const after = [...hit.preview].slice(hit.at + [...word].length).join('');
      const row = h('div', { class: 'ref-row', title: 'Show in the hex view' }, h('span', { class: 'addr link' }, hex(hit.offset)), h('span', { class: 'mono text-preview' }, before, h('mark', null, word), after));
      row.addEventListener('click', () => void store.select({ offset: hit.offset }, { view: 'hex' }));
      return row;
    });
    return h(
      'div',
      { class: 'text-encoding' },
      h('div', { class: 'text-encoding-head' }, h('b', { class: 'mono' }, `${e.letter} = $${e.first.toString(16).toUpperCase().padStart(digits, '0')}`), h('span', { class: 'muted' }, `${e.hits.length} ${e.hits.length === 1 ? 'hit' : 'hits'}`), h('span', { class: 'spacer' }), use),
      ...hits,
    );
  }

  /** Adds the alphabet an encoding implies to the table (replacing those entries). */
  private async useEncoding(e: TextEncoding) {
    const lines = await store.api.tableFromAlphabet(e.first, e.letter, this.found?.width ?? 1);
    const current = store.table?.text ?? '';
    const text = current ? `${current.replace(/\n*$/, '\n')}${lines}` : `${lines}`;
    await this.apply(text);
  }

  private async apply(text: string) {
    try {
      if (text.trim()) await store.setTable(text);
      else await store.clearTable();
      this.draft = null;
      this.listing = null;
    } catch (e) {
      store.error(e);
    }
  }

  private renderTable() {
    const t = store.table;
    const area = h('textarea', { class: 'field mono table-text', rows: 12, spellcheck: 'false', 'aria-label': 'Table file', placeholder: '80=A\n81=B\n…\n00= \n/FF=<end>\n*FE' });
    area.value = this.draft ?? t?.text ?? '';
    area.addEventListener('input', () => (this.draft = area.value));
    const apply = h('button', { class: 'btn small primary', type: 'button' }, 'Apply');
    apply.addEventListener('click', () => void this.apply(area.value));
    const input = h('input', { type: 'file', accept: '.tbl,.txt,text/plain', style: 'display:none' });
    input.addEventListener('change', async () => {
      const file = input.files?.[0];
      input.value = '';
      if (file) await this.apply(await file.text());
    });
    const load = h('button', { class: 'btn small', type: 'button' }, 'Load .tbl…');
    load.addEventListener('click', () => input.click());
    const save = h('button', { class: 'btn small', type: 'button', disabled: !t }, 'Save .tbl');
    save.addEventListener('click', () => {
      if (!store.table) return;
      const a = h('a', { href: URL.createObjectURL(new Blob([store.table.text], { type: 'text/plain' })), download: `${basename(store.file?.name ?? 'game')}.tbl` });
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    });
    const clear = h('button', { class: 'btn small', type: 'button', disabled: !t }, 'Clear');
    clear.addEventListener('click', () => void this.apply(''));
    const head = h(
      'div',
      { class: 'pane-head' },
      h('b', null, 'Table file'),
      h('span', { class: 'muted' }, t ? `${formatCount(t.entries)} entries · the hex view reads text with it` : 'none: text is read as ASCII'),
      h('span', { class: 'spacer' }),
      load,
      save,
      clear,
      input,
    );
    const help = h('div', { class: 'muted table-help' }, 'One entry per line: 80=A, 8A20=the (several bytes), /FF=<end> (ends a string), *FE (a line break). Relative search results give the alphabet.');
    const body = h('div', { class: 'table-edit' }, area, h('div', { class: 'btn-row' }, apply), help);
    const tools = t ? this.textTools() : h('div', { class: 'pad muted' }, 'With a table: find text as the game encodes it, or list all the text it reads.');
    this.right.replaceChildren(head, body, tools);
  }

  private textTools(): HTMLElement {
    const find = h('input', { class: 'field small', type: 'search', placeholder: 'Text to find (as the game spells it)', style: 'width:260px', spellcheck: 'false', 'aria-label': 'Text to find' });
    const findBtn = h('button', { class: 'btn small', type: 'submit' }, 'Find');
    const form = h('form', { class: 'toolbar' }, find, findBtn);
    const all = h('button', { class: 'btn small', type: 'button' }, 'All the text');
    const min = h('input', { class: 'field small', type: 'number', min: '2', max: '100', value: '4', style: 'width:6ch', 'aria-label': 'Entries per string at least' });
    form.append(h('span', { class: 'spacer' }), all, h('span', { class: 'muted' }, 'of'), min, h('span', { class: 'muted' }, 'entries or more'));
    const list = h('div', { class: 'text-list' });
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const text = find.value;
      if (!text) return;
      try {
        this.listing = { kind: 'found', items: await store.api.tableFind(text, 2000), what: text };
      } catch (err) {
        this.listing = null;
        list.replaceChildren(h('div', { class: 'pad' }, err instanceof Error ? err.message : String(err)));
        return;
      }
      this.showListing(list);
    });
    all.addEventListener('click', async () => {
      this.listing = { kind: 'strings', items: await store.api.tableStrings(Math.max(1, Number(min.value) || 4), 20000), what: '' };
      this.showListing(list);
    });
    this.showListing(list);
    return h('div', { class: 'text-tools' }, form, list);
  }

  private showListing(el: HTMLElement) {
    const l = this.listing;
    if (!l) {
      el.replaceChildren();
      return;
    }
    const summary =
      l.kind === 'found'
        ? `${formatCount(l.items.length)} ${l.items.length === 1 ? 'place' : 'places'} for “${l.what}”`
        : `${formatCount(l.items.length)} strings${l.items.length >= 20000 ? ' (the first 20,000)' : ''}`;
    const vlist = new VList({
      rowHeight: 24,
      renderRow: (i) => {
        const t = l.items[i];
        const row = h('div', { class: 'list-row', title: 'Show in the hex view' }, h('span', { class: 'addr', style: 'width:12ch;flex:none' }, hex(t.offset)), h('span', { class: 'mono nm' }, t.text.replace(/\n/g, '⏎')));
        row.addEventListener('click', () => void store.select({ offset: t.offset }, { view: 'hex' }));
        return row;
      },
    });
    el.replaceChildren(h('div', { class: 'pad muted' }, summary), vlist.el);
    vlist.setCount(l.items.length);
  }
}
