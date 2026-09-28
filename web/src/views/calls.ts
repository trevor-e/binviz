// Call graph view: who calls the selected function and what it calls, a few
// levels each way, in columns (callers on the left, callees on the right),
// with the complete lists of callers and callees underneath.
import { store } from '../store';
import type { CallEdge, CallGraph, GraphNode, NodeKind, PathStep } from '../types';
import { emptyState, toast, tooltip } from '../ui';
import { formatCount, formatSize, h, hex, shortName } from '../util';
import { VList } from '../vlist';
import { View } from './base';

const NODE_W = 220;
const NODE_H = 44;
const ROW_GAP = 12;
const COL_GAP = 72;
const PAD = 20;
const HEAD = 26;
const SVG = 'http://www.w3.org/2000/svg';

/** Node colours reuse the region families: functions are code, imports are linking. */
const FAMILY: Record<NodeKind, string> = { function: 'code', import: 'linking', code: 'unknown', data: 'data' };
const KIND_LABEL: Record<NodeKind, string> = {
  function: 'Function',
  import: 'Import: a stub, PLT entry or GOT/IAT slot',
  code: 'Code outside any known function',
  data: 'Data',
};

interface Placed {
  node: GraphNode;
  x: number;
  y: number;
  el: HTMLElement;
}

export class CallsView extends View {
  private graph?: CallGraph;
  /** The centre function's extent, to tell whether a new selection is still inside it. */
  private extent?: [bigint, bigint];
  private up = 1;
  private down = 2;
  private fanout = 8;
  private path: PathStep[] | null = null;
  private seq = 0;
  private toolbar!: HTMLElement;
  private pathBar!: HTMLElement;
  private canvasHost!: HTMLElement;
  private lists!: HTMLElement;
  private callers: CallEdge[] = [];
  private callees: CallEdge[] = [];
  private callerList!: VList;
  private calleeList!: VList;
  private callerHead!: HTMLElement;
  private calleeHead!: HTMLElement;
  private placed = new Map<bigint, Placed>();
  private edgeEls: SVGPathElement[] = [];

  constructor() {
    super('calls', true);
    store.on('xrefs', () => {
      if (this.visible && store.xrefs !== 'building') {
        this.extent = undefined;
        this.onSelection();
      }
    });
    // New names change node labels.
    store.on('annotations', () => {
      if (this.visible && this.graph) void this.load(this.graph.center);
    });
  }

  protected render() {
    this.graph = undefined;
    this.extent = undefined;
    this.path = null;
    this.toolbar = h('div', { class: 'toolbar' });
    this.pathBar = h('div', { class: 'cg-path' });
    this.pathBar.hidden = true;
    this.canvasHost = h('div', { class: 'cg-host' });
    this.callerHead = h('div', { class: 'pane-head cg-list-head' });
    this.calleeHead = h('div', { class: 'pane-head cg-list-head' });
    this.callerList = new VList({ rowHeight: 24, renderRow: (i) => this.edgeRow(this.callers[i], 'caller') });
    this.calleeList = new VList({ rowHeight: 24, renderRow: (i) => this.edgeRow(this.callees[i], 'callee') });
    this.lists = h(
      'div',
      { class: 'cg-lists' },
      h('div', { class: 'cg-list' }, this.callerHead, this.callerList.el),
      h('div', { class: 'cg-list' }, this.calleeHead, this.calleeList.el),
    );
    this.el.replaceChildren(this.toolbar, this.pathBar, this.canvasHost, this.lists);
  }

  protected onSelection() {
    const addr = store.selection.address;
    if (addr === undefined) {
      this.showMessage('Nothing selected', 'Select a function in Code, Symbols or search results to see what calls it and what it calls.');
      return;
    }
    const e = this.extent;
    if (e && addr >= e[0] && addr < e[1]) return;
    void this.load(addr);
  }

  private showMessage(title: string, body: string) {
    this.graph = undefined;
    this.extent = undefined;
    this.toolbar.replaceChildren(h('h3', null, 'Call graph'));
    this.pathBar.hidden = true;
    this.canvasHost.replaceChildren(emptyState(title, body));
    this.lists.hidden = true;
  }

