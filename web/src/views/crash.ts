// Crash: a crash report symbolicated with what is open — every frame's
// function, source line and inlined calls — and which images had no binary.
import { store } from '../store';
import type { SymbolLine, Symbolicated, SymbolicatedFrame, SymbolicatedThread } from '../types';
import { basename, h, hex } from '../util';
import { View } from './base';

export class CrashView extends View {
  constructor() {
    super('crash');
    store.on('crash', () => this.invalidate());
    store.on('package', () => this.invalidate());
  }

  get visible(): boolean {
    return store.view === 'crash' && store.crash !== null;
  }

  protected render() {
    const c = store.crash;
    if (!c) {
      this.el.replaceChildren();
      return;
    }
    const page = h('div', { class: 'page' });
    const s = c.result;
    if (!s) {
      page.append(h('div', { class: 'card' }, h('h2', null, c.name), h('p', { class: 'sub' }, 'Symbolicating…')));
      this.el.replaceChildren(page);
      return;
    }
    page.append(this.summary(c.name, s));
    if (s.images.length > 0) page.append(this.images(s));
    for (const t of s.threads.filter((t) => t.crashed)) page.append(this.thread(t, true));
    const others = s.threads.filter((t) => !t.crashed);
    if (others.length > 0) {
      page.append(
        h(
          'details',
          { class: 'card crash-others' },
          h('summary', null, h('span', { class: 'crash-others-title' }, `${others.length} other thread${others.length === 1 ? '' : 's'}`)),
          others.map((t) => this.thread(t, false)),
        ),
      );
    }
    this.el.replaceChildren(page);
  }

  private summary(name: string, s: Symbolicated): HTMLElement {
    const r = s.report;
    const facts: [string, string][] = [];
    if (r.exception) facts.push(['Exception', r.exception]);
    if (r.reason) facts.push(['Reason', r.reason]);
    if (r.identifier) facts.push(['Identifier', r.identifier]);
    if (r.version) facts.push(['Version', r.version]);
    if (r.os) facts.push(['OS', r.os]);
    if (r.arch) facts.push(['Architecture', r.arch]);
    const symbolicated = s.images.some((i) => i.binary !== undefined && i.binary !== null);
    const open = store.file || store.package;
    return h(
      'div',
      { class: 'card' },
      h('h2', { style: 'font-size:18px' }, r.process ? `${r.process} crashed` : 'Crash report'),
      h('p', { class: 'sub' }, `${name} · ${r.format}`),
      h('dl', { class: 'facts crash-facts' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
      symbolicated
        ? null
        : h(
            'p',
            { class: 'secondary crash-hint' },
            open
              ? 'Nothing open here is one of the report’s images (by UUID or build ID). Open the app’s binaries with their dSYMs: a folder or zip of them works, and this report stays open.'
              : 'Open the app (its binary, or a folder or zip with its dSYMs) and the frames are symbolicated here.',
          ),
    );
  }

  private images(s: Symbolicated): HTMLElement {
    const rows = s.images.map((st) => {
      const img = s.report.images[st.image];
      const status =
        st.binary !== undefined && st.binary !== null
          ? h('span', { class: 'status good' }, `symbolicated with ${binaryName(st.binary)}`)
          : st.note
            ? h('span', { class: 'status warn' }, st.note)
            : h('span', { class: 'status none' }, 'not here');
      return h(
        'tr',
        null,
        h('td', { title: img.path ?? img.name }, img.name),
        h('td', { class: 'right muted' }, String(st.frames)),
        h('td', { class: 'mono muted', title: img.id ?? '' }, img.id ?? '—'),
        h('td', null, status),
      );
    });
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Images'),
      h('p', { class: 'sub' }, 'The binaries the frames are in, found here by UUID or build ID'),
      h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Image'), h('th', { class: 'right' }, 'Frames'), h('th', null, 'UUID / build ID'), h('th', null, ''))), h('tbody', null, rows)),
    );
  }

  private thread(t: SymbolicatedThread, open: boolean): HTMLElement {
    const rows = t.frames.map((f) => frameRow(f));
    const table = h('table', { class: 'data crash-frames' }, h('thead', null, h('tr', null, h('th', { class: 'right' }, '#'), h('th', null, 'Image'), h('th', null, 'Function'), h('th', null, 'Source'), h('th', null, 'Address'))), h('tbody', null, rows));
    return h(
      'div',
      { class: open ? 'card' : 'crash-thread' },
      h('h2', null, t.name, t.crashed ? h('span', { class: 'chip crash-chip' }, 'crashed') : null),
      h('div', { class: 'table-scroll' }, table),
    );
  }
}

function binaryName(i: number): string {
  if (store.package) return store.package.info.binaries[i]?.name ?? `#${i}`;
  return store.file ? basename(store.file.name) : `#${i}`;
}

function frameRow(f: SymbolicatedFrame): HTMLElement {
  const [inner, ...outer] = f.lines;
  const fn = (l: SymbolLine) => `${l.function ?? '?'}${l.offset !== undefined && l.offset !== null ? ` + ${l.offset}` : ''}`;
  const src = (l: SymbolLine) => (l.file ? `${basename(l.file.replace(/\\/g, '/'))}${l.line ? `:${l.line}` : ''}` : '');
  const known = f.binary !== undefined && f.binary !== null && f.binaryAddress !== undefined && f.binaryAddress !== null;
  const fnCell = inner
    ? h(
        'td',
        { class: 'crash-fn' },
        h('div', { class: 'mono' }, fn(inner)),
        outer.map((l) => h('div', { class: 'crash-inline' }, 'inlined into ', h('span', { class: 'mono' }, fn(l)), l.file ? h('span', { class: 'muted', title: l.file }, `  ${src(l)}`) : null)),
      )
    : h('td', { class: 'crash-fn muted' }, h('div', { class: 'mono' }, f.reported ?? '?'));
  const tr = h(
    'tr',
    { class: known ? 'clickable' : '', title: known ? 'Show this code' : '' },
    h('td', { class: 'right muted' }, String(f.index)),
    h('td', { class: known ? '' : 'muted' }, f.imageName),
    fnCell,
    h('td', { class: 'muted', title: inner?.file ?? '' }, inner ? src(inner) : ''),
    h('td', { class: 'mono muted' }, f.address !== undefined && f.address !== null ? hex(f.address) : f.binaryAddress !== undefined && f.binaryAddress !== null ? hex(f.binaryAddress) : ''),
  );
  if (known) tr.addEventListener('click', () => void store.goToFrame(f.binary!, f.binaryAddress!));
  return tr;
}
