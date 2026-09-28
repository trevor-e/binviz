// Compare: what changed in size since an earlier build — files, kinds of
// content, each binary, and inside binaries their sections, owners and symbols.
import { KIND_LABELS as REGION_LABELS } from '../colors';
import { store } from '../store';
import type { CategoryChange, FileCategory, FolderDiff, GroupKind, OwnerChange, SizeDiff } from '../types';
import { formatCount, formatSize, h, num } from '../util';
import { View } from './base';

const CATEGORY_LABELS: Record<FileCategory, string> = {
  binaries: 'Binaries',
  'asset-catalogs': 'Asset catalogs',
  images: 'Images',
  interface: 'Nibs & storyboards',
  localization: 'Localization',
  fonts: 'Fonts',
  media: 'Audio & video',
  'ml-models': 'ML models',
  web: 'Web content',
  data: 'Data & config',
  'developer-files': 'Headers & modules',
  'code-signature': 'Code signature',
  'debug-symbols': 'Debug symbols',
  other: 'Other',
};

const GROUP_LABELS: Record<GroupKind, string> = {
  'swift-module': 'Swift module',
  'objc-class': 'Objective-C class',
  namespace: 'C++ namespace / Rust crate',
  'c-prefix': 'C prefix',
  'compiler-generated': 'Compiler-generated',
  unnamed: 'Unnamed functions',
  other: 'Other',
};

/** A row of a change table: what, before, after. */
interface Row {
  label: string | HTMLElement;
  detail?: string;
  old: bigint;
  new: bigint;
  mono?: boolean;
}

export class DiffView extends View {
  private fileInput = h('input', { type: 'file', style: 'display:none' });
  private folderInput = h('input', { type: 'file', multiple: true, webkitdirectory: true, style: 'display:none' });

  constructor() {
    super('diff');
    store.on('diff', () => this.invalidate());
    this.fileInput.addEventListener('change', () => {
      const f = this.fileInput.files?.[0];
      this.fileInput.value = '';
      if (f) void compareWithFile(f);
    });
    this.folderInput.addEventListener('change', () => {
      const files = [...(this.folderInput.files ?? [])].map((file) => ({ path: (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name, file }));
      this.folderInput.value = '';
      if (files.length === 0) return;
      const name = files[0].path.split('/')[0] || 'folder';
      void store.compareWith(name, { kind: 'folder', sources: [{ kind: 'folder', name, files }] });
    });
  }

  get visible(): boolean {
    return store.view === 'diff' && (store.file !== null || store.package !== null);
  }

  protected render() {
    const d = store.diff;
    const page = h('div', { class: 'page' }, this.fileInput, this.folderInput);
    if (!d) {
      page.append(this.pick());
    } else if (!d.result) {
      page.append(h('div', { class: 'card' }, h('h2', null, `Comparing with ${d.baseline}`), h('p', { class: 'sub' }, 'Reading both builds…')));
    } else {
      if (d.stale && !d.busy) void store.refreshDiff();
      const r = d.result;
      if (r.kind === 'folder') this.folder(page, r.diff);
      else this.binary(page, r.diff, true);
    }
    this.el.replaceChildren(page);
  }

  private pick(): HTMLElement {
    const file = h('button', { class: 'btn primary', type: 'button' }, 'Choose a file');
    file.addEventListener('click', () => this.fileInput.click());
    const folder = h('button', { class: 'btn', type: 'button' }, 'Choose a folder');
    folder.addEventListener('click', () => this.folderInput.click());
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Compare with an earlier build'),
      h(
        'p',
        { class: 'secondary' },
        'See what grew and what shrank: files and kinds of content, each binary, and inside the binaries their sections, owners (Swift modules, Objective-C classes, namespaces…) and symbols. Choose the earlier build (a binary, a zip or .ipa, or a folder) or drop it here.',
      ),
      h('div', { class: 'dropzone-actions', style: 'justify-content:flex-start;margin-top:12px' }, file, folder),
    );
  }

