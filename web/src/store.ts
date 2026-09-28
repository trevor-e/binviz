// Application state: the open binary, the current selection, navigation
// history and loaded source files. Views subscribe to changes.
import { Api } from './api';
import { parseAnnotations, serializeAnnotations } from './notes';
import type {
  Annotation, ContainerInfo, DwarfSummary, Inspection, Opened, PackageInfo, PackageSource, RefCounts, RegionKind, Section,
  Segment, SizeReport, SourceFile, Summary,
} from './types';
import { basename } from './util';

export type ViewName = 'folder' | 'overview' | 'layout' | 'hex' | 'code' | 'calls' | 'symbols' | 'sections' | 'dwarf' | 'sources' | 'map';
export type MapTab = 'files' | 'units' | 'coverage';

export interface Loaded {
  name: string;
  summary: Summary;
  /** The file's bytes, read on demand (a File stays on disk); null when only the worker has them. */
  blob: Blob | null;
  sections: Section[];
  segments: Segment[];
  dwarf: DwarfSummary | null;
  sourceFiles: SourceFile[];
  composition: [RegionKind, bigint][];
}

/** A folder or zip of binaries (an .ipa, a build…): its binaries, one of which is open. */
export interface OpenPackage {
  info: PackageInfo;
  /** The binary the other views show. */
  current: number;
  /** Every binary's size report, once the folder has been analyzed. */
  reports: Map<number, SizeReport> | null;
  analyzing: boolean;
  /** Where its files came from (debug files dropped later join them). */
  sources: PackageSource[];
}

/** Folders with at most this many binaries, and less than this in binaries and debug files, are analyzed as soon as they open. */
const AUTO_ANALYZE_COUNT = 50;
const AUTO_ANALYZE_BYTES = 256 * 1024 * 1024;

/** Bytes read to analyze every binary of a folder: each binary and its debug file. */
export function analysisBytes(info: PackageInfo): number {
  return info.binaries.reduce((a, b) => a + Number(b.size) + (b.debug !== undefined && b.debug !== null ? Number(info.debugFiles[b.debug]?.size ?? 0n) : 0), 0);
}

const buildIdKey = (id: string) => id.replace(/-/g, '').toLowerCase();

export interface Selection {
  offset?: bigint;
  address?: bigint;
  inspection?: Inspection;
  /** The view that made the selection; it doesn't need to scroll to it. */
  origin?: ViewName;
}

export interface Target {
  offset?: bigint;
  address?: bigint;
}

interface HistoryEntry {
  view: ViewName;
  target: Target;
}

type Events = {
  file: [];
  container: [];
  selection: [];
  view: [];
  sources: [];
  intent: [];
  annotations: [];
  xrefs: [];
  package: [];
  error: [string];
  status: [string];
};

/** A request for a view to reveal something specific when it is next shown. */
export interface Intent {
  die?: { unit: number; offset: bigint };
  source?: { file: number; line: number };
  region?: bigint;
  map?: { tab: MapTab; name?: string };
}

class Store {
  readonly api = new Api();
  file: Loaded | null = null;
  container: { name: string; info: ContainerInfo; blob: Blob } | null = null;
  package: OpenPackage | null = null;
  selection: Selection = {};
  view: ViewName = 'overview';
  intent: Intent = {};
  /** Source text by DWARF source file id. */
  sources = new Map<number, string>();
  sourceOrigin = new Map<number, string>();
  /** The user's annotations, sorted by address. */
  annotations: Annotation[] = [];
  /** Annotations by start address, for inline display. */
  notesAt = new Map<bigint, Annotation>();
  /** The cross-reference index: built on demand (right away for small files). */
  xrefs: 'none' | 'building' | 'ready' | 'unsupported' = 'none';
  xrefCounts: RefCounts | null = null;
  private xrefsBuild: Promise<boolean> | null = null;
  private history: HistoryEntry[] = [];
  private cursor = -1;
  private seq = 0;
  private listeners = new Map<keyof Events, Set<(...a: unknown[]) => void>>();

