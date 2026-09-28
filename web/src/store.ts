// Application state: the open binary, the current selection, navigation
// history and loaded source files. Views subscribe to changes.
import { Api } from './api';
import { parseAnnotations, serializeAnnotations } from './notes';
import type {
  Annotation, ContainerInfo, DwarfSummary, Inspection, LabelFile, LabelFormat, LabelImport, LogSummary, Opened, PackageInfo,
  PackageSource, PatchFormat, PatchState, RefCounts, RegionKind, Section, Segment, SizeReport, SourceFile, Summary, Symbolicated,
  BaselineSource, Comparison,
} from './types';
import { basename, setAddressStyle } from './util';

export type ViewName = 'folder' | 'crash' | 'diff' | 'overview' | 'layout' | 'hex' | 'code' | 'calls' | 'symbols' | 'dwarf' | 'sources' | 'text' | 'tiles' | 'patch';

/** How a navigation is recorded: a new place in history, an update of the current one, or neither. */
export type HistoryMode = 'push' | 'replace' | 'none';

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

/** A place in the app: what Back and Forward move between, and what the URL says. */
export interface Place {
  view: ViewName;
  target: Target;
  /** The view's own state: its tab. */
  state?: string;
  /** What the view was asked to reveal: a DIE, a source line, a region. */
  intent?: Intent;
  /** The binary shown (its fingerprint). */
  file?: string;
}

type Events = {
  file: [];
  /** The table file text is read with changed. */
  table: [];
  container: [];
  selection: [];
  view: [];
  sources: [];
  intent: [];
  annotations: [];
  xrefs: [];
  package: [];
  crash: [];
  diff: [];
  error: [string];
  status: [string];
  /** Back or Forward became possible or impossible. */
  history: [];
  /** The patch or the edits changed. */
  patch: [];
  /** History restored a view's state (its tab…) while it was shown. */
  viewstate: [ViewName];
};

/** A request for a view to reveal something specific when it is next shown. */
export interface Intent {
  die?: { unit: number; offset: bigint };
  source?: { file: number; line: number };
  region?: bigint;
  /** A source file or compilation unit to pick in the Sources view, by name. */
  contributor?: string;
}

/** The tab each view with tabs starts on: places record it, URLs leave it out. */
const DEFAULT_STATE: Partial<Record<ViewName, string>> = { layout: 'regions', symbols: 'symbols', sources: 'files', dwarf: 'dies' };

const hexOf = (n: bigint) => `0x${n.toString(16)}`;
/** URL-encodes, keeping the characters addresses and paths read better with. */
const enc = (s: string) => encodeURIComponent(s).replace(/%3A/gi, ':').replace(/%40/g, '@').replace(/%2F/gi, '/');

function hasIntent(i: Intent): boolean {
  return i.die !== undefined || i.source !== undefined || i.region !== undefined || i.contributor !== undefined;
}

function samePlace(a: Place, b: Place): boolean {
  return (
    a.view === b.view &&
    a.file === b.file &&
    a.state === b.state &&
    a.target.address === b.target.address &&
    a.target.offset === b.target.offset &&
    a.intent?.die?.offset === b.intent?.die?.offset &&
    a.intent?.source?.file === b.intent?.source?.file &&
    a.intent?.source?.line === b.intent?.source?.line &&
    a.intent?.region === b.intent?.region &&
    a.intent?.contributor === b.intent?.contributor
  );
}