  private actions(): HTMLElement {
    const again = h('button', { class: 'btn small', type: 'button' }, 'Compare with another build');
    again.addEventListener('click', () => {
      store.stopComparing();
    });
    return h('div', { class: 'dropzone-actions', style: 'justify-content:flex-start' }, again);
  }

  private header(title: string, oldSize: bigint, newSize: bigint, tiles: HTMLElement[], note?: string): HTMLElement {
    return h(
      'div',
      { class: 'card' },
      h('div', { class: 'card-head' }, h('div', { style: 'flex:1' }, h('h2', { style: 'font-size:18px' }, title), note ? h('p', { class: 'sub' }, note) : null), this.actions()),
      h('div', { class: 'tiles' }, tile('Size', formatSize(newSize), `${signed(oldSize, newSize)} · ${pct(oldSize, newSize)} from ${formatSize(oldSize)}`), tiles),
    );
  }

  private folder(page: HTMLElement, d: FolderDiff) {
    const changed = d.binaries.filter((b) => b.old !== b.new).length;
    page.append(
      this.header(`${d.oldName} → ${d.newName}`, d.oldSize, d.newSize, [
        tile('Files', formatCount(d.filesAdded + d.filesRemoved + d.filesChanged), `${d.filesAdded} added · ${d.filesRemoved} removed · ${d.filesChanged} changed`),
        tile('Binaries', formatCount(changed), `of ${d.binaries.length} changed size`),
        tile('Owners', formatCount(d.ownersChanged), 'changed size, across the binaries'),
      ], 'Debug files aside'),
    );
    const cats = d.categories.filter((c) => c.old !== c.new);
    page.append(
      h(
        'div',
        { class: 'grid-2' },
        changeCard('Kinds of content', 'Files by what they are', cats.map((c: CategoryChange) => ({ label: CATEGORY_LABELS[c.category], old: c.old, new: c.new }))),
        changeCard(
          'Binaries',
          'Each binary, then and now',
          d.binaries.map((b) => ({ label: b.name, detail: b.path, old: b.old, new: b.new })),
        ),
      ),
      ownerCard('Owners across the binaries', d.owners, d.ownersChanged),
      changeCard('Files', 'The biggest changes', d.files.map((f) => ({ label: f.name, old: f.old, new: f.new, mono: true }))),
    );
    for (const b of d.binaries) {
      if (!b.diff || (b.old === b.new && b.diff.symbols.length === 0)) continue;
      const box = h('details', { class: 'card diff-binary' }, h('summary', null, h('span', { class: 'diff-binary-title' }, b.name), h('span', { class: `diff-delta ${sign(b.old, b.new)}` }, ` ${signed(b.old, b.new)}`), h('span', { class: 'muted' }, `  ${b.path}`)));
      this.binary(box, b.diff, false);
      page.append(box);
    }
  }

  private binary(into: HTMLElement, d: SizeDiff, top: boolean) {
    if (top) {
      into.append(
        this.header(`${d.oldName} → ${d.newName}`, d.oldSize, d.newSize, [
          tile('Symbols', formatCount(d.symbolsAdded + d.symbolsRemoved + d.symbolsChanged), `${d.symbolsAdded} added · ${d.symbolsRemoved} removed · ${d.symbolsChanged} changed`),
          tile('Owners', formatCount(d.ownersChanged), 'changed size'),
        ]),
      );
    }
    const kinds = d.byKind.filter((k) => k.old !== k.new);
    into.append(
      h(
        'div',
        { class: 'grid-2' },
        changeCard('Kinds of bytes', 'What the bytes that changed are', kinds.map((k) => ({ label: REGION_LABELS[k.kind], old: k.old, new: k.new }))),
        changeCard('Sections', 'Sections that changed size', d.sections.map((s) => ({ label: s.name, old: s.old, new: s.new, mono: true }))),
      ),
      ownerCard('Owners', d.owners, d.ownersChanged),
      changeCard(
        'Symbols',
        `${d.symbolsAdded} added, ${d.symbolsRemoved} removed, ${d.symbolsChanged} changed size; the biggest changes`,
        d.symbols.map((s) => ({ label: s.name, detail: s.code ? undefined : 'data', old: s.old, new: s.new, mono: true })),
      ),
    );
  }
}

