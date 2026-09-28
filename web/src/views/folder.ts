// Folder: every binary in a folder or zip at once (an .ipa, an .xcarchive, a
// build…): the binaries paired with their debug files, what the other files
// are, and where the code in all the binaries comes from, combined.
import { analysisBytes, store, type OpenPackage } from '../store';
import type { BinaryKind, BuildId, FileCategory, FileRef, GroupKind, PackageInfo, SizeReport } from '../types';
import { basename, formatCount, formatSize, h, num, percent } from '../util';
import { View } from './base';

export const KIND_LABELS: Record<BinaryKind, string> = {
  executable: 'Executable',
  library: 'Library',
  plugin: 'Plug-in',
  other: 'Binary',
  object: 'Object file',
  debug: 'Debug file',
};

/** The same, as headings for a list of them. */
export const KIND_GROUPS: Record<BinaryKind, string> = {
  executable: 'Executables',
  library: 'Libraries',
  plugin: 'Plug-ins',
  other: 'Other binaries',
  object: 'Object files',
  debug: 'Debug files',
};

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
  unnamed: 'Unnamed functions',
  other: 'Other',
};

/** An owner of code and data (a Swift module, a class…) summed over the folder's binaries. */
interface Owner {
  name: string;
  kind: GroupKind;
  bytes: number;
  /** Binary index and bytes, largest first. */
  binaries: { index: number; bytes: number }[];
}

export class FolderView extends View {
  private allFiles = false;
  private allOwners = false;

  constructor() {
    super('folder');
    store.on('package', () => this.invalidate());
  }

  get visible(): boolean {
    return store.view === 'folder' && store.package !== null;
  }

  protected render() {
    const p = store.package;
    if (!p) {
      this.el.replaceChildren();
      return;
    }
    const page = h('div', { class: 'page' });
    page.append(this.identity(p.info), this.binaries(p));
    if (p.reports) page.append(this.owners(p));
    // Nothing but debug files: no contents to break down.
    const contents = p.info.size > 0n;
    if (contents) page.append(h('div', { class: 'grid-2' }, this.contents(p.info), this.largest(p.info)));
    page.append(h('div', { class: 'grid-2' }, this.debugFiles(p.info), contents ? this.duplicates(p.info) : null));
    this.el.replaceChildren(page);
  }

  private identity(info: PackageInfo): HTMLElement {
    const app = info.binaries[0]?.bundle;
    const files = info.categories.filter((c) => c.category !== 'debug-symbols').reduce((a, c) => a + c.files, 0);
    const kinds = new Map<BinaryKind, number>();
    for (const b of info.binaries) kinds.set(b.kind, (kinds.get(b.kind) ?? 0) + 1);
    const paired = info.binaries.filter((b) => b.debug !== undefined && b.debug !== null).length;
    const zipped = info.compressedSize !== undefined && info.compressedSize !== null;
    const debugOnly = info.binaries.length > 0 && info.binaries.every((b) => b.kind === 'debug');
    const tiles = debugOnly
      ? [tile('Debug files', formatCount(info.binaries.length), formatSize(info.debugSize))]
      : [
          tile('Size', formatSize(info.size), `${formatCount(files)} files, debug files aside`),
          tile('Binaries', formatCount(info.binaries.length), [...kinds].map(([k, n]) => `${n} ${(n === 1 ? KIND_LABELS : KIND_GROUPS)[k].toLowerCase()}`).join(' · ') || 'none found'),
          tile('Debug files', `${paired} of ${info.binaries.length}`, info.debugFiles.length ? `binaries paired with one · ${formatSize(info.debugSize)}` : 'no separate debug files here'),
        ];
    if (zipped) tiles.splice(1, 0, tile('Zipped', formatSize(info.compressedSize!), `${percent(num(info.compressedSize!), num(info.size) + num(info.debugSize))} of the unzipped size`));
    if ((zipped || info.duplicates.length > 0) && !debugOnly) {
      const sets = info.duplicates.length;
      tiles.push(tile('Duplicates', formatSize(info.duplicateBytes), sets ? `wasted on ${formatCount(sets)} set${sets === 1 ? '' : 's'} of identical files` : 'no identical files'));
    }
    const facts: [string, string][] = [];
    if (app?.bundleId) facts.push(['Bundle ID', app.bundleId]);
    if (app?.version || app?.build) facts.push(['Version', [app.version, app.build && `(${app.build})`].filter(Boolean).join(' ')]);
    if (app?.minOs) facts.push(['Minimum OS', app.minOs]);
    if (app?.platforms.length) facts.push(['Platforms', app.platforms.join(', ')]);
    facts.push(['Contents', `${info.kind === 'zip' ? 'Zip archive' : 'Folder'} · ${formatCount(info.files)} files`]);
    return h(
      'div',
      { class: 'card' },
      h('h2', { style: 'font-size:18px' }, app?.name ?? app?.executable ?? basename(info.name)),
      h('p', { class: 'sub' }, info.name),
      h('div', { class: 'tiles' }, tiles),
      h('dl', { class: 'facts' }, facts.flatMap(([k, v]) => [h('dt', null, k), h('dd', null, v)])),
    );
  }

