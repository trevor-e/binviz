// Where a source file's or compilation unit's code and data ended up: its
// bytes per section, drawn to scale, and its functions and variables (the
// Sources view shows it for what is picked there).
import { KIND_LABELS, familyColors, familyOf } from '../colors';
import { cssVar, roundRect, setup } from '../canvas';
import { store } from '../store';
import type { AttributedRange, AttributionMode, Contributor, Section } from '../types';
import { famClass, tile, tooltip } from '../ui';
import { formatCount, formatSize, h, hex, num, percent } from '../util';

const CONTENT = new Set(['code', 'rodata', 'data', 'bss', 'tls']);

/** A tiny bar of a contributor's bytes per section, coloured by section kind. */
export function fingerprint(c: Contributor, sections: Section[]): HTMLElement {
  const bar = h('div', { class: 'fp' });
  for (const s of c.sections) {
    const bytes = num(s.code + s.data);
    const sec = sections[s.section];
    if (bytes === 0 || !sec) continue;
    bar.appendChild(h('span', { class: famClass(sec.kind), style: `flex:${bytes} 1 0`, title: `${sec.name}: ${formatSize(bytes)}` }));
  }
  return bar;
}

/** What a contributor's code and data amount to, in words. */
export function footprintSummary(c: Contributor): string {
  const parts = [c.code > 0n ? `${formatSize(c.code)} of code in ${formatCount(c.functions)} function${c.functions === 1 ? '' : 's'}` : 'no code'];
  parts.push(c.data > 0n ? `${formatSize(c.data)} of data in ${formatCount(c.variables)} variable${c.variables === 1 ? '' : 's'}` : 'no global data');
  return parts.join(' · ');
}

/**
 * Draws where a contributor's bytes are: a strip per section it has bytes in
 * (every content section, when there are few), and, unless `compact`, figures
 * and its functions and variables. Returns the painters to re-run when the
 * theme changes.
 */
export async function renderFootprint(host: HTMLElement, mode: AttributionMode, c: Contributor, opts: { compact: boolean; isCurrent: () => boolean }): Promise<(() => void)[]> {
  const f = store.file!;
  const ranges = await store.api.attributedRanges(mode, c.id);
  if (!opts.isCurrent()) return [];
  const painters: (() => void)[] = [];
  const own = new Set(c.sections.map((s) => s.section));
  const content = f.sections.filter((s) => s.loaded && s.size > 0n && CONTENT.has(s.kind));
  const shown = f.sections.filter((s) => own.has(s.index) || (!opts.compact && content.length <= 12 && content.includes(s)));
  shown.sort((a, b) => (a.address < b.address ? -1 : a.address > b.address ? 1 : 0));
  const strips = h('div', { class: 'strips' });
  for (const s of shown) {
    const share = c.sections.find((x) => x.section === s.index);
    const bytes = share ? share.code + share.data : 0n;
    const canvas = h('canvas', { class: 'strip', role: 'img', 'aria-label': `${s.name}: ${formatSize(bytes)} from ${c.name}` });
    strips.appendChild(
      h(
        'div',
        { class: `strip-row${bytes === 0n ? ' none' : ''}` },
        h('div', { class: 'strip-label' }, h('span', { class: `swatch ${famClass(s.kind)}` }), h('span', { class: 'mono' }, s.name), h('span', { class: 'muted' }, KIND_LABELS[s.kind].toLowerCase())),
        canvas,
        h('div', { class: 'strip-value' }, bytes > 0n ? `${formatSize(bytes)} · ${percent(num(bytes), num(s.size))}` : 'none'),
      ),
    );
    painters.push(stripOfRanges(canvas, s, ranges.filter((r) => r.section === s.index)));
  }
  const groups = groupRanges(ranges);
  const table = functionsTable(groups);
  if (opts.compact) {
    const details = h(
      'details',
      { class: 'fp-functions' },
      h('summary', null, `Functions and variables (${formatCount(groups.length)})`),
      h('p', { class: 'sub' }, mode === 'file' ? 'Grouped by the function the code was compiled into; inlined code counts toward the file it was written in.' : 'Code grouped by the function containing it.'),
      table,
    );
    host.replaceChildren(
      h('div', { class: 'fp-summary' }, h('span', null, footprintSummary(c)), h('span', { class: 'muted' }, 'Each strip is a whole section, to scale: marks are where this file’s code and variables are. Click one to go there.')),
      strips,
      groups.length ? details : h('span'),
    );
  } else {
    host.replaceChildren(
      h(
        'div',
        { class: 'tiles' },
        tile('Code', formatSize(c.code), `${formatCount(c.functions)} function${c.functions === 1 ? '' : 's'}`),
        tile('Data', formatSize(c.data), `${formatCount(c.variables)} global variable${c.variables === 1 ? '' : 's'}`),
        tile('Sections', String(own.size), [...own].map((i) => f.sections[i]?.name ?? '?').join(', ')),
      ),
      h('h4', { class: 'fp-head' }, 'Where its bytes are'),
      h('p', { class: 'sub' }, 'Each strip is a whole section, drawn to scale; marks are this unit’s code (from the line tables) and global variables (from DWARF). Click a mark to go there.'),
      strips,
      h('h4', { class: 'fp-head' }, `Functions and variables (${formatCount(groups.length)})`),
      h('p', { class: 'sub' }, 'Code grouped by the function containing it.'),
      table,
    );
  }
  requestAnimationFrame(() => painters.forEach((p) => p()));
  return painters;
}

