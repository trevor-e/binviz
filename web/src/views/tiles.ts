// Graphics in ROMs: 8×8 tiles in the bit layouts consoles use, drawn from
// any offset of the file, a row of tiles at a time.
import { store } from '../store';
import { h, hex } from '../util';
import { VList } from '../vlist';
import { View } from './base';

/** A tile format: bytes per tile, bits per pixel, and how to read one tile's 64 pixels. */
interface Format {
  id: string;
  label: string;
  bytes: number;
  bpp: number;
  decode: (b: Uint8Array, at: number, out: Uint8Array) => void;
}

const bit = (v: number, x: number) => (v >> (7 - x)) & 1;

/** Planar rows: plane p of row y is at `at + offset(p, y)`. */
function planar(planes: number, offset: (p: number, y: number) => number) {
  return (b: Uint8Array, at: number, out: Uint8Array) => {
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        let v = 0;
        for (let p = 0; p < planes; p++) v |= bit(b[at + offset(p, y)] ?? 0, x) << p;
        out[y * 8 + x] = v;
      }
    }
  };
}

const FORMATS: Format[] = [
  { id: '1bpp', label: '1 bit', bytes: 8, bpp: 1, decode: planar(1, (_, y) => y) },
  // NES: the 8 rows of plane 0, then the 8 rows of plane 1.
  { id: 'nes', label: '2 bits, NES', bytes: 16, bpp: 2, decode: planar(2, (p, y) => p * 8 + y) },
  // Game Boy, SNES 2bpp: each row's two planes side by side.
  { id: 'gb', label: '2 bits, Game Boy / SNES', bytes: 16, bpp: 2, decode: planar(2, (p, y) => y * 2 + p) },
  // SNES 4bpp: planes 0-1 as above, then planes 2-3.
  { id: 'snes4', label: '4 bits, SNES / PC Engine', bytes: 32, bpp: 4, decode: planar(4, (p, y) => (p >> 1) * 16 + y * 2 + (p & 1)) },
  { id: 'snes8', label: '8 bits, SNES', bytes: 64, bpp: 8, decode: planar(8, (p, y) => (p >> 1) * 16 + y * 2 + (p & 1)) },
  {
    id: 'gba4',
    label: '4 bits, GBA / DS (linear)',
    bytes: 32,
    bpp: 4,
    decode: (b, at, out) => {
      for (let i = 0; i < 64; i++) {
        const v = b[at + (i >> 1)] ?? 0;
        out[i] = i & 1 ? v >> 4 : v & 15;
      }
    },
  },
  {
    id: 'md4',
    label: '4 bits, Mega Drive (linear)',
    bytes: 32,
    bpp: 4,
    decode: (b, at, out) => {
      for (let i = 0; i < 64; i++) {
        const v = b[at + (i >> 1)] ?? 0;
        out[i] = i & 1 ? v & 15 : v >> 4;
      }
    },
  },
  {
    id: 'gba8',
    label: '8 bits, GBA (linear)',
    bytes: 64,
    bpp: 8,
    decode: (b, at, out) => {
      for (let i = 0; i < 64; i++) out[i] = b[at + i] ?? 0;
    },
  },
];

/** Colors for pixel values: grays, grays inverted, or distinct hues to tell values apart. */
function palette(kind: string, bpp: number): [number, number, number][] {
  const n = 1 << Math.min(bpp, 8);
  return Array.from({ length: n }, (_, v) => {
    const t = n === 1 ? 0 : v / (n - 1);
    if (kind === 'hues') {
      if (v === 0) return [0, 0, 0];
      const hue = ((v * 137.5) % 360) / 360;
      const f = (k: number) => {
        const x = (k + hue * 12) % 12;
        return Math.round(255 * (0.55 - 0.4 * Math.max(-1, Math.min(x - 3, 9 - x, 1))));
      };
      return [f(0), f(8), f(4)];
    }
    const g = Math.round(255 * (kind === 'inverted' ? 1 - t : t));
    return [g, g, g];
  });
}