  private binaries(p: OpenPackage): HTMLElement {
    const info = p.info;
    const reports = p.reports;
    const within = commonDir(info.binaries.map((b) => b.path));
    const head = h(
      'tr',
      null,
      h('th', null, 'Binary'),
      h('th', null, 'Kind'),
      h('th', null, 'Format'),
      h('th', { class: 'right' }, 'Size'),
      h('th', null, 'Debug file'),
      reports ? [h('th', { class: 'right' }, 'Code'), h('th', null, 'Largest owner')] : null,
    );
    const rows = info.binaries.map((b) => {
      const current = b.index === p.current && !!store.file;
      const d = b.debug !== undefined && b.debug !== null ? info.debugFiles[b.debug] : undefined;
      const rel = b.path.slice(within.length);
      const tr = h(
        'tr',
        { class: `clickable${current ? ' selected' : ''}`, title: current ? 'Open in the other views' : 'Open this binary in the other views' },
        h('td', null, h('div', { class: 'pkg-name' }, b.name, current ? h('span', { class: 'chip' }, 'open') : null), rel !== b.name ? h('div', { class: 'pkg-path mono', title: b.path }, rel) : null),
        h('td', null, KIND_LABELS[b.kind]),
        h('td', { title: idsTitle(b.ids) }, `${b.format} ${b.ids.map((x) => x.arch).join(', ')}`),
        h('td', { class: 'right', title: b.compressedSize !== undefined && b.compressedSize !== null ? `${formatSize(b.compressedSize)} zipped` : undefined }, formatSize(b.size)),
        h('td', null, b.kind === 'debug' ? h('span', { class: 'muted' }, '—') : d ? h('span', { class: 'status good', title: d.path }, 'paired') : h('span', { class: 'status none' }, 'none')),
        reports ? reportCells(reports.get(b.index)) : null,
      );
      tr.addEventListener('click', () => void store.selectBinary(b.index, 'overview'));
      return tr;
    });
    let action: HTMLElement | null = null;
    if (!reports && info.binaries.length > 0) {
      action = h('button', { class: 'btn small', type: 'button', title: `Reads every binary and its debug file (${formatSize(analysisBytes(info))})` }, p.analyzing ? 'Analyzing…' : 'Analyze all binaries');
      if (p.analyzing) action.setAttribute('disabled', '');
      action.addEventListener('click', () => void store.analyzePackage());
    }
    const debugOnly = info.binaries.every((b) => b.kind === 'debug');
    return h(
      'div',
      { class: 'card' },
      h(
        'div',
        { class: 'card-head' },
        h(
          'div',
          { style: 'flex:1' },
          h('h2', null, 'Binaries'),
          h(
            'p',
            { class: 'sub' },
            debugOnly
              ? 'Only debug files here, each explored as a binary of its own. '
              : 'Every binary here, paired with its debug file by UUID or build ID. ',
            'Pick one to explore it in the other views.',
            within ? h('span', { class: 'mono' }, ` In ${within}`) : null,
          ),
        ),
        action,
      ),
      info.binaries.length
        ? h('div', { class: 'table-scroll' }, h('table', { class: 'data pkg-binaries' }, h('thead', null, head), h('tbody', null, rows)))
        : h('p', { class: 'secondary' }, 'No binaries were found here.'),
    );
  }

