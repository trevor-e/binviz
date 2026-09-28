// Functions compared across two versions (an earlier build and the open
// binary, or the open file and its patched copy), the way BinDiff does:
// which function is which, whether identical, relocated or changed, which
// were added or removed, and a pair's instructions lined up side by side.
import { store } from '../store';
import type { DiffLine, FnInfo, FunctionDiff, FunctionPair, Instruction } from '../types';
import { emptyState, toast } from '../ui';
import { fmtAddr, formatCount, formatSize, h } from '../util';
import { VList } from '../vlist';

type Filter = 'changed' | 'relocated' | 'added' | 'removed';

/** One row of the list: a pair, or a function on one side only. */
type Item = { pair: FunctionPair } | { only: FnInfo; side: 'added' | 'removed' };

const HOW: Record<FunctionPair['how'], string> = {
  name: 'by name',
  bytes: 'by identical bytes',
  instructions: 'by the same instructions',
  calls: 'through the call graph',
  address: 'by address',
};

export class FunctionDiffPanel {
  readonly el = h('div', { class: 'fd' });
  private diff: FunctionDiff | null = null;
  private filter: Filter = 'changed';
  private items: Item[] = [];
  private list?: VList;
  private detail = h('div', { class: 'fd-detail' });
  private picked = -1;
  private loading = false;

  /**
   * `side`: `baseline` compares the earlier build (old) with the open binary
   * (new); `patch` the open file (old) with its patched copy (new).
   */
  constructor(private readonly side: 'baseline' | 'patch') {}

  /** Forgets the comparison (something changed on either side). */
  reset() {
    this.diff = null;
    this.picked = -1;
  }

  async show() {
    if (this.diff || this.loading) return this.render();
    this.loading = true;
    this.el.replaceChildren(h('div', { class: 'pad muted' }, 'Comparing every function… (the first time takes a moment on large binaries)'));
    try {
      this.diff = await store.api.functionDiff(this.side);
    } catch (e) {
      this.el.replaceChildren(emptyState('Functions can’t be compared', e instanceof Error ? e.message : String(e)));
      return;
    } finally {
      this.loading = false;
    }
    if (this.diff.changed === 0 && this.diff.added.length + this.diff.removed.length > 0) this.filter = 'added';
    if (this.diff.changed === 0 && this.diff.added.length === 0 && this.diff.removed.length > 0) this.filter = 'removed';
    this.render();
  }

  private render() {
    const d = this.diff;
    if (!d) return;
    const counts: Record<Filter, number> = { changed: d.changed, relocated: d.relocated, added: d.added.length, removed: d.removed.length };
    const tabs = h('div', { class: 'tabs' });
    for (const f of ['changed', 'relocated', 'added', 'removed'] as const) {
      const b = h('button', { class: `tab${f === this.filter ? ' active' : ''}`, type: 'button', title: TITLES[f] }, `${f[0].toUpperCase()}${f.slice(1)} ${formatCount(counts[f])}`);
      b.addEventListener('click', () => {
        this.filter = f;
        this.picked = -1;
        this.render();
      });
      tabs.appendChild(b);
    }
    this.items =
      this.filter === 'added'
        ? d.added.map((f) => ({ only: f, side: 'added' as const }))
        : this.filter === 'removed'
          ? d.removed.map((f) => ({ only: f, side: 'removed' as const }))
          : d.pairs.filter((p) => p.status === this.filter).map((pair) => ({ pair }));
    this.list = new VList({ rowHeight: 40, renderRow: (i) => this.row(i) });
    const summary = h('span', { class: 'muted' }, `${formatCount(d.identical)} identical`);
    this.el.replaceChildren(
      h('div', { class: 'fd-head' }, tabs, h('span', { class: 'spacer' }), summary),
      h('div', { class: 'fd-split' }, h('div', { class: 'fd-list' }, this.items.length ? this.list.el : h('div', { class: 'pad muted' }, EMPTY[this.filter])), this.detail),
    );
    this.list.setCount(this.items.length);
    if (this.picked < 0 && this.items.length > 0 && this.filter !== 'added' && this.filter !== 'removed') this.pick(0);
    else if (this.picked < 0) this.detail.replaceChildren(h('div', { class: 'pad muted' }, 'Pick a function to see its instructions.'));
  }

