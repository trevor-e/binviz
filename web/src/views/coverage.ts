// Coverage (a tab of the Layout view): how much of the code and data has been
// mapped out while reverse engineering, section by section, and the largest
// stretches nothing explains yet.
import { STATUSES, STATUS_LABELS, statusColors } from '../colors';
import { setup } from '../canvas';
import { store } from '../store';
import type { Annotation, Coverage, Gap, MapStatus, Section, StatusBytes, Worklist } from '../types';
import { downloadText, emptyState, famClass, smallButton, tile, toast, tooltip } from '../ui';
import { basename, debounce, formatCount, formatSize, h, hex, num, percent } from '../util';
import type { Panel } from './base';

export class CoveragePanel implements Panel {
  readonly el = h('div', { class: 'scroll' });
  private coverage: Coverage | null = null;
  private stale = true;
  /** Canvas painters, re-run when the theme changes. */
  private painters: (() => void)[] = [];
  private generation = 0;

  constructor() {
    window.addEventListener('themechange', () => this.el.isConnected && this.painters.forEach((p) => p()));
    window.addEventListener(
      'resize',
      debounce(() => this.el.isConnected && this.render(), 250),
    );
    store.on('annotations', () => {
      this.coverage = null;
      this.worklist = null;
      this.stale = true;
      if (this.el.isConnected) this.show();
    });
    // How long ago the followed notes file was read.
    store.on('notesfile', () => this.followStatus());
    window.setInterval(() => this.el.isConnected && store.notesFile && this.followStatus(), 5000);
  }

  private worklist: Worklist | null = null;
  private followLine: HTMLElement | null = null;

  reset() {
    this.coverage = null;
    this.worklist = null;
    this.stale = true;
    this.generation++;
  }

  show() {
    if (!this.stale) return;
    this.stale = false;
    this.render();
  }

  private render() {
    this.painters = [];
    tooltip.hide();
    void this.draw(++this.generation);
  }