const PAGE = 1 << 16;

export class TilesView extends View {
  private format = FORMATS[1];
  private start = 0;
  private columns = 16;
  private zoom = 3;
  private colors = 'gray';
  private list!: VList;
  private status!: HTMLElement;
  private pages = new Map<number, Uint8Array>();
  private loading = new Set<number>();

  constructor() {
    super('tiles', true);
  }

  protected render() {
    const f = store.file!;
    this.pages.clear();
    this.loading.clear();
    // Where a console's graphics usually are, and how they are laid out.
    const platform = f.summary.properties.find((p) => p.key === 'Platform')?.value ?? '';
    const chr = f.sections.find((s) => s.name === 'CHR ROM');
    const id: Record<string, string> = {
      NES: 'nes',
      'Game Boy': 'gb',
      'Game Boy Color': 'gb',
      SNES: 'snes4',
      'Game Boy Advance': 'gba4',
      'Mega Drive / Genesis': 'md4',
    };
    this.format = FORMATS.find((x) => x.id === (id[platform] ?? 'nes'))!;
    this.start = chr?.fileOffset !== undefined ? Number(chr.fileOffset) : 0;

    const fmt = h('select', { class: 'field small', 'aria-label': 'Tile format' }, ...FORMATS.map((x) => h('option', { value: x.id, selected: x === this.format }, x.label)));
    fmt.addEventListener('change', () => {
      this.format = FORMATS.find((x) => x.id === fmt.value)!;
      this.reset();
    });
    const at = h('input', { class: 'field small mono', value: hex(BigInt(this.start)), style: 'width:12ch', 'aria-label': 'Start offset', title: 'File offset of the first tile (Enter)' });
    at.addEventListener('keydown', (e) => {
      if (e.key !== 'Enter') return;
      const v = Number.parseInt(at.value.replace(/^0x/i, ''), 16);
      if (Number.isFinite(v)) {
        this.start = Math.max(0, v);
        this.reset();
      }
    });
    const fromSel = h('button', { class: 'btn small', type: 'button', title: 'Start at the selected byte' }, 'From selection');
    fromSel.addEventListener('click', () => {
      const o = store.selection.offset;
      if (o === undefined) return;
      this.start = Number(o);
      at.value = hex(o);
      this.reset();
    });
    // Nudge the start a byte at a time, to line tiles up.
    const nudge = (d: number) => {
      const b = h('button', { class: 'btn small', type: 'button', title: d < 0 ? 'One byte earlier' : 'One byte later' }, d < 0 ? '−1' : '+1');
      b.addEventListener('click', () => {
        this.start = Math.max(0, this.start + d);
        at.value = hex(BigInt(this.start));
        this.reset();
      });
      return b;
    };
    const cols = h('select', { class: 'field small', 'aria-label': 'Tiles per row' }, ...[8, 16, 32].map((n) => h('option', { value: String(n), selected: n === this.columns }, `${n} a row`)));
    cols.addEventListener('change', () => {
      this.columns = Number(cols.value);
      this.reset();
    });
    const zoom = h('select', { class: 'field small', 'aria-label': 'Zoom' }, ...[2, 3, 4, 6].map((n) => h('option', { value: String(n), selected: n === this.zoom }, `${n}×`)));
    zoom.addEventListener('change', () => {
      this.zoom = Number(zoom.value);
      this.reset();
    });
    const colors = h(
      'select',
      { class: 'field small', 'aria-label': 'Colors' },
      h('option', { value: 'gray', selected: this.colors === 'gray' }, 'Grays'),
      h('option', { value: 'inverted', selected: this.colors === 'inverted' }, 'Grays, inverted'),
      h('option', { value: 'hues', selected: this.colors === 'hues' }, 'Hues'),
    );
    colors.addEventListener('change', () => {
      this.colors = colors.value;
      this.list.refresh();
    });
    this.status = h('span', { class: 'secondary mono' });
    this.list = new VList({ rowHeight: this.rowHeight(), className: 'tiles', renderRow: (i) => this.row(i) });
    this.el.replaceChildren(h('div', { class: 'toolbar' }, fmt, at, nudge(-1), nudge(1), fromSel, cols, zoom, colors, h('span', { class: 'spacer' }), this.status), this.list.el);
    this.reset();
  }