  private async load(addr: bigint) {
    const mine = ++this.seq;
    if (store.xrefs !== 'ready') {
      if (store.xrefs === 'unsupported') {
        this.showMessage('No call graph for this architecture', `Calls and references are found in x86, x86-64 and AArch64 code; this binary is ${store.file?.summary.arch}.`);
        return;
      }
      this.showMessage('Indexing references…', 'Finding every call, data reference and stored pointer, once per file. Large binaries take a few seconds.');
      await store.ensureXrefs();
      return; // The 'xrefs' event loads the graph.
    }
    const [g, callers, callees] = await Promise.all([
      store.api.callGraph(addr, this.up, this.down, this.fanout),
      store.api.callers(addr),
      store.api.callees(addr),
    ]);
    if (mine !== this.seq) return;
    const center = g.nodes.find((n) => n.address === g.center);
    if (!center || center.kind === 'data') {
      this.showMessage('Not in a function', `${hex(addr)} is not inside a known function. Select a function in Code, Symbols or search results.`);
      return;
    }
    this.graph = g;
    this.extent = [g.center, g.center + (center.size > 0n ? center.size : 1n)];
    if (this.path && this.path[this.path.length - 1]?.address !== g.center) this.path = null;
    this.callers = callers;
    this.callees = callees;
    this.renderToolbar(center);
    this.renderPath();
    this.renderGraph();
    this.renderLists();
  }

  // --- Toolbar ------------------------------------------------------------------