  private async draw(gen: number) {
    const f = store.file;
    if (!f) return;
    if (!this.coverage) {
      this.el.replaceChildren(h('div', { class: 'empty-state' }, 'Measuring coverage…'));
      const [cov, work] = await Promise.all([store.api.coverage(100), store.api.worklist(12).catch(() => null)]);
      if (gen !== this.generation) return;
      this.coverage = cov;
      this.worklist = work;
    }
    const cov = this.coverage;
    if (cov.sections.length === 0) {
      this.el.replaceChildren(emptyState('No code or data sections', 'Coverage is measured over loaded code and data sections.'));
      return;
    }
    const t = cov.totals;
    const total = sumStatus(t);
    const denom = total - num(t.padding);
    const named = num(t.reviewed) + num(t.annotated) + num(t.named) + num(t.structure);
    const mapped = named + num(t.recovered);
    const fn = cov.functions;

    const fileInput = h('input', { type: 'file', accept: '.json,.csv,.txt,.tsv,.map,.mlb,.nl,.sym', style: 'display:none' });
    fileInput.addEventListener('change', async () => {
      const file = fileInput.files?.[0];
      fileInput.value = '';
      if (!file) return;
      const n = await store.importAnnotations(await file.text(), file.name);
      toast(n > 0 ? `Imported ${formatCount(n)} annotations from ${file.name}` : `No addresses found in ${file.name}`, n > 0 ? 'info' : 'error');
    });
    const importBtn = smallButton('Import…', 'Import annotations: a binviz export, a symbol list (CSV with an address column, nm output, "address name" lines), or for a ROM an emulator\'s label file (.mlb, .nl, .sym)', () => fileInput.click());
    const exportBtn = smallButton('Export', 'Download your annotations as JSON', () => downloadText(`${basename(f.name)}.binviz-notes.json`, store.exportAnnotations(), 'application/json'));
    exportBtn.toggleAttribute('disabled', store.annotations.length === 0);
    // An agent's notes file, followed as it writes: Chrome and Edge can keep a file open.
    const picker = (window as unknown as { showOpenFilePicker?: (o: object) => Promise<FileSystemFileHandle[]> }).showOpenFilePicker;
    const followBtn = picker
      ? smallButton('Follow file…', `Follow a notes file (${basename(f.name)}.binviz-notes.json) while an agent writes it: its notes come in as they are saved, and yours go out to it`, async () => {
          try {
            const [handle] = await picker({ types: [{ description: 'binviz notes', accept: { 'application/json': ['.json'] } }] });
            if (handle) await store.followNotes(handle);
          } catch {
            /* cancelled */
          }
        })
      : null;
    this.followLine = h('div', { class: 'follow-status' });
    this.followStatus();

    const bar = h('div', { class: 'stack-bar tall', role: 'img', 'aria-label': 'Share of code and data bytes by coverage status' });
    for (const s of STATUSES) {
      const bytes = num(t[s.id]);
      if (bytes === 0) continue;
      const seg = h('div', { class: `st-${s.id}`, style: `flex:${bytes} 1 0;background:var(--st)` });
      seg.addEventListener('pointermove', (e) => tooltip.show(e.clientX, e.clientY, h('div', { class: 't-value' }, `${formatSize(bytes)} · ${percent(bytes, total)}`), h('div', null, s.label), h('div', { class: 'muted' }, s.description)));
      seg.addEventListener('pointerleave', () => tooltip.hide());
      bar.appendChild(seg);
    }

    const summary = h(
      'div',
      { class: 'card' },
      h(
        'div',
        { class: 'card-head' },
        h('div', { style: 'flex:1' }, h('h2', null, 'How much of this binary is mapped out'), h('p', { class: 'sub' }, 'Every byte of the loaded code and data sections, by the strongest thing that explains it. Name functions and mark them reviewed from the inspector; the Overview’s file map shows the whole file this way too.')),
        importBtn,
        exportBtn,
        followBtn,
        fileInput,
      ),
      this.followLine,
      h(
        'div',
        { class: 'tiles' },
        tile('Mapped out', percent(mapped, denom), `${formatSize(mapped)} of ${formatSize(denom)} (padding excluded)`),
        tile('Named', percent(named, denom), 'symbols, your names and known structures'),
        tile('Functions', formatCount(fn.named + fn.recovered + fn.user + fn.agents), `${formatCount(fn.named)} named · ${formatCount(fn.recovered)} recovered · ${formatCount(fn.user)} yours${fn.agents ? ` · ${formatCount(fn.agents)} agents’` : ''}`),
        tile('Notes', formatCount(cov.annotations), `${formatCount(cov.reviewed)} marked reviewed${cov.agentNotes ? ` · ${formatCount(cov.agentNotes)} by agents` : ''}`),
        tile('Unexplored', formatSize(t.unexplored), `in ${formatCount(cov.gapCount)} gap${cov.gapCount === 1 ? '' : 's'}`, 'warn'),
      ),
      bar,
      statusLegend(),
    );

    const strips = h('div', { class: 'strips' });
    const sections = [...cov.sections].sort((a, b) => num(b.size) - num(a.size)).slice(0, 40);
    sections.sort((a, b) => (a.address < b.address ? -1 : 1));
    for (const sc of sections) {
      const canvas = h('canvas', { class: 'strip', role: 'img', 'aria-label': `Coverage of ${sc.name}` });
      const top = STATUSES.map((s) => [s, num(sc.bytes[s.id])] as const)
        .filter(([, n]) => n > 0)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([s, n]) => `${percent(n, num(sc.size))} ${s.label.toLowerCase()}`)
        .join(' · ');
      strips.appendChild(
        h(
          'div',
          { class: 'strip-row' },
          h('div', { class: 'strip-label' }, h('span', { class: `swatch ${famClass(sc.kind)}` }), h('span', { class: 'mono' }, sc.name), h('span', { class: 'muted' }, formatSize(sc.size))),
          canvas,
          h('div', { class: 'strip-value' }, top),
        ),
      );
      this.stripOfStatus(canvas, sc.section, sc.address, sc.size, sc.kind === 'code', gen);
    }
    const sectionsCard = h(
      'div',
      { class: 'card' },
      h('h2', null, 'Sections'),
      h('p', { class: 'sub' }, 'Each strip is a whole section at scale. Click to jump there.'),
      strips,
      cov.sections.length > sections.length ? h('p', { class: 'muted' }, `Showing the ${sections.length} largest of ${cov.sections.length} sections.`) : null,
    );