function functionsTable(groups: Group[]): HTMLElement {
  const f = store.file!;
  if (groups.length === 0) return h('div', { class: 'muted' }, 'No ranges.');
  const rows = groups.slice(0, 500).map((g) => {
    const sec = g.section !== undefined ? f.sections[g.section] : undefined;
    const tr = h(
      'tr',
      { class: 'clickable' },
      h('td', { class: 'mono', style: 'max-width:420px;overflow:hidden;text-overflow:ellipsis', title: g.label }, g.label),
      h('td', null, h('span', { class: `swatch ${sec ? famClass(sec.kind) : ''}` }), g.data ? 'variable' : 'code'),
      h('td', { class: 'mono' }, sec?.name ?? '—'),
      h('td', { class: 'mono' }, hex(g.start)),
      h('td', { class: 'right' }, formatSize(g.bytes), g.fragments > 1 ? h('span', { class: 'muted' }, ` in ${g.fragments}`) : null),
      h('td', { class: 'right muted' }, g.line ? String(g.line) : ''),
    );
    tr.addEventListener('click', () => go(g.start, !g.data));
    return tr;
  });
  return h(
    'div',
    { class: 'table-scroll' },
    h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Name'), h('th', null, 'Kind'), h('th', null, 'Section'), h('th', null, 'Address'), h('th', { class: 'right' }, 'Bytes'), h('th', { class: 'right' }, 'Line'))), h('tbody', null, rows)),
    groups.length > 500 ? h('p', { class: 'muted' }, `Showing 500 of ${formatCount(groups.length)}.`) : null,
  );
}

/** A section strip with marks for attributed ranges; returns its painter. */
function stripOfRanges(canvas: HTMLCanvasElement, s: Section, ranges: AttributedRange[]): () => void {
  const span = num(s.size);
  const paint = () => {
    const w = canvas.clientWidth || 600;
    const g = setup(canvas, w, 18);
    const colors = familyColors();
    g.fillStyle = cssVar('--surface-2');
    roundRect(g, 0, 0, w, 18, 3);
    g.fillStyle = colors[familyOf(s.kind)];
    for (const r of ranges) {
      const x0 = ((num(r.start) - num(s.address)) / span) * w;
      const x1 = ((num(r.end) - num(s.address)) / span) * w;
      g.fillRect(Math.floor(x0), 2, Math.max(2, x1 - x0), 14);
    }
  };
  const find = (e: PointerEvent) => {
    const rect = canvas.getBoundingClientRect();
    const a = num(s.address) + ((e.clientX - rect.left) / rect.width) * span;
    const slack = (3 / rect.width) * span;
    let best: AttributedRange | undefined;
    let bestD = Infinity;
    for (const r of ranges) {
      const d = a < num(r.start) ? num(r.start) - a : a >= num(r.end) ? a - num(r.end) : 0;
      if (d <= slack && d < bestD) {
        best = r;
        bestD = d;
      }
    }
    return best;
  };
  canvas.addEventListener('pointermove', (e) => {
    const r = find(e);
    canvas.style.cursor = r ? 'pointer' : 'default';
    if (!r) return tooltip.hide();
    tooltip.show(
      e.clientX,
      e.clientY,
      h('div', { class: 't-value' }, r.label ?? hex(r.start)),
      h('div', null, `${hex(r.start)}..${hex(r.end)} · ${formatSize(r.end - r.start)}`),
      h('div', { class: 'muted' }, `${r.data ? 'global variable' : 'code'}${r.line ? `, line ${r.line}` : ''}`),
    );
  });
  canvas.addEventListener('pointerleave', () => tooltip.hide());
  canvas.addEventListener('click', (e) => {
    const r = find(e);
    if (r) go(r.start, !r.data);
  });
  return paint;
}

function go(address: bigint, code: boolean) {
  void store.select({ address }, { view: code ? 'code' : 'hex' });
}

interface Group {
  label: string;
  data: boolean;
  start: bigint;
  bytes: bigint;
  section?: number;
  line: number;
  fragments: number;
}

function groupRanges(ranges: AttributedRange[]): Group[] {
  const groups = new Map<string, Group>();
  for (const r of ranges) {
    const label = r.label ?? hex(r.start);
    const key = `${r.data ? 'd' : 'c'}:${label}`;
    const g = groups.get(key);
    if (g) {
      g.bytes += r.end - r.start;
      g.fragments++;
      if (r.start < g.start) g.start = r.start;
      if (r.line && (!g.line || r.line < g.line)) g.line = r.line;
    } else {
      groups.set(key, { label, data: r.data, start: r.start, bytes: r.end - r.start, section: r.section, line: r.line, fragments: 1 });
    }
  }
  return [...groups.values()].sort((a, b) => (a.start < b.start ? -1 : a.start > b.start ? 1 : 0));
}