  private owners(p: OpenPackage): HTMLElement {
    const owners = combineOwners(p.reports!);
    const shown = this.allOwners ? owners.slice(0, 200) : owners.slice(0, 25);
    const max = owners[0]?.bytes ?? 1;
    const name = (i: number) => p.info.binaries[i]?.name ?? `#${i}`;
    const rows = shown.map((o) =>
      h(
        'tr',
        null,
        h('td', { class: 'pkg-owner', title: o.name }, o.name),
        h('td', { class: 'secondary' }, GROUP_LABELS[o.kind]),
        h('td', { class: 'right' }, formatSize(o.bytes)),
        h('td', { class: 'bar-cell' }, bar(o.bytes / max)),
        h(
          'td',
          { class: 'pkg-spread' },
          o.binaries.slice(0, 4).map((b, k) => {
            const link = h('span', { class: 'link', title: `Open ${name(b.index)}` }, name(b.index));
            link.addEventListener('click', () => void store.selectBinary(b.index, 'overview'));
            return [k > 0 ? ' · ' : null, link, h('span', { class: 'muted' }, ` ${formatSize(b.bytes)}`)];
          }),
          o.binaries.length > 4 ? h('span', { class: 'muted' }, ` · ${o.binaries.length - 4} more`) : null,
        ),
      ),
    );
    const more = owners.length > 25 ? h('button', { class: 'btn small', type: 'button' }, this.allOwners ? 'Show fewer' : `Show ${Math.min(200, owners.length) - 25} more`) : null;
    more?.addEventListener('click', () => {
      this.allOwners = !this.allOwners;
      this.invalidate();
    });
    const binaries = p.reports!.size;
    const paired = p.info.binaries.some((b) => b.debug !== undefined && b.debug !== null);
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Where the code comes from'),
      h('p', { class: 'sub' }, `Swift modules, Objective-C classes, namespaces and C prefixes, summed over ${binaries} binar${binaries === 1 ? 'y' : 'ies'}${paired ? ' (with their debug files)' : ''}`),
      owners.length
        ? h('div', { class: 'table-scroll' }, h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Owner'), h('th', null, 'Kind'), h('th', { class: 'right' }, 'Code + data'), h('th', null, ''), h('th', null, 'In'))), h('tbody', null, rows)))
        : h('p', { class: 'secondary' }, 'No named code: the binaries are stripped and have no debug files.'),
      more,
    );
  }