    const gapRows = cov.gaps.map((g) => {
      const sec = f.sections[g.section];
      const tr = h(
        'tr',
        { class: 'clickable' },
        h('td', { class: 'mono' }, `${hex(g.start)}..${hex(g.end)}`),
        h('td', { class: 'right' }, formatSize(g.end - g.start)),
        h('td', { class: 'mono' }, sec?.name ?? '?'),
        h('td', { class: 'mono', style: 'max-width:280px;overflow:hidden;text-overflow:ellipsis', title: g.after ?? '' }, g.after ?? '—'),
        h('td', null, g.hint),
        h('td', { class: 'mono muted' }, g.preview),
      );
      tr.addEventListener('click', () => goGap(g, sec));
      return tr;
    });
    const gapsCard = h(
      'div',
      { class: 'card' },
      h('h2', null, 'Largest unexplored gaps'),
      h('p', { class: 'sub' }, 'Runs of code or data nothing accounts for yet: good places to look next. “Looks like” is a guess from the bytes.'),
      cov.gaps.length === 0
        ? h('div', { class: 'muted' }, 'No unexplored bytes. Everything is named, recovered, structure or padding.')
        : h(
            'div',
            { class: 'table-scroll' },
            h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Range'), h('th', { class: 'right' }, 'Size'), h('th', null, 'Section'), h('th', null, 'After'), h('th', null, 'Looks like'), h('th', null, 'First bytes'))), h('tbody', null, gapRows)),
          ),
      cov.gapCount > cov.gaps.length ? h('p', { class: 'muted' }, `Showing the ${cov.gaps.length} largest of ${formatCount(cov.gapCount)} gaps.`) : null,
    );
    this.el.replaceChildren(h('div', { class: 'page' }, summary, this.nextCard(), this.agentsCard(), sectionsCard, gapsCard));
  }

  /** Where the followed notes file stands. */
  private followStatus() {
    const line = this.followLine;
    if (!line) return;
    const nf = store.notesFile;
    if (!nf) {
      line.replaceChildren();
      line.hidden = true;
      return;
    }
    line.hidden = false;
    const ago = Math.max(0, Math.round((Date.now() - nf.lastRead) / 1000));
    const stop = smallButton('Stop', 'Stop following the file', () => store.stopFollowingNotes());
    line.replaceChildren(
      h('span', { class: 'chip ok' }, 'Following'),
      h('span', { class: 'mono' }, nf.name),
      h('span', { class: 'muted' }, `${nf.writable ? 'shared both ways' : 'read only'} · read ${ago < 5 ? 'just now' : `${ago} s ago`}`),
      stop,
    );
  }

  /** The unnamed functions to look at next: leaves first, then the most called. */
  private nextCard(): HTMLElement | null {
    const w = this.worklist;
    if (!w || w.functions === 0) return null;
    const rows = w.items.map((item) => {
      const row = h(
        'div',
        { class: 'row clickable', title: 'Show its code' },
        h('span', { class: 'mono' }, item.name),
        h('span', { class: 'muted' }, item.callees === 0 ? 'calls nothing' : item.unnamedCallees === 0 ? `calls ${formatCount(item.callees)}, all named` : `calls ${formatCount(item.callees)}, ${formatCount(item.unnamedCallees)} unnamed`),
        h('span', { class: 'muted right' }, item.callers === 0 ? 'no callers' : `${formatCount(item.callers)} caller${item.callers === 1 ? '' : 's'}`),
        h('span', { class: 'muted right' }, formatSize(item.size)),
      );
      row.addEventListener('click', () => void store.select({ address: item.address }, { view: 'code' }));
      return row;
    });
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Next up'),
      h('p', { class: 'sub' }, `${formatCount(w.named)} of ${formatCount(w.functions)} functions have names. Unnamed ones whose callees all have names come first (what they call says what they do), then the most called. An agent works through the same list (the MCP server’s worklist).`),
      rows.length ? h('div', { class: 'work-list' }, rows) : h('div', { class: 'muted' }, 'Every function has a name.'),
    );
  }

  /** Names agents gave, to confirm or correct. */
  private agentsCard(): HTMLElement | null {
    const theirs = store.annotations.filter((a) => a.author);
    if (theirs.length === 0) return null;
    const confirm = (list: Annotation[]) => void store.setAnnotations(store.annotations.map((a) => (list.includes(a) ? { ...a, author: undefined } : a)));
    const rows = theirs.slice(0, 200).map((a) => {
      const ok = smallButton('Confirm', 'Make it yours', () => confirm([a]));
      const name = h('span', { class: 'mono clickable', title: 'Show it' }, a.name || fmtNote(a));
      name.addEventListener('click', () => void store.select({ address: a.address }, { view: 'code' }));
      return h('div', { class: 'row' }, name, h('span', { class: 'muted', title: a.comment }, a.comment ? truncate(a.comment, 90) : '—'), h('span', { class: 'chip agent' }, a.author!), ok);
    });
    return h(
      'div',
      { class: 'card' },
      h('div', { class: 'card-head' }, h('div', { style: 'flex:1' }, h('h2', null, 'Agents’ names to check'), h('p', { class: 'sub' }, `${formatCount(theirs.length)} notes an agent wrote. Their names are guesses until you confirm them; editing a name makes it yours.`)), smallButton('Confirm all', 'Make every agent’s note yours', () => confirm(theirs))),
      h('div', { class: 'work-list' }, rows),
      theirs.length > rows.length ? h('p', { class: 'muted' }, `Showing ${rows.length} of ${formatCount(theirs.length)}.`) : null,
    );
  }

  /** A section strip coloured by coverage status. */
  private stripOfStatus(canvas: HTMLCanvasElement, section: number, address: bigint, size: bigint, code: boolean, gen: number) {
    let data: MapStatus[] = [];
    const paint = () => {
      const w = canvas.clientWidth || 600;
      const g = setup(canvas, w, 18);
      const colors = statusColors();
      const n = data.length;
      if (n === 0) return;
      // Runs of one status as single rects, so thin strips stay crisp.
      let start = 0;
      for (let i = 1; i <= n; i++) {
        if (i === n || data[i] !== data[start]) {
          g.fillStyle = colors[data[start]];
          const x0 = Math.round((start / n) * w);
          const x1 = Math.round((i / n) * w);
          g.fillRect(x0, 0, Math.max(1, x1 - x0), 18);
          start = i;
        }
      }
    };
    const addrAt = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect();
      const t = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
      return { t, a: address + BigInt(Math.floor(t * num(size))) };
    };
    canvas.addEventListener('pointermove', (e) => {
      if (data.length === 0) return;
      const { t, a } = addrAt(e);
      const st = data[Math.min(data.length - 1, Math.floor(t * data.length))];
      tooltip.show(e.clientX, e.clientY, h('div', { class: 't-value' }, hex(a)), h('div', { class: 't-row' }, h('span', { class: 't-key', style: `background:${statusColors()[st]}` }), STATUS_LABELS[st]));
    });
    canvas.addEventListener('pointerleave', () => tooltip.hide());
    canvas.addEventListener('click', (e) => go(addrAt(e).a, code));
    this.painters.push(paint);
    requestAnimationFrame(() => {
      const buckets = Math.max(64, Math.min(2048, Math.round(canvas.clientWidth || 600)));
      void store.api.coverageStrip(section, buckets).then((d) => {
        if (gen !== this.generation) return;
        data = d;
        paint();
      });
    });
  }
}

function go(address: bigint, code: boolean) {
  void store.select({ address }, { view: code ? 'code' : 'hex' });
}

function goGap(g: Gap, sec: Section | undefined) {
  if (sec?.kind === 'code' || g.offset === undefined) go(g.start, sec?.kind === 'code');
  else void store.select({ address: g.start }, { view: 'hex' });
}

function sumStatus(t: StatusBytes): number {
  return STATUSES.reduce((a, s) => a + num(t[s.id]), 0);
}

export function statusLegend(): HTMLElement {
  return h(
    'div',
    { class: 'legend' },
    STATUSES.map((s) => h('span', { title: s.description }, h('span', { class: `swatch st-${s.id}` }), s.label)),
  );
}

function fmtNote(a: Annotation): string {
  return `note at ${hex(a.address)}`;
}

function truncate(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}