  private renderToolbar(center: GraphNode) {
    const select = (label: string, value: number, options: number[], set: (v: number) => void, title: string) => {
      const s = h('select', { class: 'field small', 'aria-label': label, title }, options.map((o) => h('option', { value: String(o), selected: o === value }, String(o))));
      s.addEventListener('change', () => {
        set(Number(s.value));
        if (this.graph) void this.load(this.graph.center);
      });
      return h('label', { class: 'cg-control' }, h('span', null, label), s);
    };
    const from = h('input', { class: 'field small cg-from', type: 'search', placeholder: 'Path from… e.g. main', 'aria-label': 'Find a chain of calls from another function to this one', spellcheck: 'false' });
    from.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') void this.findPath(from.value);
    });
    const code = h('button', { class: 'btn small', type: 'button', title: 'Show the disassembly' }, 'Code');
    code.addEventListener('click', () => void store.select({ address: center.address }, { view: 'code' }));
    this.toolbar.replaceChildren(
      h('h3', { class: 'mono cg-title', title: center.name }, center.name),
      ...(center.kind === 'import' ? [h('span', { class: 'chip' }, 'import')] : []),
      h('span', { class: 'secondary mono' }, `${hex(center.address)}${center.size > 0n ? ' · ' + formatSize(center.size) : ''}`),
      h('span', { class: 'spacer' }),
      select('Callers', this.up, [0, 1, 2, 3], (v) => (this.up = v), 'Levels of callers'),
      select('Callees', this.down, [0, 1, 2, 3], (v) => (this.down = v), 'Levels of callees'),
      select('Each', this.fanout, [4, 8, 12, 20], (v) => (this.fanout = v), 'Neighbours shown per function (the most-called first)'),
      from,
      code,
    );
  }

  private async findPath(query: string) {
    const g = this.graph;
    if (!g || !query.trim()) return;
    try {
      const r = await store.api.resolve(query.trim());
      const start = r.kind === 'address' ? r.value : await store.api.offsetToAddress(r.value);
      if (start === undefined) throw new Error(`${query} is not loaded at an address`);
      const steps = await store.api.callPath(start, g.center, 12);
      if (!steps) {
        toast(`No chain of direct calls from ${r.label || query} reaches this function within 12 calls. It may be called indirectly (callbacks, virtual calls).`, 'info', 8000);
        return;
      }
      this.path = steps;
      this.renderPath();
      this.markPath();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
  }

  private renderPath() {
    const p = this.path;
    this.pathBar.hidden = !p;
    if (!p) return;
    const close = h('button', { class: 'btn ghost small', type: 'button', title: 'Clear the path' }, 'Clear');
    close.addEventListener('click', () => {
      this.path = null;
      this.renderPath();
      this.markPath();
    });
    const steps: Node[] = [];
    p.forEach((s, i) => {
      if (i > 0) steps.push(h('span', { class: 'cg-arrow', title: s.site !== undefined ? `called at ${hex(s.site)}` : '' }, '→'));
      const step = h('span', { class: 'link mono', title: `${s.name}\n${hex(s.address)}` }, shortName(s.name));
      step.addEventListener('click', () => void store.select({ address: s.site ?? s.address }, { view: s.site !== undefined ? 'code' : undefined }));
      steps.push(step);
    });
    this.pathBar.replaceChildren(h('span', { class: 'muted' }, `${p.length - 1} call${p.length === 2 ? '' : 's'}:`), ...steps, h('span', { class: 'spacer' }), close);
  }

  // --- Graph --------------------------------------------------------------------

  private renderGraph() {
    const g = this.graph!;
    const depthOf = new Map(g.nodes.map((n) => [n.address, n.depth]));
    const columns = new Map<number, GraphNode[]>();
    for (const n of g.nodes) {
      const col = columns.get(n.depth) ?? [];
      col.push(n);
      columns.set(n.depth, col);
    }
    const depths = [...columns.keys()].sort((a, b) => a - b);
    const minD = depths[0];
    const maxD = depths[depths.length - 1];
    // Order each column by where its neighbours toward the centre sit (fewer crossings).
    const rank = new Map<bigint, number>([[g.center, 0]]);
    const weight = new Map<string, number>(g.edges.map((e) => [`${e.from}:${e.to}`, e.calls]));
    const orderColumn = (d: number, toward: number) => {
      const col = columns.get(d);
      if (!col) return;
      const key = (n: GraphNode) => {
        let sum = 0;
        let calls = 0;
        let k = 0;
        for (const e of g.edges) {
          const other = d > 0 ? (e.to === n.address ? e.from : undefined) : e.from === n.address ? e.to : undefined;
          if (other === undefined || depthOf.get(other) !== toward) continue;
          sum += rank.get(other) ?? 0;
          calls += weight.get(`${e.from}:${e.to}`) ?? 0;
          k++;
        }
        return { bary: k ? sum / k : 1e9, calls };
      };
      const keyed = col.map((n) => ({ n, ...key(n) }));
      keyed.sort((a, b) => a.bary - b.bary || b.calls - a.calls || (a.n.address < b.n.address ? -1 : 1));
      keyed.forEach((k, i) => rank.set(k.n.address, i));
      columns.set(d, keyed.map((k) => k.n));
    };
    for (let d = 1; d <= maxD; d++) orderColumn(d, d - 1);
    for (let d = -1; d >= minD; d--) orderColumn(d, d + 1);

    const colHeight = (n: number) => n * NODE_H + (n - 1) * ROW_GAP;
    const tallest = Math.max(...depths.map((d) => colHeight(columns.get(d)!.length)));
    const width = PAD * 2 + depths.length * NODE_W + (depths.length - 1) * COL_GAP;
    const height = PAD * 2 + HEAD + tallest;
    const canvas = h('div', { class: 'cg-canvas', style: `width:${width}px;height:${height}px` });
    const svg = document.createElementNS(SVG, 'svg');
    svg.setAttribute('class', 'cg-edges');
    svg.setAttribute('width', String(width));
    svg.setAttribute('height', String(height));
    canvas.appendChild(svg);

    this.placed.clear();
    const colX = (d: number) => PAD + (d - minD) * (NODE_W + COL_GAP);
    const recursive = new Set(g.edges.filter((e) => e.from === e.to).map((e) => e.from));
    for (const d of depths) {
      const col = columns.get(d)!;
      const top = PAD + HEAD + (tallest - colHeight(col.length)) / 2;
      const caption = d === 0 ? '' : d === -1 ? 'Called by' : d === 1 ? 'Calls' : d < 0 ? `Callers, ${-d} levels up` : `${d} levels down`;
      if (caption) canvas.appendChild(h('div', { class: 'cg-caption', style: `left:${colX(d)}px;top:${PAD}px;width:${NODE_W}px` }, caption));
      col.forEach((n, i) => {
        const x = colX(d);
        const y = top + i * (NODE_H + ROW_GAP);
        const el = this.nodeEl(n, g, recursive.has(n.address));
        el.style.left = `${x}px`;
        el.style.top = `${y}px`;
        canvas.appendChild(el);
        this.placed.set(n.address, { node: n, x, y, el });
      });
    }

    this.edgeEls = [];
    for (const e of g.edges) {
      const a = this.placed.get(e.from);
      const b = this.placed.get(e.to);
      if (!a || !b || e.from === e.to) continue;
      const forward = b.node.depth > a.node.depth;
      const y1 = a.y + NODE_H / 2;
      const y2 = b.y + NODE_H / 2;
      let d: string;
      if (forward) {
        const x1 = a.x + NODE_W;
        const x2 = b.x;
        const dx = (x2 - x1) / 2;
        d = `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`;
      } else if (a.x === b.x) {
        // Same column: loop around the right side.
        const x = a.x + NODE_W;
        d = `M${x},${y1} C${x + 44},${y1} ${x + 44},${y2} ${x},${y2}`;
      } else {
        // A call back toward the centre: from the caller's left to the callee's right.
        d = `M${a.x},${y1} C${a.x - 48},${y1} ${b.x + NODE_W + 48},${y2} ${b.x + NODE_W},${y2}`;
      }
      const path = document.createElementNS(SVG, 'path');
      path.setAttribute('d', d);
      path.setAttribute('class', `cg-edge${forward ? '' : ' back'}`);
      path.setAttribute('stroke-width', String(1.25 + Math.min(2.5, Math.log2(e.calls))));
      path.dataset.from = String(e.from);
      path.dataset.to = String(e.to);
      const title = document.createElementNS(SVG, 'title');
      title.textContent = `${a.node.name} → ${b.node.name}: ${e.calls} call site${e.calls === 1 ? '' : 's'}`;
      path.appendChild(title);
      svg.appendChild(path);
      this.edgeEls.push(path);
    }
    if (g.hidden > 0) {
      canvas.appendChild(h('div', { class: 'cg-hidden muted', style: `left:${PAD}px;top:${height - PAD + 2}px` }, `${formatCount(g.hidden)} more not shown: raise "Each", or pick a function from the lists below.`));
    }
    this.canvasHost.replaceChildren(canvas);
    this.lists.hidden = false;
    this.markPath();
    // Bring the centre into view: its callers at the left edge, as many callee levels as fit.
    const c = this.placed.get(g.center);
    if (c) {
      const left = colX(Math.max(minD, -1)) - PAD;
      const fits = width - left <= this.canvasHost.clientWidth;
      this.canvasHost.scrollLeft = fits ? Math.max(0, width - this.canvasHost.clientWidth) : left;
      this.canvasHost.scrollTop = Math.max(0, c.y + NODE_H / 2 - this.canvasHost.clientHeight / 2);
    }
  }

  private nodeEl(n: GraphNode, g: CallGraph, recursive: boolean): HTMLElement {
    const note = store.notesAt.get(n.address);
    const counts: string[] = [];
    if (n.callers !== undefined) counts.push(`↑${formatCount(n.callers)}`);
    if (n.callees !== undefined) counts.push(`↓${formatCount(n.callees)}`);
    const el = h(
      'div',
      {
        class: `cg-node fam-${FAMILY[n.kind]}${n.address === g.center ? ' center' : ''}${n.kind === 'import' ? ' import' : ''}`,
        role: 'button',
        tabindex: '0',
        'aria-label': `${n.name}, ${KIND_LABEL[n.kind]}`,
      },
      h('div', { class: 'n' }, note?.reviewed ? h('span', { class: 'reviewed' }, '✓ ') : null, shortName(n.name)),
      h('div', { class: 'm' }, h('span', null, hex(n.address)), n.size > 0n && n.kind !== 'import' ? h('span', null, formatSize(n.size)) : null, counts.length ? h('span', null, counts.join(' ')) : null, recursive ? h('span', { title: 'Calls itself' }, '↻') : null),
    );
    const open = () => void store.select({ address: n.address }, { origin: 'calls' });
    el.addEventListener('click', open);
    el.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') open();
    });
    el.addEventListener('dblclick', () => void store.select({ address: n.address }, { view: 'code' }));
    el.addEventListener('mouseenter', (e) => {
      this.highlight(n.address, true);
      const counts = [n.callers !== undefined ? `called by ${formatCount(n.callers)}` : '', n.callees !== undefined ? `calls ${formatCount(n.callees)}` : ''].filter(Boolean);
      const lines: HTMLElement[] = [
        h('div', { class: 'mono', style: 'font-weight:600;overflow-wrap:anywhere' }, n.name),
        h('div', { class: 'muted' }, `${KIND_LABEL[n.kind]} · ${hex(n.address)}${n.size > 0n ? ' · ' + formatSize(n.size) : ''}`),
      ];
      if (counts.length) lines.push(h('div', null, counts.join(' · ')));
      if (note?.comment) lines.push(h('div', null, note.comment));
      lines.push(h('div', { class: 'muted' }, n.address === g.center ? 'Double-click for the code' : 'Click to centre on it · double-click for the code'));
      tooltip.show(e.clientX, e.clientY, ...lines);
    });
    el.addEventListener('mouseleave', () => {
      this.highlight(n.address, false);
      tooltip.hide();
    });
    return el;
  }

  private highlight(address: bigint, on: boolean) {
    const key = String(address);
    for (const p of this.edgeEls) {
      if (p.dataset.from === key || p.dataset.to === key) p.classList.toggle('hl', on);
    }
  }

  /** Marks the nodes and edges of the current call path. */
  private markPath() {
    for (const p of this.placed.values()) p.el.classList.remove('on-path');
    for (const e of this.edgeEls) e.classList.remove('on-path');
    const path = this.path;
    if (!path) return;
    for (const s of path) this.placed.get(s.address)?.el.classList.add('on-path');
    for (let i = 1; i < path.length; i++) {
      const from = String(path[i - 1].address);
      const to = String(path[i].address);
      for (const e of this.edgeEls) if (e.dataset.from === from && e.dataset.to === to) e.classList.add('on-path');
    }
  }

  // --- Lists --------------------------------------------------------------------

  private renderLists() {
    const sites = (list: CallEdge[]) => list.reduce((n, e) => n + e.calls, 0);
    const head = (label: string, list: CallEdge[]) => [
      h('strong', null, label),
      h('span', { class: 'muted' }, list.length ? `${formatCount(list.length)} function${list.length === 1 ? '' : 's'} · ${formatCount(sites(list))} call site${sites(list) === 1 ? '' : 's'}` : 'none found'),
    ];
    const indirect = h('span', { class: 'muted', title: 'Calls through registers, vtables and callbacks are only found when they go through an import slot' }, '· direct calls only');
    this.callerHead.replaceChildren(...head('Called by', this.callers));
    this.calleeHead.replaceChildren(...head('Calls', this.callees), indirect);
    this.callerList.setCount(this.callers.length);
    this.calleeList.setCount(this.callees.length);
  }

  private edgeRow(e: CallEdge | undefined, side: 'caller' | 'callee'): HTMLElement {
    if (!e) return h('div', { class: 'list-row' });
    const site = h('span', { class: 'addr link', title: side === 'caller' ? 'The call site, in the caller' : 'The first call, in this function' }, hex(e.site));
    site.addEventListener('click', (ev) => {
      ev.stopPropagation();
      void store.select({ address: e.site }, { view: 'code' });
    });
    const row = h(
      'div',
      { class: 'list-row', title: `${e.name}\n${KIND_LABEL[e.kind]} at ${hex(e.address)}` },
      h('span', { class: 'cg-calls' }, `${formatCount(e.calls)}×`),
      h('span', { class: `cg-dot fam-${FAMILY[e.kind]}` }),
      h('span', { class: 'nm' }, e.name),
      h('span', { class: 'spacer' }),
      site,
    );
    row.addEventListener('click', () => void store.select({ address: e.address }, { origin: 'calls' }));
    return row;
  }
}