  private contents(info: PackageInfo): HTMLElement {
    const cats = info.categories.filter((c) => c.category !== 'debug-symbols');
    const total = num(info.size);
    const max = Math.max(1, ...cats.map((c) => num(c.size)));
    const zipped = info.compressedSize !== undefined && info.compressedSize !== null;
    const rows = cats.map((c) =>
      h(
        'tr',
        null,
        h('td', null, CATEGORY_LABELS[c.category]),
        h('td', { class: 'right muted' }, formatCount(c.files)),
        h('td', { class: 'right' }, formatSize(c.size)),
        h('td', { class: 'bar-cell' }, bar(num(c.size) / max)),
        h('td', { class: 'right muted' }, percent(num(c.size), total)),
        zipped ? h('td', { class: 'right muted' }, c.compressedSize !== undefined && c.compressedSize !== null ? formatSize(c.compressedSize) : '') : null,
      ),
    );
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'What the files are'),
      h('p', { class: 'sub' }, 'By kind of content, debug files aside'),
      h(
        'table',
        { class: 'data' },
        h('thead', null, h('tr', null, h('th', null, 'Content'), h('th', { class: 'right' }, 'Files'), h('th', { class: 'right' }, 'Size'), h('th', null, ''), h('th', { class: 'right' }, 'Share'), zipped ? h('th', { class: 'right' }, 'Zipped') : null)),
        h('tbody', null, rows),
      ),
    );
  }

  private largest(info: PackageInfo): HTMLElement {
    const list = this.allFiles ? info.largest : info.largest.slice(0, 12);
    const max = Math.max(1, num(info.largest[0]?.size ?? 1n));
    const within = commonDir(info.largest.map((f) => f.path));
    const binaryOf = new Map(info.binaries.map((b) => [b.file, b]));
    const rows = list.map((f: FileRef) => {
      const b = binaryOf.get(f.file);
      const tr = h(
        'tr',
        { class: b ? 'clickable' : '' },
        h('td', { class: 'pkg-file mono', title: f.path }, f.path.slice(within.length)),
        h('td', { class: 'secondary' }, CATEGORY_LABELS[f.category]),
        h('td', { class: 'right' }, formatSize(f.size)),
        h('td', { class: 'bar-cell' }, bar(num(f.size) / max)),
      );
      if (b) tr.addEventListener('click', () => void store.selectBinary(b.index, 'overview'));
      return tr;
    });
    const more = info.largest.length > 12 ? h('button', { class: 'btn small', type: 'button' }, this.allFiles ? 'Show fewer' : `Show ${info.largest.length - 12} more`) : null;
    more?.addEventListener('click', () => {
      this.allFiles = !this.allFiles;
      this.invalidate();
    });
    return h(
      'div',
      { class: 'card' },
      h('h2', null, 'Largest files'),
      h('p', { class: 'sub' }, 'Binaries open when clicked', within ? h('span', { class: 'mono' }, ` · in ${within}`) : null),
      h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'File'), h('th', null, 'Content'), h('th', { class: 'right' }, 'Size'), h('th', null, ''))), h('tbody', null, rows)),
      more,
    );
  }

  private debugFiles(info: PackageInfo): HTMLElement {
    const body: HTMLElement[] = [];
    if (info.debugFiles.length === 0) {
      body.push(
        h(
          'p',
          { class: 'secondary' },
          info.binaries.length > 0 && info.binaries.every((b) => b.kind === 'debug')
            ? 'Drop the binaries they belong to with them to pair them.'
            : 'None here. Drop them onto this page (dSYMs, .debug files, or a zip of them, as App Store Connect gives them) and each pairs with its binary.',
        ),
      );
    } else {
      const rows = info.debugFiles.map((d) => {
        const b = d.binary !== undefined && d.binary !== null ? info.binaries[d.binary] : undefined;
        const tr = h(
          'tr',
          { class: b ? 'clickable' : '' },
          h('td', { class: 'pkg-file mono', title: d.path }, debugName(d.path)),
          h('td', { title: idsTitle(d.ids) }, d.ids.map((x) => x.arch).join(', ') || '—'),
          h('td', { class: 'right' }, formatSize(d.size)),
          h('td', null, b ? h('span', { class: 'status good' }, b.name) : h('span', { class: 'status warn', title: 'No binary here has its UUID or build ID, or names it' }, 'no match')),
        );
        if (b) tr.addEventListener('click', () => void store.selectBinary(b.index, 'overview'));
        return tr;
      });
      body.push(h('table', { class: 'data' }, h('thead', null, h('tr', null, h('th', null, 'Debug file'), h('th', null, 'Architectures'), h('th', { class: 'right' }, 'Size'), h('th', null, 'Pairs with'))), h('tbody', null, rows)));
    }
    return h('div', { class: 'card' }, h('h2', null, 'Debug files'), h('p', { class: 'sub' }, 'They pair with binaries by UUID or build ID (or the debug link an ELF file names), whatever their names'), body);
  }

  private duplicates(info: PackageInfo): HTMLElement {
    const zipped = info.compressedSize !== undefined && info.compressedSize !== null;
    let body: HTMLElement;
    if (!zipped && info.duplicates.length === 0) {
      body = h('p', { class: 'secondary' }, 'Identical files are found from a zip’s checksums: open it zipped to see them.');
    } else if (info.duplicates.length === 0) {
      body = h('p', { class: 'secondary' }, 'No two files have the same contents.');
    } else {
      const within = commonDir(info.duplicates.flatMap((d) => d.paths));
      body = h(
        'div',
        { class: 'pkg-dups' },
        info.duplicates.slice(0, 20).map((d) =>
          h(
            'div',
            { class: 'pkg-dup' },
            h('div', { class: 'pkg-dup-head' }, h('strong', null, formatSize(d.wasted)), h('span', { class: 'muted' }, ` wasted · ${d.paths.length} copies of ${formatSize(d.size)}`)),
            d.paths.slice(0, 6).map((p) => h('div', { class: 'pkg-file mono', title: p }, p.slice(within.length))),
            d.paths.length > 6 ? h('div', { class: 'muted' }, `and ${d.paths.length - 6} more`) : null,
          ),
        ),
      );
    }
    return h('div', { class: 'card' }, h('h2', null, 'Duplicate files'), h('p', { class: 'sub' }, info.duplicateBytes > 0n ? `${formatSize(info.duplicateBytes)} could be saved by keeping one copy of each` : 'Files with the same contents'), body);
  }
}

