// Virtualized list with fixed row height. Handles millions of rows: past the
// browsers' maximum element height the scroll position is scaled.
import { h } from './util';

const MAX_HEIGHT = 8_000_000;

export interface VListOptions {
  rowHeight: number;
  renderRow: (index: number) => HTMLElement;
  overscan?: number;
  className?: string;
  /** Called after each render with the visible index range. */
  onRange?: (first: number, last: number) => void;
}

export class VList {
  readonly el: HTMLElement;
  private spacer: HTMLElement;
  private window: HTMLElement;
  private count = 0;
  private scale = 1;
  private frame = 0;
  private opts: VListOptions;

  constructor(opts: VListOptions) {
    this.opts = opts;
    this.spacer = h('div', { class: 'vlist-spacer' });
    this.window = h('div', { class: 'vlist-window' });
    this.el = h('div', { class: `vlist ${opts.className ?? ''}`, tabindex: '0' }, this.spacer, this.window);
    this.el.addEventListener('scroll', () => this.schedule());
    new ResizeObserver(() => this.schedule()).observe(this.el);
  }

  get length() {
    return this.count;
  }

  setCount(n: number) {
    this.count = n;
    const total = n * this.opts.rowHeight;
    this.scale = total > MAX_HEIGHT ? MAX_HEIGHT / total : 1;
    this.spacer.style.height = `${Math.round(total * this.scale)}px`;
    this.render();
  }

  /** Re-renders visible rows (e.g. after data changed). */
  refresh() {
    this.render();
  }

  private schedule() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.render();
    });
  }

  /** Index of the first fully or partially visible row. */
  firstVisible(): number {
    return Math.floor(this.el.scrollTop / this.scale / this.opts.rowHeight);
  }

  visibleCount(): number {
    return Math.ceil(this.el.clientHeight / this.opts.rowHeight);
  }

  private render() {
    const rh = this.opts.rowHeight;
    const overscan = this.opts.overscan ?? 6;
    const virtualTop = this.el.scrollTop / this.scale;
    const first = Math.max(0, Math.floor(virtualTop / rh) - overscan);
    const last = Math.min(this.count, Math.ceil((virtualTop + this.el.clientHeight) / rh) + overscan);
    // Place the window so that row `first` lands where it would at scale 1.
    const top = this.el.scrollTop - (virtualTop - first * rh);
    this.window.style.transform = `translateY(${Math.max(0, top)}px)`;
    const frag = document.createDocumentFragment();
    for (let i = first; i < last; i++) frag.appendChild(this.opts.renderRow(i));
    this.window.replaceChildren(frag);
    this.opts.onRange?.(first, last);
  }

  scrollToIndex(index: number, align: 'start' | 'center' | 'nearest' = 'nearest') {
    const rh = this.opts.rowHeight;
    const viewRows = this.el.clientHeight / rh;
    const first = this.el.scrollTop / this.scale / rh;
    let target: number;
    if (align === 'start') target = index;
    else if (align === 'center') target = index - viewRows / 2 + 0.5;
    else if (index < first) target = index;
    else if (index + 1 > first + viewRows) target = index + 1 - viewRows;
    else return;
    this.el.scrollTop = Math.max(0, target * rh * this.scale);
    this.render();
  }
}