class Store {
  readonly api = new Api();
  file: Loaded | null = null;
  container: { name: string; info: ContainerInfo; blob: Blob } | null = null;
  package: OpenPackage | null = null;
  /** Sizes compared with an earlier build (`stale` once what is open changed). */
  diff: { baseline: string; kind: 'binary' | 'folder'; result: Comparison | null; stale: boolean; busy: boolean } | null = null;
  /** A crash report being looked at, symbolicated with what is open. */
  crash: { name: string; text: string; result: Symbolicated | null } | null = null;
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
  /** Each view's own state (its tab), kept in places. */
  viewState: Partial<Record<ViewName, string>> = {};
  /** The bundled sample open, if any: the URL names it, so that it reopens. */
  sample: string | null = null;
  /** Every place recorded, by id: a browser history entry holds its place's id. */
  private places: Place[] = [];
  /** The ids of the browser's history entries as far as we know them, and where we are in them. */
  private stack: number[] = [];
  private pos = -1;
  /** Tells this page's history entries from earlier pages' (their places are gone). */
  private readonly session = Math.random().toString(36).slice(2);
  /** While history is restoring a place: nothing is recorded. */
  private restoring = false;
  /** The browser made the current history entry (the URL was edited): the next place goes into it. */
  private adopting = false;
  /** What was last asked to be selected: places keep it as asked. */
  private target: Target = {};
  /** How to reopen the last few files, for Back and Forward to them (by fingerprint). */
  private reopeners = new Map<string, () => Promise<void>>();
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

  /** The table file the text of games is read with (see the Text view), and each byte's text by it. */
  table: { text: string; entries: number; chars: string[] } | null = null;

  /** Reads text with this table file (`.tbl` lines) from now on. */
  async setTable(text: string) {
    const entries = await this.api.tableSet(text);
    const chars = await this.api.tableChars();
    this.table = { text, entries, chars };
    this.emit('table');
    return entries;
  }

  async clearTable() {
    await this.api.tableClear();
    this.table = null;
    this.emit('table');
  }

  error(e: unknown) {
    this.emit('error', e instanceof Error ? e.message : String(e));
  }

  /**
   * Opens a binary. `sample` names a bundled sample (the URL then reopens
   * it); `reopen` opens it again for Back and Forward, when opening it
   * involves more than its bytes.
   */
  async open(name: string, blob: Blob, opts: { sample?: string; reopen?: () => Promise<void> } = {}) {
    this.emit('status', `Opening ${name}…`);
    try {
      // Drop our references to the old file first so its memory can go.
      this.file = null;
      this.container = null;
      this.dropPackage();
      this.sample = opts.sample ?? null;
      const opened = await this.api.open(name, blob);
      if (opened.kind === 'container') {
        this.container = { name, info: opened.info, blob };
        this.emit('container');
        this.emit('status', '');
        return;
      }
      this.remember(opened.summary.fingerprint, opts.reopen ?? (() => this.open(name, blob, opts)));
      await this.loaded(opened.name, opened.summary, blob);
      void this.resymbolicate();
      this.diffChanged();
    } catch (e) {
      this.emit('status', '');
      this.error(e);
    }
  }

  /** Keeps how to reopen a file for Back and Forward (the last eight). */
  private remember(fingerprint: string, reopen: () => Promise<void>) {
    this.reopeners.delete(fingerprint);
    this.reopeners.set(fingerprint, reopen);
    if (this.reopeners.size > 8) this.reopeners.delete(this.reopeners.keys().next().value!);
  }

  /** Reads an earlier build to compare sizes with, and compares. */
  async compareWith(name: string, source: BaselineSource) {
    const d = { baseline: name, kind: 'binary' as 'binary' | 'folder', result: null, stale: true, busy: true };
    this.diff = d;
    this.setView('diff');
    this.emit('diff');
    try {
      d.kind = await this.api.compareWith(source);
      d.busy = false;
      await this.refreshDiff();
    } catch (e) {
      if (this.diff === d) this.diff = null;
      this.emit('diff');
      this.error(e);
    }
  }

  /** Compares again, with what is open now. */
  async refreshDiff() {
    const d = this.diff;
    if (!d || d.busy || !(this.file || this.package)) return;
    d.stale = false;
    d.busy = true;
    this.emit('diff');
    try {
      const result = await this.api.sizeDiff(60);
      if (this.diff !== d) return;
      d.result = result;
    } catch (e) {
      this.error(e);
    } finally {
      d.busy = false;
      if (this.diff === d) this.emit('diff');
    }
  }