  on<K extends keyof Events>(event: K, fn: (...a: Events[K]) => void) {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
    }
    set.add(fn as (...a: unknown[]) => void);
  }

  emit<K extends keyof Events>(event: K, ...args: Events[K]) {
    for (const fn of this.listeners.get(event) ?? []) fn(...args);
  }

  error(e: unknown) {
    this.emit('error', e instanceof Error ? e.message : String(e));
  }

  async open(name: string, blob: Blob) {
    this.emit('status', `Opening ${name}…`);
    try {
      // Drop our references to the old file first so its memory can go.
      this.file = null;
      this.container = null;
      this.dropPackage();
      const opened = await this.api.open(name, blob);
      if (opened.kind === 'container') {
        this.container = { name, info: opened.info, blob };
        this.emit('container');
        this.emit('status', '');
        return;
      }
      await this.loaded(opened.name, opened.summary, blob);
    } catch (e) {
      this.emit('status', '');
      this.error(e);
    }
  }

  async openMember(index: number) {
    const c = this.container;
    const member = c?.info.members.find((m) => m.index === index);
    if (!c || !member) return;
    try {
      const opened = await this.api.openMember(index);
      const blob = c.blob.slice(Number(member.offset), Number(member.offset + member.size));
      if (opened.kind === 'binary') await this.loaded(opened.name, opened.summary, blob);
    } catch (e) {
      this.error(e);
    }
  }

  private dropPackage() {
    if (!this.package) return;
    this.package = null;
    if (this.view === 'folder') this.view = 'overview';
    this.emit('package');
  }

  /**
   * Opens folders and zips (several at once combine; zips inside open too):
   * every binary found, each paired with its debug file by UUID or build ID.
   * When all that is found is debug files, they go with what is open: they
   * join the open folder, or the one matching the open binary is attached.
   * Returns false when nothing in them is a binary.
   */
  async openFolder(sources: PackageSource[]): Promise<boolean> {
    const name = sources.map((s) => s.name).join(' + ');
    this.emit('status', `Looking through ${name}…`);
    try {
      const info = await this.api.scanFolder(sources);
      if (info.binaries.length === 0) {
        this.emit('status', '');
        return false;
      }
      const debugOnly = info.binaries.every((b) => b.kind === 'debug');
      if (debugOnly && this.package) {
        const p = this.package;
        const merged = [...p.sources, ...sources];
        await this.api.scanFolder(merged);
        await this.openScanned(merged, { path: p.info.binaries[p.current]?.path, view: this.view });
      } else if (debugOnly && this.file) {
        await this.attachMatching(info);
      } else {
        await this.openScanned(sources);
      }
    } catch (e) {
      this.emit('status', '');
      this.error(e);
    }
    return true;
  }

  /** Opens what the worker last scanned; `keep` reopens a binary (by path) in a view. */
  private async openScanned(sources: PackageSource[], keep?: { path?: string; view: ViewName }) {
    this.file = null;
    this.container = null;
    this.dropPackage();
    const { info, opened } = await this.api.openScanned();
    this.package = { info, current: 0, reports: null, analyzing: false, sources };
    this.view = 'folder';
    this.emit('package');
    if (opened?.kind === 'binary') {
      await this.loaded(opened.name, opened.summary, await this.api.packageFileBlob(info.binaries[0].file));
    } else {
      this.emit('status', '');
      this.emit('file');
      this.emit('view');
    }
    const again = keep?.path !== undefined ? info.binaries.findIndex((b) => b.path === keep.path) : -1;
    if (again > 0) await this.selectBinary(again);
    if (keep) this.setView(keep.view);
    // Small folders are combined right away; big ones when asked.
    const n = info.binaries.length;
    if (n > 0 && n <= AUTO_ANALYZE_COUNT && analysisBytes(info) < AUTO_ANALYZE_BYTES) void this.analyzePackage();
  }

  /** Attaches the one of the scanned debug files that matches the open binary. */
  private async attachMatching(info: PackageInfo) {
    const f = this.file!;
    const want = f.summary.buildId ? buildIdKey(f.summary.buildId) : '';
    const match =
      info.binaries.find((b) => want && b.ids.some((x) => x.id && buildIdKey(x.id) === want)) ??
      (info.binaries.length === 1 ? info.binaries[0] : undefined);
    if (!match) {
      throw new Error(`None of the ${info.binaries.length} debug files matches ${basename(f.name)}${f.summary.buildId ? ` (${f.summary.buildId})` : ''}`);
    }
    await this.attached(await this.api.attachScanned(match.file, match.path));
    this.emit('status', `Attached ${match.path}`);
  }

  /** Opens another binary of the folder (loading it and its debug file the first time). */
  async selectBinary(index: number, view?: ViewName) {
    const p = this.package;
    if (!p) return;
    if (index !== p.current || !this.file) {
      try {
        const { info, opened } = await this.api.selectPackageBinary(index);
        if (this.package !== p) return;
        p.info = info;
        p.current = index;
        this.emit('package');
        if (opened.kind === 'binary') await this.loaded(opened.name, opened.summary, await this.api.packageFileBlob(info.binaries[index].file));
      } catch (e) {
        this.error(e);
        return;
      }
    }
    if (view) this.setView(view);
  }

  /** Size reports for every binary of the folder, each with its debug file attached. */
  async analyzePackage(top = 500) {
    const p = this.package;
    if (!p || p.analyzing) return;
    p.analyzing = true;
    this.emit('package');
    try {
      const { info, reports } = await this.api.analyzePackage(top);
      if (this.package !== p) return;
      p.info = info;
      p.reports = new Map(reports.map((r) => [r.index, r.report]));
    } catch (e) {
      this.error(e);
    } finally {
      p.analyzing = false;
      if (this.package === p) this.emit('package');
    }
  }

  /** Bytes `[start, end)` of the open file, from its Blob or (with none) from the worker. */
  async readBytes(start: number, end: number): Promise<Uint8Array> {
    const f = this.file;
    if (!f) return new Uint8Array();
    if (f.blob) return new Uint8Array(await f.blob.slice(start, Math.min(f.blob.size, end)).arrayBuffer());
    return this.api.read(BigInt(start), Math.max(0, Math.min(end, Number(f.summary.fileSize)) - start));
  }

  private async loaded(name: string, summary: Summary, blob: Blob | null) {
    const [sections, segments, dwarf, sourceFiles, composition] = await Promise.all([
      this.api.sections(),
      this.api.segments(),
      this.api.dwarfSummary(),
      this.api.sourceFiles(),
      this.api.composition(),
    ]);
    this.file = { name, summary, blob, sections, segments, dwarf, sourceFiles, composition };
    this.annotations = [];
    this.notesAt.clear();
    const saved = loadNotes(summary.fingerprint);
    if (saved.length > 0) {
      try {
        this.file.summary = await this.api.setAnnotations(saved);
        this.indexNotes(saved);
      } catch (e) {
        this.error(e);
      }
    }
    this.selection = {};
    this.history = [];
    this.cursor = -1;
    this.sources.clear();
    this.sourceOrigin.clear();
    this.resetXrefs();
    this.emit('status', '');
    this.emit('file');
    this.emit('sources');
    const entry = summary.entry;
    if (entry !== undefined) void this.select({ address: entry }, { history: true });
    else void this.select({ offset: 0n }, { history: true });
    // Small files are indexed right away; large ones when references are first asked for.
    if (summary.fileSize < 32n * 1024n * 1024n) void this.ensureXrefs();
  }

  private resetXrefs() {
    this.xrefs = 'none';
    this.xrefCounts = null;
    this.xrefsBuild = null;
  }

  /** Builds the cross-reference index if needed; false if this architecture has none. */
  ensureXrefs(): Promise<boolean> {
    if (!this.file) return Promise.resolve(false);
    if (this.xrefsBuild) return this.xrefsBuild;
    const file = this.file;
    const build = (async () => {
      try {
        if (!(await this.api.xrefsSupported())) {
          if (this.file === file) {
            this.xrefs = 'unsupported';
            this.emit('xrefs');
          }
          return false;
        }
        this.xrefs = 'building';
        this.emit('xrefs');
        const counts = await this.api.prepareXrefs();
        if (this.file !== file) return false;
        this.xrefCounts = counts;
        this.xrefs = 'ready';
        this.emit('xrefs');
        return true;
      } catch (e) {
        if (this.file === file) {
          this.xrefsBuild = null;
          this.xrefs = 'none';
        }
        this.error(e);
        return false;
      }
    })();
    this.xrefsBuild = build;
    return build;
  }

  async attachDebug(name: string, blob: Blob) {
    if (!this.file) return;
    try {
      await this.attached(await this.api.attachDebug(name, blob));
    } catch (e) {
      this.error(e);
    }
  }

  /** Refreshes what depends on debug info after a debug file was attached. */
  private async attached(opened: Opened) {
    if (!this.file || opened.kind !== 'binary') return;
    const [dwarf, sourceFiles] = await Promise.all([this.api.dwarfSummary(), this.api.sourceFiles()]);
    this.file = { ...this.file, summary: opened.summary, dwarf, sourceFiles };
    // DWARF may add function boundaries: references are found again.
    const had = this.xrefs === 'ready';
    this.resetXrefs();
    this.emit('file');
    if (had) void this.ensureXrefs();
    this.emit('sources');
    await this.reselect();
  }

  // --- Annotations ------------------------------------------------------------

  private indexNotes(list: Annotation[]) {
    this.annotations = [...list].sort((a, b) => (a.address < b.address ? -1 : a.address > b.address ? 1 : 0));
    this.notesAt = new Map(this.annotations.map((a) => [a.address, a]));
  }

  /** Replaces all annotations, saves them for this file, and refreshes what depends on them. */
  async setAnnotations(list: Annotation[]) {
    if (!this.file) return;
    try {
      const summary = await this.api.setAnnotations(list);
      this.file = { ...this.file, summary };
      this.indexNotes(list);
      saveNotes(this.file.summary.fingerprint, this.file.name, this.annotations);
      this.emit('annotations');
      await this.reselect();
    } catch (e) {
      this.error(e);
    }
  }

  /** Adds or replaces the annotation starting at `a.address`. */
  async annotate(a: Annotation) {
    const rest = this.annotations.filter((x) => !(x.address === a.address && x.size === a.size));
    await this.setAnnotations([...rest, a]);
  }

  async removeAnnotation(a: Annotation) {
    await this.setAnnotations(this.annotations.filter((x) => !(x.address === a.address && x.size === a.size)));
  }

  exportAnnotations(): string {
    return serializeAnnotations(this.annotations, this.file?.name ?? '', this.file?.summary.fingerprint ?? '');
  }

  /** Merges annotations from a binviz export or a symbol list (CSV, nm, IDA/Ghidra exports). */
  async importAnnotations(text: string): Promise<number> {
    if (!this.file) return 0;
    const f = this.file;
    const mapped = (a: bigint) => f.sections.some((s) => s.loaded && a >= s.address && a < s.address + (s.size > 0n ? s.size : 1n));
    const incoming = parseAnnotations(text).map((a) => {
      // Lists of RVAs: rebase when that is the only reading that lands in the image.
      const base = f.summary.imageBase;
      return base !== undefined && base > 0n && !mapped(a.address) && mapped(a.address + base) ? { ...a, address: a.address + base } : a;
    });
    const byKey = new Map(this.annotations.map((a) => [`${a.address}:${a.size}`, a]));
    for (const a of incoming) {
      // A size-less entry (a symbol list) merges into whatever note starts there.
      const same = a.size === 0n ? this.annotations.find((x) => x.address === a.address) : undefined;
      const key = same ? `${same.address}:${same.size}` : `${a.address}:${a.size}`;
      const old = byKey.get(key);
      byKey.set(key, old ? { ...old, name: a.name || old.name, comment: a.comment || old.comment, reviewed: a.reviewed || old.reviewed } : a);
    }
    await this.setAnnotations([...byKey.values()]);
    return incoming.length;
  }

  /** Opens the map view on a tab (and a source file or unit by name). */
  openMap(tab: MapTab, name?: string) {
    this.intent = { map: { tab, name } };
    this.setView('map');
    this.emit('intent');
  }

  /** Selects a location and refreshes the inspection. */
  async select(target: Target, opts: { origin?: ViewName; view?: ViewName; history?: boolean } = {}) {
    if (!this.file) return;
    const mine = ++this.seq;
    try {
      const inspection =
        target.address !== undefined
          ? await this.api.inspectAddress(target.address)
          : await this.api.inspectOffset(target.offset ?? 0n);
      if (mine !== this.seq) return;
      this.selection = {
        offset: inspection.offset ?? target.offset,
        address: inspection.address ?? target.address,
        inspection,
        origin: opts.origin,
      };
      if (opts.history !== false) this.push({ view: opts.view ?? this.view, target });
      if (opts.view && opts.view !== this.view) this.setView(opts.view, false);
      this.emit('selection');
    } catch (e) {
      this.error(e);
    }
  }

  private async reselect() {
    const { address, offset } = this.selection;
    if (address !== undefined || offset !== undefined) await this.select(address !== undefined ? { address } : { offset }, { history: false });
  }

  setView(view: ViewName, record = true) {
    if (view === this.view) return;
    this.view = view;
    if (record && this.cursor >= 0) {
      const cur = this.history[this.cursor];
      if (cur && cur.view !== view) this.push({ view, target: cur.target });
    }
    this.emit('view');
  }

  private push(entry: HistoryEntry) {
    const cur = this.history[this.cursor];
    if (cur && cur.view === entry.view && cur.target.address === entry.target.address && cur.target.offset === entry.target.offset) return;
    this.history = this.history.slice(0, this.cursor + 1);
    this.history.push(entry);
    if (this.history.length > 200) this.history.shift();
    this.cursor = this.history.length - 1;
  }

  /** Opens the DWARF view on a DIE. */
  openDie(unit: number, offset: bigint) {
    this.intent = { die: { unit, offset } };
    this.setView('dwarf');
    this.emit('intent');
  }

  /** Opens the source view on a line. */
  openSource(file: number, line: number) {
    this.intent = { source: { file, line } };
    this.setView('sources');
    this.emit('intent');
  }

  /** Opens the layout view with the region containing `offset` revealed. */
  revealRegion(offset: bigint) {
    this.intent = { region: offset };
    this.setView('layout');
    this.emit('intent');
  }

  canGoBack() {
    return this.cursor > 0;
  }

  canGoForward() {
    return this.cursor < this.history.length - 1;
  }

  async back() {
    if (!this.canGoBack()) return;
    await this.go(this.history[--this.cursor]);
  }

  async forward() {
    if (!this.canGoForward()) return;
    await this.go(this.history[++this.cursor]);
  }

  private async go(entry: HistoryEntry) {
    if (entry.view !== this.view) {
      this.view = entry.view;
      this.emit('view');
    }
    await this.select(entry.target, { history: false });
  }

  /** Matches dropped/selected files to DWARF source paths by longest suffix. */
  async loadSources(files: { path: string; file: File }[]): Promise<number> {
    if (!this.file) return 0;
    const norm = (p: string) => p.replace(/\\/g, '/').toLowerCase();
    const candidates = files.map((f) => ({ ...f, n: norm(f.path) }));
    let matched = 0;
    for (const sf of this.file.sourceFiles) {
      const target = norm(sf.path);
      let best: (typeof candidates)[number] | undefined;
      let bestLen = 0;
      for (const c of candidates) {
        // Compare path components from the end.
        const a = target.split('/');
        const b = c.n.split('/');
        let k = 0;
        while (k < a.length && k < b.length && a[a.length - 1 - k] === b[b.length - 1 - k]) k++;
        if (k > bestLen) {
          bestLen = k;
          best = c;
        }
      }
      if (best && bestLen >= 1) {
        this.sources.set(sf.id, await best.file.text());
        this.sourceOrigin.set(sf.id, best.path);
        matched++;
      }
    }
    this.emit('sources');
    return matched;
  }

  async sourceText(file: number): Promise<string | undefined> {
    const loaded = this.sources.get(file);
    if (loaded !== undefined) return loaded;
    const sf = this.file?.sourceFiles[file];
    if (sf?.embeddedSource) {
      const text = await this.api.embeddedSource(file);
      if (text !== undefined) {
        this.sources.set(file, text);
        this.sourceOrigin.set(file, 'embedded in DWARF');
        return text;
      }
    }
    return undefined;
  }

  /** Virtual address of a file offset, computed locally from the segments. */
  addressOf(offset: bigint): bigint | undefined {
    for (const s of this.file?.segments ?? []) {
      if (s.mapped && offset >= s.fileOffset && offset < s.fileOffset + s.fileSize) return s.address + (offset - s.fileOffset);
    }
    if (this.file && this.file.segments.length === 0) {
      for (const s of this.file.sections) {
        if (s.loaded && !s.compressed && s.fileOffset !== undefined && offset >= s.fileOffset && offset < s.fileOffset + s.fileSize) {
          return s.address + (offset - s.fileOffset);
        }
      }
    }
    return undefined;
  }
}

export const store = new Store();

const NOTES_KEY = (fingerprint: string) => `binviz-notes:${fingerprint}`;

function loadNotes(sha: string): Annotation[] {
  try {
    const raw = localStorage.getItem(NOTES_KEY(sha));
    return raw ? parseAnnotations(raw) : [];
  } catch {
    return [];
  }
}

function saveNotes(sha: string, name: string, list: Annotation[]) {
  try {
    if (list.length === 0) localStorage.removeItem(NOTES_KEY(sha));
    else localStorage.setItem(NOTES_KEY(sha), serializeAnnotations(list, name, sha));
  } catch {
    /* storage full or unavailable: notes last for this session only */
  }
}