function tile(label: string, value: string, sub: string, tone = ''): HTMLElement {
  return h('div', { class: `tile ${tone}` }, h('div', { class: 'tile-label' }, label), h('div', { class: 'tile-value' }, value), h('div', { class: 'tile-sub' }, sub));
}

/** A thin magnitude bar, `fraction` of the cell's width. */
function bar(fraction: number): HTMLElement {
  return h('div', { class: 'size-bar' }, h('span', { style: `width:${Math.max(0.5, Math.min(1, fraction) * 100).toFixed(1)}%` }));
}

function idsTitle(ids: BuildId[]): string {
  return ids.map((x) => (x.id ? `${x.arch}  ${x.id}` : x.arch)).join('\n');
}

function reportCells(r: SizeReport | undefined): HTMLElement[] {
  if (!r) return [h('td', { class: 'right muted' }, '—'), h('td', { class: 'muted' }, 'couldn’t be read')];
  const code = r.byKind.filter(([k]) => k === 'code').reduce((a, [, n]) => a + num(n), 0);
  const top = r.groups[0];
  return [
    h('td', { class: 'right' }, formatSize(code)),
    h('td', { class: 'pkg-owner', title: top ? `${GROUP_LABELS[top.kind]} · ${formatSize(num(top.codeBytes) + num(top.dataBytes))}` : undefined }, top ? top.name : '—'),
  ];
}

/** Sums owners over the binaries' size reports, largest first. */
function combineOwners(reports: Map<number, SizeReport>): Owner[] {
  const by = new Map<string, Owner>();
  for (const [index, r] of reports) {
    for (const g of r.groups) {
      const bytes = num(g.codeBytes) + num(g.dataBytes);
      if (bytes === 0) continue;
      const key = `${g.kind}\u0000${g.name}`;
      let o = by.get(key);
      if (!o) by.set(key, (o = { name: g.name, kind: g.kind, bytes: 0, binaries: [] }));
      o.bytes += bytes;
      o.binaries.push({ index, bytes });
    }
  }
  const out = [...by.values()];
  for (const o of out) o.binaries.sort((a, b) => b.bytes - a.bytes);
  out.sort((a, b) => b.bytes - a.bytes || a.name.localeCompare(b.name));
  return out;
}

/** The folder all `paths` are in (`Payload/Shop.app/`), to show them from there. */
function commonDir(paths: string[]): string {
  if (paths.length === 0) return '';
  let dir = paths[0].slice(0, paths[0].lastIndexOf('/') + 1);
  for (const p of paths) {
    while (dir && !p.startsWith(dir)) dir = dir.slice(0, dir.lastIndexOf('/', dir.length - 2) + 1);
  }
  return dir;
}

/** A debug file's name: a dSYM's bundle (`Shop.app.dSYM`), or the file's. */
function debugName(path: string): string {
  return path.split('/').find((c) => /\.dsym$/i.test(c)) ?? basename(path);
}