async function compareWithFile(f: File) {
  const head = new Uint8Array(await f.slice(0, 4).arrayBuffer());
  const zip = head[0] === 0x50 && head[1] === 0x4b;
  await store.compareWith(f.name, zip ? { kind: 'folder', sources: [{ kind: 'zip', name: f.name, blob: f }] } : { kind: 'file', name: f.name, blob: f });
}

function tile(label: string, value: string, sub: string): HTMLElement {
  return h('div', { class: 'tile' }, h('div', { class: 'tile-label' }, label), h('div', { class: 'tile-value' }, value), h('div', { class: 'tile-sub' }, sub));
}

function sign(old: bigint, now: bigint): string {
  return now > old ? 'grew' : now < old ? 'shrank' : 'same';
}

function signed(old: bigint, now: bigint): string {
  if (now === old) return 'no change';
  return now > old ? `+${formatSize(now - old)}` : `−${formatSize(old - now)}`;
}

function pct(old: bigint, now: bigint): string {
  if (old === 0n) return now === 0n ? '0%' : 'new';
  const p = ((num(now) - num(old)) * 100) / num(old);
  return `${p > 0 ? '+' : p < 0 ? '−' : ''}${Math.abs(p).toFixed(1)}%`;
}

function thenNow(old: bigint, now: bigint): string {
  if (old === 0n) return 'added';
  if (now === 0n) return 'removed';
  return '';
}

/** A diverging bar: growth to the right, shrinking to the left, of the largest change in the table. */
function deltaBar(old: bigint, now: bigint, max: number): HTMLElement {
  const d = num(now) - num(old);
  const w = `${Math.max(1, (Math.abs(d) / Math.max(1, max)) * 100).toFixed(1)}%`;
  return h(
    'div',
    { class: 'delta-bar', role: 'img', 'aria-label': signed(old, now) },
    h('div', { class: 'delta-neg' }, d < 0 ? h('span', { style: `width:${w}` }) : null),
    h('div', { class: 'delta-pos' }, d > 0 ? h('span', { style: `width:${w}` }) : null),
  );
}

function changeCard(title: string, sub: string, rows: Row[]): HTMLElement {
  const max = Math.max(1, ...rows.map((r) => Math.abs(num(r.new) - num(r.old))));
  const body = rows.length
    ? h(
        'div',
        { class: 'table-scroll' },
        h(
          'table',
          { class: 'data diff-table' },
          h('thead', null, h('tr', null, h('th', null, ''), h('th', { class: 'right' }, 'Before'), h('th', { class: 'right' }, 'After'), h('th', { class: 'right' }, 'Change'), h('th', null, ''))),
          h(
            'tbody',
            null,
            rows.map((r) =>
              h(
                'tr',
                null,
                h('td', { class: `diff-name${r.mono ? ' mono' : ''}`, title: typeof r.label === 'string' ? r.label : undefined }, r.label, r.detail ? h('div', { class: 'diff-detail' }, r.detail) : null),
                h('td', { class: 'right muted' }, r.old === 0n ? '—' : formatSize(r.old)),
                h('td', { class: 'right' }, r.new === 0n ? '—' : formatSize(r.new)),
                h('td', { class: `right diff-delta ${sign(r.old, r.new)}` }, signed(r.old, r.new), thenNow(r.old, r.new) ? h('span', { class: 'muted' }, ` ${thenNow(r.old, r.new)}`) : null),
                h('td', { class: 'bar-cell' }, deltaBar(r.old, r.new, max)),
              ),
            ),
          ),
        ),
      )
    : h('p', { class: 'secondary' }, 'No change.');
  return h('div', { class: 'card' }, h('h2', null, title), h('p', { class: 'sub' }, sub), body);
}

function ownerCard(title: string, owners: OwnerChange[], changed: number): HTMLElement {
  return changeCard(
    title,
    `${formatCount(changed)} changed size${owners.length < changed ? `; the ${owners.length} biggest changes` : ''}`,
    owners.map((o) => ({ label: o.name, detail: GROUP_LABELS[o.kind], old: o.old, new: o.new })),
  );
}