  stopComparing() {
    this.diff = null;
    void this.api.clearBaseline();
    this.emit('diff');
  }

  private diffChanged() {
    if (!this.diff) return;
    this.diff.stale = true;
    if (this.view === 'diff') void this.refreshDiff();
  }

  /** Opens a crash report (its text), symbolicated with what is open, and again whenever that changes. */
  async openCrash(name: string, text: string) {
    this.crash = { name, text, result: null };
    this.setView('crash');
    this.emit('crash');
    this.emit('view');
    await this.resymbolicate();
  }

  private async resymbolicate() {
    const c = this.crash;
    if (!c) return;
    try {
      const result = await this.api.symbolicateCrash(c.text);
      if (this.crash !== c) return;
      c.result = result;
      this.emit('crash');
    } catch (e) {
      this.error(e);
    }
  }

  /** Shows a frame's code, switching to its binary in a folder. */
  async goToFrame(binary: number | undefined, address: bigint | undefined) {
    if (binary === undefined || address === undefined) return;
    if (this.package && binary !== this.package.current) await this.selectBinary(binary);
    await this.select({ address }, { view: 'code' });
  }

  async openMember(index: number) {
    const c = this.container;
    const member = c?.info.members.find((m) => m.index === index);
    if (!c || !member) return;
    try {
      const opened = await this.api.openMember(index);
      // A raw CD image keeps a file's bytes sector by sector: the worker has them.
      const blob = member.contiguous === false ? null : c.blob.slice(Number(member.offset), Number(member.offset + member.size));
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
  async openFolder(sources: PackageSource[], opts: { sample?: string } = {}): Promise<boolean> {
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
        this.sample = opts.sample ?? null;
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
    // Opened to symbolicate a crash report: stay on it.
    if (!(this.crash && this.view === 'crash')) this.view = 'folder';
    this.emit('package');
    if (opened?.kind === 'binary') {
      this.rememberBinary(this.package, 0, opened.summary.fingerprint);
      await this.loaded(opened.name, opened.summary, await this.api.packageFileBlob(info.binaries[0].file));
    } else {
      this.emit('status', '');
      this.emit('file');
      this.emit('view');
    }
    const again = keep?.path !== undefined ? info.binaries.findIndex((b) => b.path === keep.path) : -1;
    if (again > 0) await this.selectBinary(again);
    if (keep) this.setView(keep.view);
    void this.resymbolicate();
    this.diffChanged();
    // Small folders are combined right away; big ones when asked.
    const n = info.binaries.length;
    if (n > 0 && n <= AUTO_ANALYZE_COUNT && analysisBytes(info) < AUTO_ANALYZE_BYTES) void this.analyzePackage();
  }

  /**
   * Attaches the one of the scanned debug files that matches the open binary:
   * by build ID, or by the name its debug link or PDB path gives.
   */
  private async attachMatching(info: PackageInfo) {
    const f = this.file!;
    const want = f.summary.buildId ? buildIdKey(f.summary.buildId) : '';
    const linked = f.summary.debugLink?.split(' (crc ')[0].split(/[\\/]/).pop()?.toLowerCase();
    const match =
      info.binaries.find((b) => want && b.ids.some((x) => x.id && buildIdKey(x.id) === want)) ??
      info.binaries.find((b) => linked && basename(b.path).toLowerCase() === linked) ??
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
        if (opened.kind === 'binary') {
          this.rememberBinary(p, index, opened.summary.fingerprint);
          await this.loaded(opened.name, opened.summary, await this.api.packageFileBlob(info.binaries[index].file));
        }
        // A binary compared with a binary: now another one.
        if (this.diff?.result?.kind === 'binary') this.diffChanged();
      } catch (e) {
        this.error(e);
        return;
      }
    }
    if (view) this.setView(view);
  }

