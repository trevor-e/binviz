// Call graph view: who calls a function and what it calls, a few levels
// each way, in columns (callers on the left, callees on the right), with
// the complete lists of callers and callees underneath.
//
// The graph is centred on the function selected elsewhere (Code, Symbols,
// search). Within it, a click selects a function (the inspector shows it)
// and leaves the graph where it is; a double-click, or "Centre here",
// centres the graph on it. The functions centred on so far make a trail
// back, and each centre is a place Back returns to.
import { store } from '../store';
import type { CallEdge, CallGraph, GraphNode, NodeKind, PathStep } from '../types';
import { emptyState, toast, tooltip } from '../ui';
import { fmtAddr, formatCount, formatSize, h, shortName } from '../util';
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

/** How the graph came to be centred where it is. */
type Via = 'graph' | 'outside';

export class CallsView extends View {
  private graph?: CallGraph;
  /** The centre function's extent, to tell whether a new selection is still inside it. */
  private extent?: [bigint, bigint];
  private up = 1;
  private down = 2;
  private fanout = 8;
  private path: PathStep[] | null = null;
  /** The functions centred on, oldest first: the way back. */
  private trail: { address: bigint; name: string }[] = [];
  private seq = 0;
  private toolbar!: HTMLElement;
  private pathBar!: HTMLElement;
  private canvasHost!: HTMLElement;
  private actions!: HTMLElement;
  private lists!: HTMLElement;
  private callers: CallEdge[] = [];
  private callees: CallEdge[] = [];
  private callerList!: VList;
  private calleeList!: VList;
  private callerHead!: HTMLElement;
  private calleeHead!: HTMLElement;
  private placed = new Map<bigint, Placed>();
  private edgeEls: SVGPathElement[] = [];
  /** The function selected in the graph or the lists (not necessarily the centre). */
  private picked?: bigint;

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
      if (this.visible && this.graph) void this.load(this.graph.center, 'graph');
    });
  }

  protected render() {
    this.graph = undefined;
    this.extent = undefined;
    this.path = null;
    this.trail = [];
    this.picked = undefined;
    this.toolbar = h('div', { class: 'toolbar cg-toolbar' });
    this.pathBar = h('div', { class: 'cg-path' });
    this.pathBar.hidden = true;
    this.canvasHost = h('div', { class: 'cg-host' });
    this.actions = h('div', { class: 'cg-actions' });
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
    const sel = store.selection;
    // Picked in the graph or the lists: shown, the graph stays.
    if (sel.origin === 'calls') {
      this.markPicked();
      return;
    }
    const addr = sel.address;
    if (addr === undefined) {
      if (this.graph) this.setNote('The selection has no address: the graph stays on the last function.');
      else this.showMessage('Nothing selected', 'Select a function in Code, Symbols or search results to see what calls it and what it calls.');
      return;
    }
    const e = this.extent;
    if (e && addr >= e[0] && addr < e[1]) {
      this.picked = this.graph?.center;
      this.markPicked();
      return;
    }
    void this.load(addr, 'outside');
  }

  private showMessage(title: string, body: string) {
    this.graph = undefined;
    this.extent = undefined;
    this.toolbar.replaceChildren(h('h3', null, 'Call graph'));
    this.pathBar.hidden = true;
    this.canvasHost.replaceChildren(emptyState(title, body));
    this.lists.hidden = true;
  }

  /** A line under the toolbar about why the graph is what it is. */
  private setNote(text: string) {
    this.toolbar.querySelector('.cg-note')?.remove();
    if (text) this.toolbar.appendChild(h('div', { class: 'cg-note muted' }, text));
  }

  /** Centres the graph on a function picked in it: a new place in history. */
  private centre(address: bigint) {
    this.picked = address;
    void this.load(address, 'graph');
    void store.select({ address }, { origin: 'calls', history: 'push' });
  }

  private async load(addr: bigint, via: Via) {
    const mine = ++this.seq;
    if (store.xrefs !== 'ready') {
      if (store.xrefs === 'unsupported') {
        this.showMessage('No call graph for this architecture', `Calls and references are found in x86, x86-64, AArch64 and ARM code and in the consoles' CPUs; this binary is ${store.file?.summary.arch}.`);
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
      if (this.graph) {
        this.setNote(`${fmtAddr(addr)} isn’t in a function: the graph stays on ${shortName(this.graphCentreName())}.`);
        return;
      }
      this.showMessage('Not in a function', `${fmtAddr(addr)} is not inside a known function. Select a function in Code, Symbols or search results.`);
      return;
    }
    // The trail: back to a centre on it, one more for a function in the graph, a fresh one for elsewhere.
    const at = this.trail.findIndex((t) => t.address === g.center);
    if (at >= 0) this.trail = this.trail.slice(0, at + 1);
    else if (via === 'graph' || this.placed.has(g.center)) this.trail = [...this.trail, { address: g.center, name: center.name }].slice(-8);
    else this.trail = [{ address: g.center, name: center.name }];
    this.graph = g;
    this.extent = [g.center, g.center + (center.size > 0n ? center.size : 1n)];
    if (via === 'outside') this.picked = g.center;
    this.callers = callers;
    this.callees = callees;
    this.renderToolbar(center);
    this.renderPath();
    this.renderGraph();
    this.renderLists();
    this.markPicked();
  }

  private graphCentreName(): string {
    const g = this.graph;
    return g?.nodes.find((n) => n.address === g.center)?.name ?? '';
  }

  // --- Toolbar ------------------------------------------------------------------

  private renderToolbar(center: GraphNode) {
    const select = (label: string, value: number, options: number[], set: (v: number) => void, title: string) => {
      const s = h('select', { class: 'field small', 'aria-label': label, title }, options.map((o) => h('option', { value: String(o), selected: o === value }, String(o))));
      s.addEventListener('change', () => {
        set(Number(s.value));
        if (this.graph) void this.load(this.graph.center, 'graph');
      });
      return h('label', { class: 'cg-control' }, h('span', null, label), s);
    };
    const from = h('input', { class: 'field small cg-from', type: 'search', placeholder: 'Path from… e.g. main', 'aria-label': 'Find a chain of calls from another function to this one', spellcheck: 'false' });
    from.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') void this.findPath(from.value);
    });
    const code = h('button', { class: 'btn small', type: 'button', title: 'Show its disassembly' }, 'Code');
    code.addEventListener('click', () => void store.select({ address: center.address }, { view: 'code' }));
    // The trail of centres: each one a way back.
    const crumbs: Node[] = [];
    this.trail.forEach((t, i) => {
      if (i > 0) crumbs.push(h('span', { class: 'cg-crumb-sep', 'aria-hidden': 'true' }, '›'));
      const last = i === this.trail.length - 1;
      const el = h(last ? 'strong' : 'button', { class: last ? 'cg-crumb current mono' : 'cg-crumb link mono', type: last ? undefined : 'button', title: last ? `${t.name}\nThe graph is centred on it` : `${t.name}\nCentre the graph on it again` }, shortName(t.name));
      if (!last) el.addEventListener('click', () => this.centre(t.address));
      crumbs.push(el);
    });
    this.toolbar.replaceChildren(
      h('span', { class: 'cg-label' }, 'Centred on'),
      h('div', { class: 'cg-crumbs' }, ...crumbs),
      ...(center.kind === 'import' ? [h('span', { class: 'chip' }, 'import')] : []),
      h('span', { class: 'secondary mono' }, `${fmtAddr(center.address)}${center.size > 0n ? ' · ' + formatSize(center.size) : ''}`),
      code,
      h('span', { class: 'spacer' }),
      select('Callers', this.up, [0, 1, 2, 3], (v) => (this.up = v), 'Levels of callers'),
      select('Callees', this.down, [0, 1, 2, 3], (v) => (this.down = v), 'Levels of callees'),
      select('Each', this.fanout, [4, 8, 12, 20], (v) => (this.fanout = v), 'Neighbours shown per function (the most-called first)'),
      from,
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
      if (i > 0) {
        const arrow = h('span', { class: `cg-arrow${s.site !== undefined ? ' link' : ''}`, title: s.site !== undefined ? `The call, at ${fmtAddr(s.site)}: show its code` : '' }, '→');
        if (s.site !== undefined) arrow.addEventListener('click', () => void store.select({ address: s.site! }, { view: 'code' }));
        steps.push(arrow);
      }
      const step = h('span', { class: 'link mono', title: `${s.name}\n${fmtAddr(s.address)}\nCentre the graph on it` }, shortName(s.name));
      step.addEventListener('click', () => this.centre(s.address));
      steps.push(step);
    });
    this.pathBar.replaceChildren(h('span', { class: 'muted' }, `${p.length - 1} call${p.length === 2 ? '' : 's'} from ${shortName(p[0].name)}:`), ...steps, h('span', { class: 'spacer' }), close);
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
    // Room under the lowest node for the picked function's actions.
    const height = PAD * 2 + HEAD + tallest + 34;
    const canvas = h('div', { class: 'cg-canvas', style: `width:${width}px;height:${height}px` });
    canvas.addEventListener('click', (e) => {
      // A click on the background drops the pick back to the centre.
      if (e.target === canvas) {
        this.picked = g.center;
        void store.select({ address: g.center }, { origin: 'calls', history: 'none' });
      }
    });
    const svg = document.createElementNS(SVG, 'svg');
    svg.setAttribute('class', 'cg-edges');
    svg.setAttribute('width', String(width));
    svg.setAttribute('height', String(height));
    // Arrowheads: which way each call goes.
    const defs = document.createElementNS(SVG, 'defs');
    for (const [id, cls] of [
      ['cg-head', 'cg-head'],
      ['cg-head-hl', 'cg-head hl'],
      ['cg-head-path', 'cg-head on-path'],
    ]) {
      const marker = document.createElementNS(SVG, 'marker');
      marker.setAttribute('id', id);
      marker.setAttribute('viewBox', '0 0 8 8');
      marker.setAttribute('refX', '7');
      marker.setAttribute('refY', '4');
      marker.setAttribute('markerWidth', '7');
      marker.setAttribute('markerHeight', '7');
      marker.setAttribute('markerUnits', 'userSpaceOnUse');
      marker.setAttribute('orient', 'auto');
      const tip = document.createElementNS(SVG, 'path');
      tip.setAttribute('d', 'M0,0 L8,4 L0,8 z');
      tip.setAttribute('class', cls);
      marker.appendChild(tip);
      defs.appendChild(marker);
    }
    svg.appendChild(defs);
    canvas.appendChild(svg);

    this.placed.clear();
    const colX = (d: number) => PAD + (d - minD) * (NODE_W + COL_GAP);
    const recursive = new Set(g.edges.filter((e) => e.from === e.to).map((e) => e.from));
    for (const d of depths) {
      const col = columns.get(d)!;
      const top = PAD + HEAD + (tallest - colHeight(col.length)) / 2;
      const caption = d === 0 ? 'Centred on' : d === -1 ? '← Called by' : d === 1 ? 'Calls →' : d < 0 ? `← Callers, ${-d} levels up` : `${d} levels down →`;
      canvas.appendChild(h('div', { class: `cg-caption${d === 0 ? ' centre' : ''}`, style: `left:${colX(d)}px;top:${PAD}px;width:${NODE_W}px` }, caption));
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
      path.setAttribute('marker-end', 'url(#cg-head)');
      path.dataset.from = String(e.from);
      path.dataset.to = String(e.to);
      const title = document.createElementNS(SVG, 'title');
      title.textContent = `${a.node.name} calls ${b.node.name}: ${e.calls} call site${e.calls === 1 ? '' : 's'}`;
      path.appendChild(title);
      svg.appendChild(path);
      this.edgeEls.push(path);
    }
    if (g.hidden > 0) {
      canvas.appendChild(h('div', { class: 'cg-hidden muted', style: `left:${PAD}px;top:${height - PAD + 2}px` }, `${formatCount(g.hidden)} more not shown: raise "Each", or pick a function from the lists below.`));
    }
    canvas.appendChild(this.actions);
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
    const centre = n.address === g.center;
    const el = h(
      'div',
      {
        class: `cg-node fam-${FAMILY[n.kind]}${centre ? ' center' : ''}${n.kind === 'import' ? ' import' : ''}`,
        role: 'button',
        tabindex: '0',
        'aria-label': `${n.name}, ${KIND_LABEL[n.kind]}${centre ? ', the centre' : ''}`,
      },
      h('div', { class: 'n' }, note?.reviewed ? h('span', { class: 'reviewed' }, '✓ ') : null, shortName(n.name)),
      h('div', { class: 'm' }, h('span', null, fmtAddr(n.address)), n.size > 0n && n.kind !== 'import' ? h('span', null, formatSize(n.size)) : null, counts.length ? h('span', null, counts.join(' ')) : null, recursive ? h('span', { title: 'Calls itself' }, '↻') : null),
    );
    el.addEventListener('click', () => this.pick(n.address));
    el.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this.centre(n.address);
      else if (e.key === ' ') {
        e.preventDefault();
        this.pick(n.address);
      }
    });
    el.addEventListener('dblclick', () => !centre && this.centre(n.address));
    el.addEventListener('mouseenter', (e) => {
      this.highlight(n.address, true);
      const counts = [n.callers !== undefined ? `called by ${formatCount(n.callers)}` : '', n.callees !== undefined ? `calls ${formatCount(n.callees)}` : ''].filter(Boolean);
      const lines: HTMLElement[] = [
        h('div', { class: 'mono', style: 'font-weight:600;overflow-wrap:anywhere' }, n.name),
        h('div', { class: 'muted' }, `${KIND_LABEL[n.kind]} · ${fmtAddr(n.address)}${n.size > 0n ? ' · ' + formatSize(n.size) : ''}`),
      ];
      if (counts.length) lines.push(h('div', null, counts.join(' · ')));
      if (note?.comment) lines.push(h('div', null, note.comment));
      lines.push(h('div', { class: 'muted' }, centre ? 'The graph is centred on it' : 'Click to select it · double-click to centre the graph on it'));
      tooltip.show(e.clientX, e.clientY, ...lines);
    });
    el.addEventListener('mouseleave', () => {
      this.highlight(n.address, false);
      tooltip.hide();
    });
    return el;
  }

  /** Selects a function without moving the graph: the inspector shows it. */
  private pick(address: bigint) {
    this.picked = address;
    this.markPicked();
    void store.select({ address }, { origin: 'calls', history: 'none' });
  }

  /** Marks the picked function, with its actions under it. */
  private markPicked() {
    const g = this.graph;
    for (const p of this.placed.values()) p.el.classList.toggle('picked', p.node.address === this.picked);
    this.callerList?.refresh();
    this.calleeList?.refresh();
    if (!g || this.picked === undefined) {
      this.actions.hidden = true;
      return;
    }
    const at = this.placed.get(this.picked);
    const node = at?.node ?? [...this.callers, ...this.callees].find((e) => e.address === this.picked);
    if (!at || !node || this.picked === g.center) {
      this.actions.hidden = true;
      return;
    }
    const address = this.picked;
    const centre = h('button', { class: 'btn tiny primary', type: 'button', title: 'Centre the graph on it (double-click does too)' }, 'Centre here');
    centre.addEventListener('click', (e) => {
      e.stopPropagation();
      this.centre(address);
    });
    const code = h('button', { class: 'btn tiny', type: 'button', title: 'Show its disassembly' }, 'Code');
    code.addEventListener('click', (e) => {
      e.stopPropagation();
      void store.select({ address }, { view: 'code' });
    });
    this.actions.replaceChildren(centre, code);
    this.actions.style.left = `${at.x}px`;
    this.actions.style.top = `${at.y + NODE_H + 4}px`;
    this.actions.hidden = false;
  }

  private highlight(address: bigint, on: boolean) {
    const key = String(address);
    for (const p of this.edgeEls) {
      if (p.dataset.from === key || p.dataset.to === key) {
        p.classList.toggle('hl', on);
        this.setHead(p);
      }
    }
  }

  private setHead(p: SVGPathElement) {
    p.setAttribute('marker-end', p.classList.contains('on-path') ? 'url(#cg-head-path)' : p.classList.contains('hl') ? 'url(#cg-head-hl)' : 'url(#cg-head)');
  }

  /** Marks the nodes and edges of the current call path. */
  private markPath() {
    for (const p of this.placed.values()) p.el.classList.remove('on-path');
    for (const e of this.edgeEls) e.classList.remove('on-path');
    const path = this.path;
    if (path) {
      for (const s of path) this.placed.get(s.address)?.el.classList.add('on-path');
      for (let i = 1; i < path.length; i++) {
        const from = String(path[i - 1].address);
        const to = String(path[i].address);
        for (const e of this.edgeEls) if (e.dataset.from === from && e.dataset.to === to) e.classList.add('on-path');
      }
    }
    for (const e of this.edgeEls) this.setHead(e);
  }

  // --- Lists --------------------------------------------------------------------

  private renderLists() {
    const sites = (list: CallEdge[]) => list.reduce((n, e) => n + e.calls, 0);
    const head = (label: string, list: CallEdge[]) => [
      h('strong', null, label),
      h('span', { class: 'muted' }, list.length ? `${formatCount(list.length)} function${list.length === 1 ? '' : 's'} · ${formatCount(sites(list))} call site${sites(list) === 1 ? '' : 's'}` : 'none found'),
    ];
    const indirect = h('span', { class: 'muted', title: 'Calls through registers, vtables and callbacks are only found when they go through an import slot' }, '· direct calls only');
    const name = shortName(this.graphCentreName());
    this.callerHead.replaceChildren(...head(`Called by (what calls ${name})`, this.callers));
    this.calleeHead.replaceChildren(...head(`Calls (what ${name} calls)`, this.callees), indirect);
    this.callerList.setCount(this.callers.length);
    this.calleeList.setCount(this.callees.length);
  }

  private edgeRow(e: CallEdge | undefined, side: 'caller' | 'callee'): HTMLElement {
    if (!e) return h('div', { class: 'list-row' });
    const site = h('span', { class: 'addr link', title: side === 'caller' ? 'The call, in the caller: show its code' : 'The first call to it: show its code' }, fmtAddr(e.site));
    site.addEventListener('click', (ev) => {
      ev.stopPropagation();
      void store.select({ address: e.site }, { view: 'code' });
    });
    const centre = h('button', { class: 'btn tiny cg-row-centre', type: 'button', title: 'Centre the graph on it' }, 'Centre');
    centre.addEventListener('click', (ev) => {
      ev.stopPropagation();
      this.centre(e.address);
    });
    const row = h(
      'div',
      { class: `list-row${e.address === this.picked ? ' selected' : ''}`, title: `${e.name}\n${KIND_LABEL[e.kind]} at ${fmtAddr(e.address)}\nClick to select it · double-click to centre the graph on it` },
      h('span', { class: 'cg-calls' }, `${formatCount(e.calls)}×`),
      h('span', { class: `cg-dot fam-${FAMILY[e.kind]}` }),
      h('span', { class: 'nm' }, e.name),
      h('span', { class: 'spacer' }),
      centre,
      site,
    );
    row.addEventListener('click', () => this.pick(e.address));
    row.addEventListener('dblclick', () => this.centre(e.address));
    return row;
  }
}
