// Sections (a tab of the Layout view): the segments and sections tables, the
// file as the loader and the linker see it.
import { store } from '../store';
import { swatch } from '../ui';
import { fmtAddr, formatSize, h, hex } from '../util';
import type { Panel } from './base';

export class SectionsPanel implements Panel {
  readonly el = h('div', { class: 'scroll' });
  private rows: { el: HTMLElement; start?: bigint; end?: bigint; fileStart?: bigint; fileEnd?: bigint }[] = [];
  private stale = true;

  reset() {
    this.stale = true;
  }

  show() {
    if (!this.stale) return;
    this.stale = false;
    this.render();
  }

  private render() {
    const f = store.file!;
    this.rows = [];
    const segTable = h('table', { class: 'data' }, h('thead', null, h('tr', null, ...['#', 'Name', 'Type', 'Perms', 'Virtual address', 'Memory size', 'File range', 'File size', 'Align'].map((c, i) => h('th', { class: i >= 4 ? 'right' : '' }, c)))));
    const segBody = h('tbody');
    for (const s of f.segments) {
      const tr = h(
        'tr',
        { class: 'clickable', title: 'Double-click to open in the hex view' },
        h('td', { class: 'muted' }, String(s.index)),
        h('td', null, s.name),
        h('td', { class: 'secondary' }, s.kind, s.mapped ? '' : h('span', { class: 'muted' }, ' (overlay)')),
        h('td', { class: 'mono' }, s.perms),
        h('td', { class: 'right mono' }, fmtAddr(s.address)),
        h('td', { class: 'right' }, formatSize(s.memSize)),
        h('td', { class: 'right mono' }, s.fileSize > 0n ? `${hex(s.fileOffset)}..${hex(s.fileOffset + s.fileSize)}` : '—'),
        h('td', { class: 'right' }, formatSize(s.fileSize)),
        h('td', { class: 'right mono muted' }, s.align > 0n ? hex(s.align) : ''),
      );
      tr.addEventListener('click', () => {
        if (s.mapped && s.memSize > 0n) void store.select({ address: s.address }, { origin: 'layout' });
        else void store.select({ offset: s.fileOffset }, { origin: 'layout' });
      });
      tr.addEventListener('dblclick', () => void store.select(s.fileSize > 0n ? { offset: s.fileOffset } : { address: s.address }, { view: 'hex' }));
      segBody.appendChild(tr);
      this.rows.push({ el: tr, start: s.mapped ? s.address : undefined, end: s.mapped ? s.address + s.memSize : undefined });
    }
    segTable.appendChild(segBody);

    const secTable = h('table', { class: 'data' }, h('thead', null, h('tr', null, ...['#', 'Name', 'Kind', 'Perms', 'Address', 'Size', 'File range', 'Flags'].map((c, i) => h('th', { class: i >= 4 && i <= 6 ? 'right' : '' }, c)))));
    const secBody = h('tbody');
    for (const s of f.sections) {
      const fileRange = s.fileOffset !== undefined ? `${hex(s.fileOffset)}..${hex(s.fileOffset + s.fileSize)}${s.compressed ? ' (compressed)' : ''}` : '— (no file data)';
      const tr = h(
        'tr',
        { class: 'clickable', title: 'Double-click to open in the hex view' },
        h('td', { class: 'muted' }, String(s.index)),
        h('td', { class: 'mono' }, s.segmentName ? h('span', { class: 'muted' }, `${s.segmentName},`) : null, s.name),
        h('td', null, swatch(s.kind), s.kind),
        h('td', { class: 'mono' }, s.perms),
        h('td', { class: 'right mono' }, s.loaded ? fmtAddr(s.address) : '—'),
        h('td', { class: 'right' }, formatSize(s.size)),
        h('td', { class: 'right mono' }, fileRange),
        h('td', { class: 'secondary', style: 'white-space:normal;min-width:240px;font-size:12px' }, s.flags),
      );
      tr.addEventListener('click', () => {
        if (s.loaded && s.size > 0n) void store.select({ address: s.address }, { origin: 'layout' });
        else if (s.fileOffset !== undefined) void store.select({ offset: s.fileOffset }, { origin: 'layout' });
      });
      tr.addEventListener('dblclick', () => {
        if (s.fileOffset !== undefined) void store.select({ offset: s.fileOffset }, { view: 'hex' });
      });
      secBody.appendChild(tr);
      this.rows.push({ el: tr, start: s.loaded ? s.address : undefined, end: s.loaded ? s.address + s.size : undefined, fileStart: s.fileOffset, fileEnd: s.fileOffset !== undefined ? s.fileOffset + s.fileSize : undefined });
    }
    secTable.appendChild(secBody);

    const rom = f.summary.format === 'rom';
    this.el.replaceChildren(
      h(
        'div',
        { class: 'page' },
        h('div', { class: 'card' }, h('h2', null, `${rom ? 'Memory map' : 'Segments'} (${f.segments.length})`), h('p', { class: 'sub' }, segmentBlurb(f.summary.format)), h('div', { style: 'overflow:auto' }, segTable)),
        h('div', { class: 'card' }, h('h2', null, `Sections (${f.sections.length})`), h('p', { class: 'sub' }, rom ? 'The ROM’s banks, and the RAM and registers the console’s CPU sees. Double-click to open in the hex view.' : 'Named ranges of the file that the linker and tools work with. Double-click to open in the hex view.'), h('div', { style: 'overflow:auto' }, secTable)),
      ),
    );
    this.onSelection();
  }

  onSelection() {
    const { address, offset } = store.selection;
    for (const r of this.rows) {
      const hit =
        (address !== undefined && r.start !== undefined && r.end !== undefined && address >= r.start && address < r.end) ||
        (address === undefined && offset !== undefined && r.fileStart !== undefined && r.fileEnd !== undefined && offset >= r.fileStart && offset < r.fileEnd);
      r.el.classList.toggle('selected', !!hit);
    }
  }
}

function segmentBlurb(format: string): string {
  switch (format) {
    case 'elf':
      return 'Program headers. PT_LOAD segments define the memory image; the others (PT_DYNAMIC, PT_NOTE, …) describe parts of it.';
    case 'mach-o':
      return 'Mach-O segments from LC_SEGMENT load commands, each mapping a file range to memory with protections.';
    case 'pe':
      return 'PE images map the headers plus each section at ImageBase + RVA.';
    case 'rom':
      return 'Where the console’s CPU sees each part: ROM banks at their addresses (a switched bank’s as bank:address), RAM and hardware registers.';
    default:
      return 'Memory mappings.';
  }
}
