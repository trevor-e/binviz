// Patch: what a patch changes in the open file (an IPS, UPS or BPS file
// dropped on it, or bytes edited in the hex view), each run of changed bytes
// placed in its bank or section, function and region, with its bytes (and
// its text, read with the table file) before and after. From here the
// changes are saved as a patch file, or the patched file downloaded or
// opened in place of the original.
import { store } from '../store';
import type { PatchFormat, PatchRow, PatchState } from '../types';
import { downloadBlob, emptyState, toast } from '../ui';
import { basename, fmtAddr, formatCount, formatSize, h, hex, hexPad, num } from '../util';
import { VList } from '../vlist';
import { View } from './base';
import { FunctionDiffPanel } from './functions-diff';

const FORMATS: { format: PatchFormat; title: string }[] = [
  { format: 'ips', title: 'IPS: what most patchers read (files up to 16 MiB)' },
  { format: 'bps', title: 'BPS: checks it is applied to the right file (CRC-32s)' },
  { format: 'ups', title: 'UPS: CRC-32s too, and applies both ways' },
];

export class PatchView extends View {
  private list?: VList;
  /** The bytes a patch changes, or the functions. */
  private tab: 'bytes' | 'functions' = 'bytes';
  private functions = new FunctionDiffPanel('patch');

  constructor() {
    super('patch', true);
    store.on('patch', () => {
      this.functions.reset();
      if (this.visible) this.render();
    });
    // Text previews are read with the table file.
    store.on('table', () => void store.refreshPatch());
  }

  protected render() {
    const p = store.patch;
    if (!p) {
      this.el.replaceChildren(
        emptyState(
          'No patch',
          'Drop an IPS, UPS or BPS patch on the open file to see what it changes, placed in banks, functions and text. Or edit bytes in the hex view (Edit, then type hex digits): your edits collect here, to save as a patch.',
        ),
      );
      return;
    }
    const toolbar = h(
      'div',
      { class: 'toolbar' },
      h('h3', null, 'Patch'),
      h('span', { class: 'secondary' }, p.name ? basename(p.name) : 'Your edits'),
      h('span', { class: 'muted' }, `${formatCount(p.total)} ${p.total === 1 ? 'change' : 'changes'} · ${formatCount(num(p.differ))} ${p.differ === 1n ? 'byte differs' : 'bytes differ'}`),
      h('span', { class: 'spacer' }),
      ...FORMATS.map(({ format, title }) => this.button(`Save as ${format.toUpperCase()}`, title, () => void this.save(format))),
      this.button('Download patched', 'The patched file', () => void this.download()),
      this.button('Open patched', 'Open the patched file in place of this one (your notes come along)', () => void store.openPatched(), 'primary'),
      store.canUndoEdit() ? this.button('Undo edit', 'Take the last edit back (Ctrl+Z in the hex view)', () => void store.undoEdit()) : null,
      this.button('Discard', 'Drop the patch and your edits', () => void store.clearPatch()),
    );
    const tabs = h('div', { class: 'tabs patch-tabs' });
    for (const [t, label, title] of [
      ['bytes', 'Bytes', 'Each run of bytes it changes, placed in its bank, function and region'],
      ['functions', 'Functions', 'The functions it changes, their instructions before and after (the patched file read as a whole)'],
    ] as const) {
      const b = h('button', { class: `tab${this.tab === t ? ' active' : ''}`, type: 'button', title }, label);
      b.addEventListener('click', () => {
        this.tab = t;
        this.render();
      });
      tabs.appendChild(b);
    }
    const parts: HTMLElement[] = [toolbar];
    if (p.applied) parts.push(this.applied(p));
    parts.push(h('div', { class: 'patch-tabbar' }, tabs));
    if (this.tab === 'bytes') parts.push(this.changes(p));
    else {
      parts.push(this.functions.el);
      void this.functions.show();
    }
    this.el.replaceChildren(...parts);
  }

  private button(label: string, title: string, onClick: () => void, extra = ''): HTMLButtonElement {
    const b = h('button', { class: `btn small ${extra}`, type: 'button', title }, label);
    b.addEventListener('click', onClick);
    return b;
  }