  /** Keeps how to reopen a binary of a folder: the folder too, if another opened since. */
  private rememberBinary(p: OpenPackage, index: number, fingerprint: string) {
    const path = p.info.binaries[index]?.path;
    const sources = p.sources;
    const sample = this.sample ?? undefined;
    this.remember(fingerprint, async () => {
      if (this.package?.sources !== sources) await this.openFolder(sources, { sample });
      const i = this.package?.info.binaries.findIndex((b) => b.path === path) ?? -1;
      if (i >= 0) await this.selectBinary(i);
    });
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
    setAddressStyle(banked(summary) ? 'banked' : 'hex');
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
    this.target = {};
    this.codeLog = null;
    // Edits made to this very file before (the same bytes) are still there.
    this.undo = [];
    this.patch = await this.api.patchState(PATCH_ROWS);
    this.sources.clear();
    this.sourceOrigin.clear();
    this.resetXrefs();
    this.emit('status', '');
    this.emit('file');
    this.emit('sources');
    // Small files are indexed right away; large ones when references are first asked for.
    if (summary.fileSize < 32n * 1024n * 1024n) void this.ensureXrefs();
    // The file's first place: done before whoever opened it goes on (a link's own place replaces it).
    const entry = summary.entry;
    await this.select(entry !== undefined ? { address: entry } : { offset: 0n });
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

  /**
   * Links the open binary's debug map from the object files of a folder the
   * user chose (a Mach-O binary linked without dsymutil keeps its DWARF there).
   */
  async linkDebugMap(files: { path: string; file: File }[]) {
    if (!this.file) return;
    try {
      const { report, summary } = await this.api.linkDebugMap(files);
      const missing = report.missing.length + report.failed.length;
      this.emit('status', `DWARF from ${report.linked} of ${report.objects} object files${missing ? ` (${missing} missing or unusable)` : ''}`);
      await this.debugChanged(summary);
    } catch (e) {
      this.error(e);
    }
  }

  /** Refreshes what depends on debug info after a debug file was attached. */
  private async attached(opened: Opened) {
    if (!this.file || opened.kind !== 'binary') return;
    await this.debugChanged(opened.summary);
  }

  /** Refreshes what depends on debug info once the binary has more of it. */
  private async debugChanged(summary: Summary) {
    if (!this.file) return;
    const [dwarf, sourceFiles] = await Promise.all([this.api.dwarfSummary(), this.api.sourceFiles()]);
    this.file = { ...this.file, summary, dwarf, sourceFiles };
    // DWARF may add function boundaries: references are found again.
    const had = this.xrefs === 'ready';
    this.resetXrefs();
    this.emit('file');
    if (had) void this.ensureXrefs();
    this.emit('sources');
    await this.reselect();
    void this.resymbolicate();
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

  /**
   * Merges annotations from a binviz export or a symbol list (CSV, nm,
   * IDA/Ghidra exports); for a ROM, from an emulator's label file too (by
   * its `name`: .mlb, .nl, .sym).
   */
  async importAnnotations(text: string, name = ''): Promise<number> {
    if (!this.file) return 0;
    const f = this.file;
    if (f.summary.format === 'rom' && LABEL_FILE.test(name)) return (await this.importLabels(name, text))?.labels.length ?? 0;
    const mapped = (a: bigint) => f.sections.some((s) => s.loaded && a >= s.address && a < s.address + (s.size > 0n ? s.size : 1n));
    const incoming = parseAnnotations(text).map((a) => {
      // Lists of RVAs: rebase when that is the only reading that lands in the image.
      const base = f.summary.imageBase;
      return base !== undefined && base > 0n && !mapped(a.address) && mapped(a.address + base) ? { ...a, address: a.address + base } : a;
    });
    await this.mergeAnnotations(incoming);
    return incoming.length;
  }

  /** Adds notes, merged into those at the same place. */
  private async mergeAnnotations(incoming: Annotation[]) {
    const byKey = new Map(this.annotations.map((a) => [`${a.address}:${a.size}`, a]));
    for (const a of incoming) {
      // A size-less entry (a symbol list) merges into whatever note starts there.
      const same = a.size === 0n ? this.annotations.find((x) => x.address === a.address) : undefined;
      const key = same ? `${same.address}:${same.size}` : `${a.address}:${a.size}`;
      const old = byKey.get(key);
      byKey.set(key, old ? { ...old, name: a.name || old.name, comment: a.comment || old.comment, reviewed: a.reviewed || old.reviewed } : a);
    }
    await this.setAnnotations([...byKey.values()]);
  }

  // --- Emulators: code/data logs and label files --------------------------------

  /** What the code/data log the open ROM was read with covers. */
  codeLog: LogSummary | null = null;

  /**
   * Reads the open ROM again with a code/data log (FCEUX's or Mesen's): the
   * code the game ran is followed too, the data it read isn't taken for
   * code, and an NES game's switched banks go where they ran.
   */
  async loadCodeLog(name: string, blob: Blob): Promise<LogSummary | null> {
    if (!this.file) return null;
    try {
      const summary = await this.api.codeLog(new Uint8Array(await blob.arrayBuffer()));
      this.codeLog = summary;
      await this.reread();
      return summary;
    } catch (e) {
      this.error(e instanceof Error ? new Error(`${name}: ${e.message}`) : e);
      return null;
    }
  }

  /** The open file was read again: its sections (a ROM's banks), functions and references may have changed. */
  private async reread() {
    if (!this.file) return;
    const [summary, sections, segments, composition] = await Promise.all([this.api.summary(), this.api.sections(), this.api.segments(), this.api.composition()]);
    this.file = { ...this.file, summary, sections, segments, composition };
    const had = this.xrefs === 'ready';
    this.resetXrefs();
    this.emit('file');
    if (had || summary.fileSize < 32n * 1024n * 1024n) void this.ensureXrefs();
    // Banks may have moved: the same byte, wherever it is now.
    const { offset, address } = this.selection;
    if (offset !== undefined || address !== undefined) await this.select(offset !== undefined ? { offset } : { address }, { history: 'none' });
  }

  // --- Patches ----------------------------------------------------------------------

  /** A patched version of the open file (a patch file applied, bytes edited), and what it changes. */
  patch: PatchState | null = null;
  /** Edits in the hex view, newest last: where, and the bytes there before. */
  private undo: { offset: bigint; before: Uint8Array }[] = [];

  /** Applies an IPS, UPS or BPS patch to the open file: the Patch view shows what it changes. */
  async applyPatch(name: string, blob: Blob) {
    if (!this.file) return;
    try {
      this.patch = await this.api.patchApply(name, new Uint8Array(await blob.arrayBuffer()), PATCH_ROWS);
      this.undo = [];
      this.emit('patch');
      this.setView('patch');
    } catch (e) {
      this.error(e instanceof Error ? new Error(`${name}: ${e.message}`) : e);
    }
  }

  /** Writes bytes over the open file's: edits, which the Patch view shows and saves as a patch. */
  async editBytes(offset: bigint, bytes: Uint8Array) {
    if (!this.file) return;
    try {
      const before = await this.readPatched(offset, bytes.length);
      this.patch = await this.api.patchEdit(offset, bytes, PATCH_ROWS);
      this.undo.push({ offset, before });
      this.emit('patch');
    } catch (e) {
      this.error(e);
    }
  }

  canUndoEdit() {
    return this.undo.length > 0;
  }

  /** Takes the last edit back. */
  async undoEdit() {
    const last = this.undo.pop();
    if (!last) return;
    this.patch = await this.api.patchEdit(last.offset, last.before, PATCH_ROWS);
    this.emit('patch');
  }

  /** Reads the changes again (the table file changed, say). */
  async refreshPatch() {
    if (!this.patch) return;
    this.patch = await this.api.patchState(PATCH_ROWS);
    this.emit('patch');
  }

  /** Drops the patch and the edits. */
  async clearPatch() {
    await this.api.patchClear();
    this.patch = null;
    this.undo = [];
    this.emit('patch');
    if (this.view === 'patch') this.setView('hex');
  }

  /** Bytes of the patched file (the open file's, with no patch). */
  async readPatched(offset: bigint, count: number): Promise<Uint8Array> {
    if (!this.patch) return this.readBytes(Number(offset), Number(offset) + count);
    return this.api.patchRead(offset, count);
  }

  /** A patch file (IPS, UPS or BPS) that turns the open file into the patched one. */
  async patchFile(format: PatchFormat): Promise<Blob> {
    return new Blob([(await this.api.patchCreate(format)) as Uint8Array<ArrayBuffer>]);
  }

  /** The patched file. */
  async patchedFile(): Promise<Blob> {
    return new Blob([(await this.api.patchTarget()) as Uint8Array<ArrayBuffer>]);
  }

  /** What to call the patched file: the game's name, patched. */
  patchedName(): string {
    const name = basename(this.file?.name ?? 'file');
    const dot = name.lastIndexOf('.');
    return dot > 0 ? `${name.slice(0, dot)} (patched)${name.slice(dot)}` : `${name} (patched)`;
  }

  /** Opens the patched file in place of the open one, with the notes carried over. */
  async openPatched() {
    const f = this.file;
    if (!f || !this.patch) return;
    const notes = this.annotations;
    const blob = await this.patchedFile();
    await this.open(this.patchedName(), blob);
    if (this.file && this.file !== f && this.annotations.length === 0 && notes.length > 0) await this.setAnnotations(notes);
  }

  /** Reads an emulator's label file (Mesen, FCEUX, RGBDS, WLA DX, no$gba) into the notes. */
  async importLabels(name: string, text: string): Promise<LabelImport | null> {
    if (!this.file) return null;
    const read = await this.api.readLabels(name, text);
    await this.mergeAnnotations(read.labels);
    return read;
  }

  /** The notes as label files an emulator reads, named after the ROM. */
  async exportLabels(format: LabelFormat): Promise<{ name: string; text: string }[]> {
    const f = this.file;
    if (!f) return [];
    const files: LabelFile[] = await this.api.writeLabels(format);
    const base = basename(f.name);
    // FCEUX's name lists go by the ROM's whole name (game.nes.0.nl); the others replace its extension.
    const stem = format === 'nl' ? base : base.replace(/\.[^.]+$/, '');
    return files.map((x) => ({ name: `${stem}.${x.suffix}`, text: x.text }));
  }

  /**
   * Selects a location and refreshes the inspection. A selection that goes
   * somewhere (to another view, or from a link) is a new place in history;
   * one made within a view (`origin` without `view`: a click on a byte or a
   * row) moves the current place along.
   */
  async select(target: Target, opts: { origin?: ViewName; view?: ViewName; history?: HistoryMode } = {}) {
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
      this.target = target.address !== undefined ? { address: target.address } : { offset: target.offset ?? 0n };
      const moved = opts.view !== undefined && opts.view !== this.view;
      if (moved) this.view = opts.view!;
      this.record(opts.history ?? (opts.view !== undefined || opts.origin === undefined ? 'push' : 'replace'));
      if (moved) this.emit('view');
      this.emit('selection');
    } catch (e) {
      this.error(e);
    }
  }

  private async reselect() {
    const { address, offset } = this.selection;
    if (address !== undefined || offset !== undefined) await this.select(address !== undefined ? { address } : { offset }, { history: 'none' });
  }

  setView(view: ViewName, history: HistoryMode = 'push') {
    if (view === this.view) return;
    this.view = view;
    this.record(history);
    this.emit('view');
  }

  /** Changes a view's own state (its tab…): a new place in history when it is the view shown. */
  setViewState(view: ViewName, value: string, history: HistoryMode = 'push') {
    if (this.viewState[view] === value) return;
    this.viewState[view] = value;
    if (view === this.view) this.record(history);
  }

  /** Shows a view asked to reveal something: a new place in history. */
  private navigate(view: ViewName, intent: Intent, state?: string) {
    this.intent = intent;
    if (state !== undefined) this.viewState[view] = state;
    const moved = view !== this.view;
    this.view = view;
    this.record('push');
    if (moved) this.emit('view');
    else if (state !== undefined) this.emit('viewstate', view);
    this.emit('intent');
  }

  /** Opens the DWARF view on a DIE. */
  openDie(unit: number, offset: bigint) {
    this.navigate('dwarf', { die: { unit, offset } }, 'dies');
  }

  /** Opens the source view on a line. */
  openSource(file: number, line: number) {
    this.navigate('sources', { source: { file, line } }, 'files');
  }

  /** Opens the Sources view on a source file or compilation unit (by name): where its code and data are. */
  openContributor(mode: 'files' | 'units', name: string) {
    this.navigate('sources', { contributor: name }, mode);
  }

  /** Opens the layout view with the region containing `offset` revealed. */
  revealRegion(offset: bigint) {
    this.navigate('layout', { region: offset }, 'regions');
  }

  // --- History ------------------------------------------------------------------

  /**
   * Records where the app is: a new place (a browser history entry), or the
   * current place updated. The URL follows, so that it can be shared (a
   * sample reopens from it) and Back and Forward work as in any web page.
   */
  private record(mode: HistoryMode, intent = this.intent) {
    if (mode === 'none' || this.restoring || typeof history === 'undefined') return;
    const cur = this.pos >= 0 ? this.places[this.stack[this.pos]] : undefined;
    const place: Place = {
      view: this.view,
      target: this.target,
      state: this.viewState[this.view] ?? DEFAULT_STATE[this.view],
      intent: hasIntent(intent) ? { ...intent } : undefined,
      file: this.file?.summary.fingerprint,
    };
    // Views consume their intents: a place moved along keeps the one it was opened with.
    if (!place.intent && mode === 'replace' && cur?.view === place.view && cur.file === place.file) place.intent = cur.intent;
    // Another file is always a new place, but for the first (it replaces the landing page).
    if (cur && cur.file !== place.file) mode = cur.file ? 'push' : 'replace';
    if (cur && mode === 'push' && samePlace(cur, place)) return;
    const url = `${location.pathname}${location.search}${this.hashOf(place)}`;
    if (this.adopting || this.pos < 0 || mode === 'replace') {
      // Into the current entry: as a new place if the browser made it, else in place of ours.
      const fresh = this.adopting || this.pos < 0;
      const id = fresh ? this.places.push(place) - 1 : this.stack[this.pos];
      this.places[id] = place;
      if (fresh) {
        this.stack = [...this.stack.slice(0, this.pos + 1), id];
        this.pos = this.stack.length - 1;
      }
      this.adopting = false;
      history.replaceState({ binviz: { session: this.session, id } }, '', url);
    } else {
      const id = this.places.push(place) - 1;
      this.stack = [...this.stack.slice(0, this.pos + 1), id];
      this.pos = this.stack.length - 1;
      history.pushState({ binviz: { session: this.session, id } }, '', url);
    }
    this.emit('history');
  }

  /** The URL fragment for a place: `#sample=…&view=code&goto=0x401000`. */
  private hashOf(p: Place): string {
    const q: string[] = [];
    if (this.sample) q.push(`sample=${enc(this.sample)}`);
    const pkg = this.package;
    if (this.sample && pkg && pkg.current > 0) q.push(`bin=${enc(pkg.info.binaries[pkg.current]?.path ?? '')}`);
    q.push(`view=${p.view}`);
    if (p.state !== undefined && p.state !== DEFAULT_STATE[p.view]) q.push(`tab=${enc(p.state)}`);
    if (p.target.address !== undefined) q.push(`goto=${hexOf(p.target.address)}`);
    else if (p.target.offset !== undefined) q.push(`goto=@${hexOf(p.target.offset)}`);
    if (p.intent?.die) q.push(`die=${p.intent.die.unit}:${hexOf(p.intent.die.offset)}`);
    if (p.intent?.source) q.push(`line=${p.intent.source.file}:${p.intent.source.line}`);
    if (p.intent?.contributor) q.push(`name=${enc(p.intent.contributor)}`);
    return `#${q.join('&')}`;
  }

  /**
   * The browser went back or forward to one of its history entries: shows
   * its place. False when the entry isn't one of this page's: the URL says
   * where to go, and that place goes into the entry.
   */
  async popped(state: unknown): Promise<boolean> {
    const s = (state as { binviz?: { session?: string; id?: number } } | null)?.binviz;
    const id = s && s.session === this.session ? s.id : undefined;
    const place = id !== undefined ? this.places[id] : undefined;
    if (id === undefined || !place) {
      this.adopting = true;
      return false;
    }
    const at = this.stack.indexOf(id);
    if (at >= 0) this.pos = at;
    else {
      this.stack = [...this.stack.slice(0, this.pos + 1), id];
      this.pos = this.stack.length - 1;
    }
    await this.restore(place);
    this.emit('history');
    return true;
  }

  /** Shows a place again: its file (reopened if need be), view, state, intent and selection. */
  private async restore(p: Place) {
    if (p.file && p.file !== this.file?.summary.fingerprint) {
      const reopen = this.reopeners.get(p.file);
      if (!reopen) {
        this.error('That place is in a file that is no longer open.');
        return;
      }
      this.restoring = true;
      try {
        await reopen();
      } finally {
        this.restoring = false;
      }
      if (this.file?.summary.fingerprint !== p.file) return;
    }
    await this.goTo(p, 'none');
  }

  /**
   * Shows a place (a URL's, or one from history): its view with its state
   * (a tab…), what the view is to reveal, and the selection; recorded once.
   */
  async goTo(p: { view?: ViewName; state?: string; target?: Target; intent?: Intent }, history: HistoryMode) {
    const view = p.view ?? this.view;
    // A view named without its tab is on its first.
    const state = p.state ?? (p.view !== undefined ? DEFAULT_STATE[view] : undefined);
    if (state !== undefined) this.viewState[view] = state;
    const intent: Intent = p.intent ? { ...p.intent } : {};
    this.intent = { ...intent };
    const moved = view !== this.view;
    this.view = view;
    this.restoring = true;
    try {
      if (moved) this.emit('view');
      else if (state !== undefined) this.emit('viewstate', view);
      if (p.target && (p.target.address !== undefined || p.target.offset !== undefined)) await this.select(p.target, { history: 'none' });
    } finally {
      this.restoring = false;
    }
    // The view may have acted on the intent already: the place keeps it.
    this.record(history, intent);
    if (hasIntent(this.intent)) this.emit('intent');
  }

  /** The browser made the current history entry (the URL was edited): the place shown next goes into it. */
  adoptEntry() {
    this.adopting = true;
  }

  canGoBack() {
    return this.pos > 0;
  }

  canGoForward() {
    return this.pos < this.stack.length - 1;
  }

  back() {
    history.back();
  }

  forward() {
    history.forward();
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

/** A game ROM whose CPU sees banks (NES, Game Boy) or 24-bit addresses (SNES): addresses read as bank:address. */
function banked(s: Summary): boolean {
  return s.format === 'rom' && s.bits <= 16;
}

/** Changes the Patch view lists (the first, when a patch changes more). */
const PATCH_ROWS = 5000;

/** Patch files, by name. */
export const PATCH_FILE = /\.(ips|ups|bps)$/i;

/** Emulators' label files, by name. */
export const LABEL_FILE = /\.(mlb|nl|sym)$/i;

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
