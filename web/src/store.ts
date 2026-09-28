// Application state: the open binary, the current selection, navigation
// history and loaded source files. Views subscribe to changes.
import { Api } from './api';
import { parseAnnotations, serializeAnnotations } from './notes';
import type {
  Annotation, ContainerInfo, DwarfSummary, Inspection, RegionKind, Section, Segment, SourceFile, Summary,
} from './types';

export type ViewName = 'overview' | 'layout' | 'hex' | 'code' | 'symbols' | 'sections' | 'dwarf' | 'sources' | 'map';
export type MapTab = 'files' | 'units' | 'coverage';

export interface Loaded {
  name: string;
  summary: Summary;
  bytes: Uint8Array;
  sections: Section[];
  segments: Segment[];
  dwarf: DwarfSummary | null;
  sourceFiles: SourceFile[];
  composition: [RegionKind, bigint][];
  /** SHA-256 of the bytes (hex), the key annotations are saved under. */
  sha256: string;
}

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
  container: { name: string; info: ContainerInfo; bytes: Uint8Array } | null = null;
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

  async open(name: string, bytes: Uint8Array) {
    this.emit('status', `Parsing ${name}…`);
    try {
      const opened = await this.api.open(name, bytes.slice());
      if (opened.kind === 'container') {
        this.file = null;
        this.container = { name, info: opened.info, bytes };
        this.emit('container');
        this.emit('status', '');
        return;
      }
      this.container = null;
      await this.loaded(opened.name, opened.summary);
    } catch (e) {
      this.emit('status', '');
      this.error(e);
    }
  }

  async openMember(index: number) {
    try {
      const opened = await this.api.openMember(index);
      if (opened.kind === 'binary') await this.loaded(opened.name, opened.summary);
    } catch (e) {
      this.error(e);
    }
  }

  private async loaded(name: string, summary: Summary) {
    const [bytes, sections, segments, dwarf, sourceFiles, composition] = await Promise.all([
      this.api.bytes(),
      this.api.sections(),
      this.api.segments(),
      this.api.dwarfSummary(),
      this.api.sourceFiles(),
      this.api.composition(),
    ]);
    const sha256 = await digest(bytes);
    this.file = { name, summary, bytes, sections, segments, dwarf, sourceFiles, composition, sha256 };
    this.annotations = [];
    this.notesAt.clear();
    const saved = loadNotes(sha256);
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
    this.emit('status', '');
    this.emit('file');
    this.emit('sources');
    const entry = summary.entry;
    if (entry !== undefined) void this.select({ address: entry }, { history: true });
    else void this.select({ offset: 0n }, { history: true });
  }

  async attachDebug(name: string, bytes: Uint8Array) {
    if (!this.file) return;
    try {
      const opened = await this.api.attachDebug(name, bytes);
      if (opened.kind !== 'binary') return;
      const [dwarf, sourceFiles] = await Promise.all([this.api.dwarfSummary(), this.api.sourceFiles()]);
      this.file = { ...this.file, summary: opened.summary, dwarf, sourceFiles };
      this.emit('file');
      this.emit('sources');
      await this.reselect();
    } catch (e) {
      this.error(e);
    }
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
      saveNotes(this.file.sha256, this.file.name, this.annotations);
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
    return serializeAnnotations(this.annotations, this.file?.name ?? '', this.file?.sha256 ?? '');
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

async function digest(bytes: Uint8Array): Promise<string> {
  try {
    const buf = await crypto.subtle.digest('SHA-256', bytes as unknown as ArrayBuffer);
    return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
  } catch {
    // No WebCrypto (insecure context): fall back to a cheap fingerprint.
    let h = 2166136261;
    for (let i = 0; i < bytes.length; i += Math.max(1, Math.floor(bytes.length / 65536))) h = Math.imul(h ^ bytes[i], 16777619);
    return `fnv-${bytes.length}-${(h >>> 0).toString(16)}`;
  }
}

const NOTES_KEY = (sha: string) => `binviz-notes:${sha}`;

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