  private row(i: number): HTMLElement {
    const it = this.items[i];
    let el: HTMLElement;
    if ('pair' in it) {
      const p = it.pair;
      const renamed = p.old.name !== p.new.name;
      const delta = Number(p.new.size) - Number(p.old.size);
      el = h(
        'div',
        { class: `fd-row${i === this.picked ? ' selected' : ''}`, title: `Matched ${HOW[p.how]}` },
        h('div', { class: 'fd-top' }, h('span', { class: 'fd-name mono' }, renamed ? `${p.old.name} → ${p.new.name}` : p.new.name), h('span', { class: 'fd-sim' }, p.status === 'changed' ? `${Math.round(p.similarity * 100)}%` : 'moved')),
        h(
          'div',
          { class: 'fd-sub muted' },
          `${fmtAddr(p.old.address)} → ${fmtAddr(p.new.address)} · ${delta === 0 ? 'same size' : `${delta > 0 ? '+' : ''}${delta} bytes`} · ${HOW[p.how]}`,
        ),
        h('div', { class: 'fd-bar' }, h('span', { style: `width:${Math.round(p.similarity * 100)}%` })),
      );
    } else {
      const f = it.only;
      el = h(
        'div',
        { class: `fd-row${i === this.picked ? ' selected' : ''}` },
        h('div', { class: 'fd-top' }, h('span', { class: 'fd-name mono' }, f.name), h('span', { class: 'fd-sim' }, formatSize(f.size))),
        h('div', { class: 'fd-sub muted' }, `${fmtAddr(f.address)} · ${f.instructions ?? '?'} instructions · only in the ${it.side === 'added' ? 'newer' : 'older'} version`),
      );
    }
    el.addEventListener('click', () => this.pick(i));
    return el;
  }

  private async pick(i: number) {
    this.picked = i;
    this.list?.refresh();
    const it = this.items[i];
    if (!it) return;
    if (!('pair' in it)) {
      // A function on one side only: show it where it lives.
      const f = it.only;
      const here = (it.side === 'added') === (this.side === 'baseline');
      this.detail.replaceChildren(
        h(
          'div',
          { class: 'pad' },
          h('h3', { class: 'mono' }, f.name),
          h('p', { class: 'secondary' }, `Only in the ${it.side === 'added' ? 'newer' : 'older'} version: ${formatSize(f.size)} at ${fmtAddr(f.address)}.`),
          here ? link('Show its code', () => void store.select({ address: f.address }, { view: 'code' })) : h('span', { class: 'muted' }, 'It is in the other file.'),
        ),
      );
      return;
    }
    const p = it.pair;
    this.detail.replaceChildren(h('div', { class: 'pad muted' }, 'Lining the instructions up…'));
    let lines: DiffLine[];
    try {
      lines = await store.api.functionCode(this.side, p.old.address, p.new.address);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
      return;
    }
    if (this.picked !== i) return;
    // The side that is the open binary, to jump to its code.
    const open = this.side === 'baseline' ? 'new' : 'old';
    const cell = (ins: Instruction | undefined, side: 'old' | 'new') => {
      if (!ins) return h('span', { class: 'fd-cell empty' });
      const el = h('span', { class: `fd-cell${side === open ? ' link' : ''}`, title: side === open ? 'Show in the code view' : '' }, h('span', { class: 'fd-addr' }, fmtAddr(ins.address)), h('span', { class: 'fd-mn' }, ins.mnemonic), ' ', h('span', { class: 'fd-ops' }, ins.targetSymbol ? `${ins.operands}  <${ins.targetSymbol}>` : ins.operands));
      if (side === open) el.addEventListener('click', () => void store.select({ address: ins.address }, { view: 'code' }));
      return el;
    };
    const rows = lines.map((l) => h('div', { class: `fd-line ${l.kind}` }, cell(l.old, 'old'), cell(l.new, 'new')));
    const changedLines = lines.filter((l) => l.kind !== 'same').length;
    this.detail.replaceChildren(
      h(
        'div',
        { class: 'fd-code-head' },
        h('div', null, h('b', { class: 'mono' }, p.old.name), h('span', { class: 'muted' }, ` ${fmtAddr(p.old.address)} · older`)),
        h('div', null, h('b', { class: 'mono' }, p.new.name), h('span', { class: 'muted' }, ` ${fmtAddr(p.new.address)} · newer`)),
      ),
      h('div', { class: 'fd-code-sub muted' }, `${formatCount(changedLines)} of ${formatCount(lines.length)} lines differ · ${p.status === 'changed' ? `${Math.round(p.similarity * 100)}% similar` : 'only addresses differ'} · matched ${HOW[p.how]}`),
      h('div', { class: 'fd-code mono' }, ...rows),
    );
  }
}

const TITLES: Record<Filter, string> = {
  changed: 'Matched, with other instructions or constants',
  relocated: 'Matched, the same instructions at other addresses (or calling moved code)',
  added: 'Only in the newer version',
  removed: 'Only in the older version',
};

const EMPTY: Record<Filter, string> = {
  changed: 'No function changed.',
  relocated: 'No function only moved.',
  added: 'No function was added.',
  removed: 'No function was removed.',
};

function link(label: string, onClick: () => void): HTMLElement {
  const b = h('button', { class: 'btn small', type: 'button' }, label);
  b.addEventListener('click', onClick);
  return b;
}
