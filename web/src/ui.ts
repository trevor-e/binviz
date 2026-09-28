// Shared UI pieces: tooltip, toasts, swatches, legend.
import { FAMILIES, KIND_LABELS, familyOf, type Family } from './colors';
import type { RegionKind } from './types';
import { h, icon } from './util';

// --- Tooltip ----------------------------------------------------------------

const tip = h('div', { class: 'tooltip', role: 'tooltip' });
tip.style.display = 'none';
document.body.appendChild(tip);

export const tooltip = {
  show(x: number, y: number, ...content: (Node | string)[]) {
    tip.replaceChildren(...content);
    tip.style.display = 'block';
    const { innerWidth: w, innerHeight: hgt } = window;
    const r = tip.getBoundingClientRect();
    let left = x + 14;
    let top = y + 16;
    if (left + r.width > w - 8) left = Math.max(8, x - r.width - 14);
    if (top + r.height > hgt - 8) top = Math.max(8, y - r.height - 12);
    tip.style.left = `${left}px`;
    tip.style.top = `${top}px`;
  },
  hide() {
    tip.style.display = 'none';
  },
};

// --- Toasts -----------------------------------------------------------------

const toasts = h('div', { class: 'toast-host', 'aria-live': 'polite' });
document.body.appendChild(toasts);

export function toast(message: string, kind: 'info' | 'error' = 'info', ms = 6000) {
  const close = icon('close');
  const el = h('div', { class: `toast ${kind}`, role: kind === 'error' ? 'alert' : 'status' }, h('div', { style: 'flex:1' }, message), close);
  const remove = () => el.remove();
  close.addEventListener('click', remove);
  toasts.appendChild(el);
  if (ms > 0) setTimeout(remove, ms);
}

// --- Swatches & legend ------------------------------------------------------

export function famClass(kind: RegionKind): string {
  return `fam-${familyOf(kind)}`;
}

export function swatch(kind: RegionKind): HTMLElement {
  return h('span', { class: `swatch ${famClass(kind)}`, title: KIND_LABELS[kind] });
}

export function familySwatch(f: Family): HTMLElement {
  return h('span', { class: `swatch fam-${f}` });
}

/** Legend of colour families; `present` limits it to families in use. */
export function legend(present?: Set<Family>): HTMLElement {
  return h(
    'div',
    { class: 'legend' },
    FAMILIES.filter((f) => !present || present.has(f.id)).map((f) => h('span', { title: f.description }, familySwatch(f.id), f.label)),
  );
}

export function kindLabel(kind: RegionKind): string {
  return KIND_LABELS[kind];
}

export function emptyState(title: string, body?: string, ...extra: Node[]): HTMLElement {
  return h('div', { class: 'empty-state' }, h('strong', null, title), body ? h('div', null, body) : null, ...extra);
}
