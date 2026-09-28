// DOM and formatting helpers.

type Attrs = Record<string, unknown> & {
  class?: string;
  style?: Partial<CSSStyleDeclaration> | string;
  dataset?: Record<string, string>;
};
type Child = Node | string | number | null | undefined | false | Child[];

/** Creates an element. Text children become text nodes (never parsed as HTML). */
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs?: Attrs | null, ...children: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (attrs) {
    for (const [k, v] of Object.entries(attrs)) {
      if (v === undefined || v === null || v === false) continue;
      if (k === 'class') el.className = String(v);
      else if (k === 'style') {
        if (typeof v === 'string') el.setAttribute('style', v);
        else Object.assign(el.style, v);
      } else if (k === 'dataset') Object.assign(el.dataset, v);
      else if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v as EventListener);
      else if (v === true) el.setAttribute(k, '');
      else el.setAttribute(k, String(v));
    }
  }
  append(el, children);
  return el;
}

function append(el: Node, children: Child[]) {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    if (Array.isArray(c)) append(el, c);
    else if (c instanceof Node) el.appendChild(c);
    else el.appendChild(document.createTextNode(String(c)));
  }
}

export function clear(el: Element) {
  while (el.firstChild) el.removeChild(el.firstChild);
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}

/** `0x1f` for any integer-ish value. */
export function hex(v: bigint | number | undefined | null): string {
  if (v === undefined || v === null) return '—';
  return '0x' + v.toString(16);
}

/** Zero-padded hex without prefix. */
export function hexPad(v: bigint | number, width: number): string {
  return v.toString(16).padStart(width, '0');
}

export function num(v: bigint | number): number {
  return typeof v === 'bigint' ? Number(v) : v;
}

export function formatSize(v: bigint | number): string {
  const n = num(v);
  if (n < 1024) return `${n} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB'];
  let x = n / 1024;
  let u = 0;
  while (x >= 1024 && u < units.length - 1) {
    x /= 1024;
    u++;
  }
  return `${x >= 100 ? x.toFixed(0) : x >= 10 ? x.toFixed(1) : x.toFixed(2)} ${units[u]}`;
}

export function formatCount(n: number): string {
  return n.toLocaleString('en-US');
}

export function percent(part: number, whole: number): string {
  if (whole === 0) return '0%';
  const p = (part * 100) / whole;
  return p >= 10 ? `${p.toFixed(0)}%` : p >= 0.1 ? `${p.toFixed(1)}%` : p > 0 ? '<0.1%' : '0%';
}

export function debounce<A extends unknown[]>(fn: (...a: A) => void, ms: number): (...a: A) => void {
  let t: ReturnType<typeof setTimeout> | undefined;
  return (...a: A) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}

export function basename(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i >= 0 ? path.slice(i + 1) : path;
}

/** Parses 0x-prefixed hex or decimal into a bigint. */
export function parseNumber(s: string): bigint | undefined {
  const t = s.trim();
  try {
    if (/^0x[0-9a-f]+$/i.test(t)) return BigInt(t);
    if (/^[0-9]+$/.test(t)) return BigInt(t);
  } catch {
    /* fallthrough */
  }
  return undefined;
}

export function copyText(text: string) {
  void navigator.clipboard?.writeText(text);
}

/** Tracks the latest of several async requests so stale responses are dropped. */
export class Latest {
  private seq = 0;
  async run<T>(p: Promise<T>): Promise<T | undefined> {
    const mine = ++this.seq;
    const v = await p;
    return mine === this.seq ? v : undefined;
  }
}

export function icon(name: 'open' | 'debug' | 'source' | 'theme' | 'back' | 'forward' | 'copy' | 'chevron' | 'close'): SVGSVGElement {
  const paths: Record<string, string> = {
    open: 'M3 7h6l2 2h10v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z',
    debug: 'M9 4h6M12 4v3m-5 3a5 5 0 0 1 10 0v4a5 5 0 0 1-10 0zm-3 1h3m10 0h3M4 17h3m10 0h3',
    source: 'M8 8l-4 4 4 4m8-8l4 4-4 4M14 5l-4 14',
    theme: 'M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9z',
    back: 'M15 6l-6 6 6 6',
    forward: 'M9 6l6 6-6 6',
    copy: 'M9 9h10v10H9zM5 15V5h10',
    chevron: 'M9 6l6 6-6 6',
    close: 'M6 6l12 12M18 6L6 18',
  };
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('class', `icon icon-${name}`);
  svg.setAttribute('aria-hidden', 'true');
  const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  p.setAttribute('d', paths[name]);
  svg.appendChild(p);
  return svg;
}