  /** What the patch file said about itself. */
  private applied(p: PatchState): HTMLElement {
    const a = p.applied!;
    const i = a.info;
    const check = (ok: boolean | undefined, yes: string, no: string) => (ok === undefined ? null : h('span', { class: `chip ${ok ? 'ok' : 'warn'}` }, ok ? yes : no));
    const facts: [string, Node | string][] = [
      ['Format', `${i.format.toUpperCase()} · ${formatCount(num(i.records))} ${i.format === 'ips' ? 'records' : i.format === 'ups' ? 'runs' : 'actions'}`],
    ];
    if (i.sourceSize !== undefined) facts.push(['For', `${formatSize(i.sourceSize)}${i.sourceCrc32 !== undefined ? `, CRC-32 ${hexPad(i.sourceCrc32, 8).toUpperCase()}` : ''}`]);
    facts.push(['Makes', `${formatSize(i.targetSize)}${i.targetCrc32 !== undefined ? `, CRC-32 ${hexPad(i.targetCrc32, 8).toUpperCase()}` : ''}${i.truncate !== undefined ? ' (cut to that size)' : ''}`]);
    const checks = h('div', { class: 'btn-row' }, check(a.sourceMatches, 'made for this file', 'made for another file'), check(a.targetMatches, 'result as promised', 'result differs from what it promises'));
    return h(
      'div',
      { class: 'patch-info' },
      h('dl', { class: 'facts' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
      checks,
      i.metadata ? h('details', null, h('summary', { class: 'secondary' }, 'Metadata'), h('pre', { class: 'mono patch-meta' }, i.metadata)) : null,
      ...a.warnings.map((w) => h('div', { class: 'patch-warning' }, w)),
    );
  }

  private changes(p: PatchState): HTMLElement {
    const rows = p.changes;
    const head = h(
      'div',
      { class: 'patch-row patch-head' },
      h('span', { class: 'p-off' }, 'Offset'),
      h('span', { class: 'p-len' }, 'Bytes'),
      h('span', { class: 'p-where' }, 'Where'),
      h('span', { class: 'p-bytes' }, 'Before → after'),
    );
    this.list = new VList({ rowHeight: 40, renderRow: (i) => this.row(rows[i]) });
    const more = p.total > rows.length ? h('div', { class: 'pad muted' }, `The first ${formatCount(rows.length)} of ${formatCount(p.total)} changes.`) : null;
    const box = h('div', { class: 'patch-list' }, head, this.list.el, more);
    this.list.setCount(rows.length);
    return box;
  }

  private row(r: PatchRow): HTMLElement {
    const where: Node[] = [];
    if (r.section) where.push(h('span', { class: 'p-sec' }, r.section));
    if (r.address !== undefined) where.push(h('span', { class: 'mono' }, fmtAddr(r.address)));
    if (r.function) where.push(h('span', { class: 'mono p-fn', title: r.function }, r.function));
    const regions = r.region.filter((x) => x !== r.section);
    const before = [...r.before];
    const after = [...r.after];
    const bytes = (list: number[], other: number[]) =>
      list.slice(0, 12).map((b, i) => h('span', { class: other[i] !== b ? 'p-diff' : '' }, hexPad(b, 2)));
    const text = r.beforeText !== undefined && r.afterText !== undefined && r.beforeText !== r.afterText ? h('div', { class: 'p-text' }, h('span', null, show(r.beforeText)), ' → ', h('span', null, show(r.afterText))) : null;
    const el = h(
      'div',
      { class: `patch-row kind-${r.kind}`, title: 'Show in the hex view' },
      h('span', { class: 'p-off mono' }, hex(r.offset)),
      h('span', { class: 'p-len' }, r.kind === 'changed' ? formatCount(num(r.len)) : `${formatCount(num(r.len))} ${r.kind}`),
      h('div', { class: 'p-where' }, h('div', { class: 'p-line' }, ...(where.length ? where : [h('span', { class: 'muted' }, 'not loaded at an address')])), regions.length ? h('div', { class: 'muted p-region' }, regions.join(' › ')) : null),
      h(
        'div',
        { class: 'p-bytes mono' },
        h(
          'div',
          null,
          ...(r.kind === 'added' ? [h('span', { class: 'muted' }, 'added')] : bytes(before, after)),
          r.kind === 'changed' || r.kind === 'added' ? h('span', { class: 'muted' }, '→') : h('span', { class: 'muted' }, '→ removed'),
          ...(r.kind === 'removed' ? [] : bytes(after, r.kind === 'added' ? [] : before)),
          Math.max(before.length, after.length) < num(r.len) ? h('span', { class: 'muted' }, '…') : null,
        ),
        text,
      ),
    );
    el.addEventListener('click', () => void store.select({ offset: r.offset }, { view: 'hex' }));
    return el;
  }

  private async save(format: PatchFormat) {
    try {
      const blob = await store.patchFile(format);
      const name = basename(store.file?.name ?? 'file').replace(/\.[^.]+$/, '');
      downloadBlob(`${name}.${format}`, blob);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'error');
    }
  }

  private async download() {
    downloadBlob(store.patchedName(), await store.patchedFile());
  }
}

function show(text: string): string {
  const t = text.replace(/\n/g, '⏎');
  return t.length > 40 ? `${t.slice(0, 39)}…` : t;
}
