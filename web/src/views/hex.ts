// Hex view: every byte, tinted by the structure it belongs to.
import { familyColors, familyOf, type Family } from '../colors';
import { store } from '../store';
import type { Span } from '../types';
import { legend, tooltip } from '../ui';
import { debounce, escapeHtml, h, hex, hexPad, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';

const ROW = 16;
const BLOCK = 4096;
/** Bytes are read from the file in pages this big, as rows come into view. */
const PAGE = 64 * 1024;
/** Pages kept in memory (16 MiB). */
const MAX_PAGES = 256;

export class HexView extends View {
  private list!: VList;
  private spans = new Map<number, Span[]>();
  private loading = new Set<number>();
  private pages = new Map<number, Uint8Array>();
  private pageLoading = new Set<number>();
  private showVA = true;
  private minimap!: HTMLCanvasElement;
  private viewport!: HTMLElement;
  private mapKinds: string[] = [];
  private hover: { start: bigint; end: bigint } | null = null;
  private selRange: { start: bigint; end: bigint } | null = null;
  private status!: HTMLElement;
  private offWidth = 8;

  constructor() {
    super('hex', true);
    window.addEventListener('themechange', () => this.visible && this.drawMinimap());
  }

  protected render() {
    const f = store.file!;
    this.spans.clear();
    this.loading.clear();
    this.pages.clear();
    this.pageLoading.clear();
    this.offWidth = Math.max(8, num(f.summary.fileSize).toString(16).length);
    const hasVA = f.segments.some((s) => s.mapped) || f.sections.some((s) => s.loaded && s.fileOffset !== undefined);
    this.showVA = hasVA;

    const vaToggle = h('input', { type: 'checkbox', checked: this.showVA, disabled: !hasVA });
    vaToggle.addEventListener('change', () => {
      this.showVA = vaToggle.checked;
      header.replaceChildren(...this.headerCells());
      this.list.refresh();
    });
    this.status = h('span', { class: 'mono secondary' });
    const present = new Set<Family>(f.composition.map(([k]) => familyOf(k)));
    const toolbar = h(
      'div',
      { class: 'toolbar' },
      h('h3', null, 'Hex'),
      this.status,
      h('span', { class: 'spacer' }),
      legend(present),
      h('label', { class: 'secondary', style: 'display:flex;gap:4px;align-items:center' }, vaToggle, 'Addresses'),
    );
    const header = h('div', { class: 'hex-header' }, ...this.headerCells());

    this.list = new VList({ rowHeight: 22, className: 'hex', renderRow: (i) => this.row(i), onRange: (a, b) => this.onRange(a, b) });
    this.list.setCount(Math.ceil(num(f.summary.fileSize) / ROW));
    this.list.el.addEventListener('click', (e) => {
      const o = this.offsetFromEvent(e);
      if (o !== undefined) void store.select({ offset: o }, { origin: 'hex' });
    });
    this.list.el.addEventListener('mouseover', (e) => this.onHover(e));
    this.list.el.addEventListener('mouseleave', () => {
      tooltip.hide();
      this.setHover(null);
    });
    this.list.el.addEventListener('keydown', (e) => this.onKey(e));

    this.minimap = h('canvas');
    this.viewport = h('div', { class: 'viewport' });
    const mm = h('div', { class: 'minimap', title: 'File minimap: click or drag to scroll' }, this.minimap, this.viewport);
    let dragging = false;
    const jump = (e: PointerEvent) => {
      const r = mm.getBoundingClientRect();
      const t = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
      this.list.scrollToIndex(Math.floor(t * this.list.length), 'center');
    };
    mm.addEventListener('pointerdown', (e) => {
      dragging = true;
      mm.setPointerCapture(e.pointerId);
      jump(e);
    });
    mm.addEventListener('pointermove', (e) => dragging && jump(e));
    mm.addEventListener('pointerup', () => (dragging = false));
    new ResizeObserver(debounce(() => this.visible && void this.loadMinimap(), 150)).observe(mm);

    this.el.replaceChildren(toolbar, header, h('div', { class: 'hex-wrap' }, this.list.el, mm));
    requestAnimationFrame(() => void this.loadMinimap());
  }

  private headerCells(): HTMLElement[] {
    const cols = Array.from({ length: ROW }, (_, i) => h('span', { style: `width:3ch;text-align:center${i === 8 ? ';margin-left:1ch' : ''}` }, hexPad(i, 2)));
    return [
      h('span', { style: `width:${this.offWidth + 3}ch;flex:none` }, 'Offset'),
      this.showVA ? h('span', { style: 'width:19ch;flex:none' }, 'Address') : h('span'),
      h('span', { style: 'display:flex;margin-right:16px' }, ...cols),
      h('span', null, 'Text'),
    ];
  }

  private blockSpans(block: number): Span[] | undefined {
    const cached = this.spans.get(block);
    if (cached) return cached;
    if (!this.loading.has(block)) {
      this.loading.add(block);
      const start = BigInt(block * BLOCK);
      void store.api.spans(start, start + BigInt(BLOCK)).then((s) => {
        this.loading.delete(block);
        if (this.spans.size > 512) this.spans.clear();
        this.spans.set(block, s);
        this.list.refresh();
      });
    }
    return undefined;
  }

  /** A page of the file's bytes, or undefined while it is being read. */
  private pageBytes(page: number): Uint8Array | undefined {
    const cached = this.pages.get(page);
    if (cached) return cached;
    const f = store.file!;
    if (!this.pageLoading.has(page)) {
      this.pageLoading.add(page);
      const start = page * PAGE;
      void store.readBytes(start, start + PAGE).then((bytes) => {
        this.pageLoading.delete(page);
        if (store.file?.name !== f.name || store.file.summary.fingerprint !== f.summary.fingerprint) return;
        if (this.pages.size >= MAX_PAGES) this.pages.clear();
        this.pages.set(page, bytes);
        this.list.refresh();
      }, (e) => {
        this.pageLoading.delete(page);
        store.error(e);
      });
    }
    return undefined;
  }

  private row(i: number): HTMLElement {
    const f = store.file!;
    const off = i * ROW;
    const end = Math.min(off + ROW, num(f.summary.fileSize));
    const page = Math.floor(off / PAGE);
    const bytes = this.pageBytes(page);
    const base = page * PAGE;
    const spans = this.blockSpans(Math.floor(off / BLOCK)) ?? [];
    let si = 0;
    const sel = store.selection.offset !== undefined ? num(store.selection.offset) : -1;
    const hl = this.selRange;
    const hov = this.hover;
    let hexHtml = '';
    let asciiHtml = '';
    for (let o = off; o < off + ROW; o++) {
      const gap = o - off === 8 ? ' gap' : '';
      if (o >= end) {
        hexHtml += `<span class="b${gap}"> </span>`;
        continue;
      }
      const big = BigInt(o);
      while (si < spans.length && spans[si].end <= big) si++;
      const sp = spans[si] && spans[si].start <= big ? spans[si] : undefined;
      const cls = sp ? `fam-${familyOf(sp.kind)} t${sp.shade}` : '';
      if (!bytes) {
        hexHtml += `<span class="b${gap} pending" data-o="${o}">··</span>`;
        asciiHtml += `<span class="a pending" data-o="${o}"> </span>`;
        continue;
      }
      const b = bytes[o - base];
      let extra = b === 0 ? ' zero' : '';
      if (o === sel) extra += ' sel';
      if ((hl && big >= hl.start && big < hl.end) || (hov && big >= hov.start && big < hov.end)) extra += ' hl';
      hexHtml += `<span class="b${gap} ${cls}${extra}" data-o="${o}">${hexPad(b, 2)}</span>`;
      const ch = b >= 0x20 && b < 0x7f ? escapeHtml(String.fromCharCode(b)) : '·';
      asciiHtml += `<span class="a ${cls}${extra}" data-o="${o}">${ch}</span>`;
    }
    let va = '';
    if (this.showVA) {
      const a = store.addressOf(BigInt(off));
      va = `<span class="va">${a !== undefined ? '0x' + hexPad(a, 16).replace(/^0{1,12}(?=[0-9a-f]{4})/, '') : ''}</span>`;
    }
    const row = document.createElement('div');
    row.className = 'hex-row';
    row.innerHTML = `<span class="off" style="width:${this.offWidth + 3}ch">${hexPad(off, this.offWidth)}</span>${va}<span class="bytes">${hexHtml}</span><span class="ascii">${asciiHtml}</span>`;
    return row;
  }

  private offsetFromEvent(e: Event): bigint | undefined {
    const t = (e.target as HTMLElement).closest('[data-o]') as HTMLElement | null;
    return t ? BigInt(t.dataset.o!) : undefined;
  }

  private hoverSeq = 0;
  private onHover = debounce(async (e: MouseEvent) => {
    const o = this.offsetFromEvent(e);
    if (o === undefined) return;
    const mine = ++this.hoverSeq;
    const ins = await store.api.inspectOffset(o);
    if (mine !== this.hoverSeq) return;
    const leaf = ins.path[ins.path.length - 1];
    this.setHover(leaf ? { start: leaf.start, end: leaf.end } : null);
    const colors = familyColors();
    const rows: Node[] = [h('div', { class: 't-value' }, `${hex(o)}${ins.address !== undefined ? ` · ${hex(ins.address)}` : ''}`)];
    ins.path.slice(-4).forEach((p) =>
      rows.push(h('div', { class: 't-row' }, h('span', { class: 't-key', style: `background:${colors[familyOf(p.kind)]}` }), h('span', null, p.name, p.value ? h('span', { class: 'muted' }, ` = ${truncate(p.value, 60)}`) : null))),
    );
    if (ins.symbol) rows.push(h('div', { class: 'muted' }, `in ${ins.symbol.demangled ?? ins.symbol.name}+${hex(ins.symbol.offset)}`));
    if (ins.source) rows.push(h('div', { class: 'muted' }, `${ins.source.path.split(/[\\/]/).pop()}:${ins.source.line}`));
    tooltip.show(e.clientX, e.clientY, ...rows);
  }, 40);

  private setHover(r: { start: bigint; end: bigint } | null) {
    if (this.hover?.start === r?.start && this.hover?.end === r?.end) return;
    this.hover = r;
    this.list.refresh();
  }

  private onKey(e: KeyboardEvent) {
    const sel = store.selection.offset;
    if (sel === undefined) return;
    const size = store.file!.summary.fileSize;
    const page = BigInt(Math.max(1, this.list.visibleCount() - 2) * ROW);
    const moves: Record<string, bigint> = { ArrowLeft: -1n, ArrowRight: 1n, ArrowUp: -16n, ArrowDown: 16n, PageUp: -page, PageDown: page };
    let next: bigint | undefined;
    if (e.key in moves) next = sel + moves[e.key];
    else if (e.key === 'Home') next = e.ctrlKey ? 0n : sel - (sel % 16n);
    else if (e.key === 'End') next = e.ctrlKey ? size - 1n : sel - (sel % 16n) + 15n;
    if (next === undefined) return;
    e.preventDefault();
    next = next < 0n ? 0n : next >= size ? size - 1n : next;
    this.list.scrollToIndex(num(next / 16n));
    void store.select({ offset: next }, { origin: 'hex', history: false });
  }

  protected onSelection() {
    const sel = store.selection;
    const leaf = sel.inspection?.path[sel.inspection.path.length - 1];
    this.selRange = leaf ? { start: leaf.start, end: leaf.end } : null;
    if (sel.offset !== undefined && sel.origin !== 'hex') this.list.scrollToIndex(num(sel.offset / 16n), 'center');
    this.list.refresh();
    this.status.textContent = sel.offset !== undefined ? `offset ${hex(sel.offset)}${sel.address !== undefined ? ` · address ${hex(sel.address)}` : ''}` : sel.address !== undefined ? `address ${hex(sel.address)} has no file bytes` : '';
  }

  private onRange(first: number, last: number) {
    if (!this.viewport || !this.list) return;
    const total = Math.max(1, this.list.length);
    const hgt = this.minimap.parentElement?.clientHeight ?? 0;
    const visible = Math.max(1, last - first - 12);
    const top = (this.list.firstVisible() / total) * hgt;
    this.viewport.style.top = `${top}px`;
    this.viewport.style.height = `${Math.max(4, (visible / total) * hgt)}px`;
  }

  private async loadMinimap() {
    const parent = this.minimap.parentElement;
    if (!parent || !store.file) return;
    const rows = Math.max(16, Math.min(2000, parent.clientHeight));
    this.mapKinds = await store.api.fileMap(rows);
    this.drawMinimap();
  }

  private drawMinimap() {
    const c = this.minimap;
    const parent = c.parentElement;
    if (!parent || this.mapKinds.length === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(parent.clientWidth * dpr);
    c.height = Math.round(parent.clientHeight * dpr);
    const g = c.getContext('2d')!;
    const colors = familyColors();
    const n = this.mapKinds.length;
    const rh = c.height / n;
    for (let i = 0; i < n; i++) {
      g.fillStyle = colors[familyOf(this.mapKinds[i] as never)] ?? colors.unknown;
      g.fillRect(0, Math.floor(i * rh), c.width, Math.ceil(rh));
    }
    this.onRange(this.list.firstVisible(), this.list.firstVisible() + this.list.visibleCount() + 12);
  }
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + '…' : s;
}