  private rowHeight() {
    return 8 * this.zoom + 2;
  }

  private reset() {
    if (!this.list) return;
    const size = Number(store.file!.summary.fileSize);
    const tiles = Math.max(0, Math.floor((size - this.start) / this.format.bytes));
    this.list = new VList({ rowHeight: this.rowHeight(), className: 'tiles', renderRow: (i) => this.row(i) });
    this.el.lastElementChild?.replaceWith(this.list.el);
    this.list.setCount(Math.ceil(tiles / this.columns));
    this.status.textContent = `${tiles.toLocaleString()} tiles from ${hex(BigInt(this.start))}`;
  }

  /** The bytes of `start..end`, if loaded (asking for them otherwise). */
  private bytes(start: number, end: number): Uint8Array | null {
    const first = Math.floor(start / PAGE);
    const last = Math.floor((end - 1) / PAGE);
    const out = new Uint8Array(end - start);
    for (let p = first; p <= last; p++) {
      const page = this.pages.get(p);
      if (!page) {
        if (!this.loading.has(p)) {
          this.loading.add(p);
          void store.readBytes(p * PAGE, (p + 1) * PAGE).then((b) => {
            this.pages.set(p, b);
            this.loading.delete(p);
            this.list.refresh();
          });
        }
        return null;
      }
      const from = Math.max(start, p * PAGE);
      const to = Math.min(end, (p + 1) * PAGE);
      out.set(page.subarray(from - p * PAGE, to - p * PAGE), from - start);
    }
    return out;
  }

  private row(i: number): HTMLElement {
    const { bytes: per, decode, bpp } = this.format;
    const start = this.start + i * this.columns * per;
    const row = h('div', { class: 'tile-row' }, h('span', { class: 'addr' }, hex(BigInt(start))));
    const data = this.bytes(start, start + this.columns * per);
    if (!data) return row;
    const canvas = h('canvas', { width: this.columns * 8, height: 8 }) as HTMLCanvasElement;
    canvas.style.width = `${this.columns * 8 * this.zoom}px`;
    canvas.style.height = `${8 * this.zoom}px`;
    const ctx = canvas.getContext('2d')!;
    const image = ctx.createImageData(this.columns * 8, 8);
    const colors = palette(this.colors, bpp);
    const px = new Uint8Array(64);
    for (let t = 0; t < this.columns; t++) {
      if ((t + 1) * per > data.length) break;
      decode(data, t * per, px);
      for (let y = 0; y < 8; y++) {
        for (let x = 0; x < 8; x++) {
          const c = colors[px[y * 8 + x]] ?? [255, 0, 255];
          const o = (y * this.columns * 8 + t * 8 + x) * 4;
          image.data[o] = c[0];
          image.data[o + 1] = c[1];
          image.data[o + 2] = c[2];
          image.data[o + 3] = 255;
        }
      }
    }
    ctx.putImageData(image, 0, 0);
    const offsetAt = (e: MouseEvent) => {
      const col = Math.min(this.columns - 1, Math.floor(e.offsetX / (8 * this.zoom)));
      return start + col * per;
    };
    canvas.addEventListener('mousemove', (e) => {
      const o = offsetAt(e);
      this.status.textContent = `tile ${((o - this.start) / per).toLocaleString()} at ${hex(BigInt(o))}`;
    });
    canvas.addEventListener('click', (e) => void store.select({ offset: BigInt(offsetAt(e)) }, { origin: 'tiles' }));
    canvas.addEventListener('dblclick', (e) => void store.select({ offset: BigInt(offsetAt(e)) }, { view: 'hex' }));
    row.appendChild(canvas);
    return row;
  }
}
