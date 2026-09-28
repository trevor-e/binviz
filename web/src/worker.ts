// Runs the WebAssembly session off the main thread so parsing large binaries
// never freezes the UI. Messages are {id, method, args} → {id, result | error},
// with {id, progress} / {id, status} updates while files are being read in.
//
// Folders and zips of binaries are read here too: zip entries are inflated
// in the worker (zips inside open like folders), and binaries are streamed
// straight into WebAssembly memory, one at a time. CD images (a PlayStation
// game's disc, hundreds of megabytes) are read a sector at a time: their
// folders, then the file opened.
import init, {
  Session, crashParse, discBoot, discLayout, discPrefix, discRecords, discRoot, discSort, packageBundle, packageDiscover,
  packageHeader, packagePlan, zipDecompressZstandard, zipFindDirectory, zipParseDirectory, zipZip64Directory,
} from './pkg/binviz_wasm.js';
import type { BaselineSource, BinaryHeader, BundleInfo, DebugMapObject, DebugMapReport, DiscFile, PackageBinary, PackageInfo, PackageSource, Summary } from './types';

const ready = init();
let session: Session | null = null;

interface Request {
  id: number;
  method: string;
  args: unknown[];
}

const post = (msg: unknown, transfer: Transferable[] = []) =>
  (self as unknown as { postMessage(m: unknown, t: Transferable[]): void }).postMessage(msg, transfer);

/** Chunk size for copying a file into WebAssembly memory. */
const CHUNK = 16 * 1024 * 1024;

/**
 * Copies a Blob (a File, usually) straight into a buffer the session
 * allocated in WebAssembly memory, a chunk at a time: the whole file is never
 * held in JavaScript, and never copied twice.
 */
async function copyInto(memory: WebAssembly.Memory, ptr: number, blob: Blob, onProgress: (f: number) => void) {
  for (let off = 0; off < blob.size; off += CHUNK) {
    const chunk = new Uint8Array(await blob.slice(off, Math.min(blob.size, off + CHUNK)).arrayBuffer());
    // Re-create the view each time: a growing memory detaches old buffers.
    new Uint8Array(memory.buffer, ptr + off, chunk.length).set(chunk);
    onProgress(Math.min(1, (off + chunk.length) / Math.max(1, blob.size)));
  }
}

// --- CD images ------------------------------------------------------------------------

interface DiscLayout {
  sector: bigint;
  data: bigint;
}

/** The CD image open, and its files (a member's index is its place here). */
let disc: { blob: Blob; name: string; layout: DiscLayout; files: DiscFile[] } | null = null;

/** `count` logical sectors from `lba`: 2048 bytes each, wherever the image keeps them. */
async function readSectors(blob: Blob, layout: DiscLayout, lba: number, count: number): Promise<Uint8Array> {
  const sector = Number(layout.sector);
  const data = Number(layout.data);
  const raw = new Uint8Array(await blob.slice(lba * sector, (lba + count) * sector).arrayBuffer());
  if (sector === 2048) return raw;
  const out = new Uint8Array(count * 2048);
  for (let i = 0; i < count && i * sector + data < raw.length; i++) out.set(raw.subarray(i * sector + data, i * sector + data + 2048), i * 2048);
  return out;
}

/** Lists a CD image's files, reading only its folders: a container, what it boots first. */
async function openDisc(name: string, blob: Blob, layout: DiscLayout) {
  const root = discRoot(await readSectors(blob, layout, 16, 1)) as [string, DiscFile] | null;
  if (!root) throw new Error('no ISO 9660 volume on the disc');
  const [volume, top] = root;
  const files: DiscFile[] = [];
  const folders = [top];
  const seen = new Set<number>();
  while (folders.length > 0 && files.length < 20_000) {
    const d = folders.pop()!;
    const lba = Number(d.lba);
    if (seen.has(lba)) continue;
    seen.add(lba);
    const count = Math.min(64, Math.max(1, Math.ceil(Number(d.size) / 2048)));
    for (const f of discRecords(await readSectors(blob, layout, lba, count), d.path) as DiscFile[]) {
      if (f.dir) folders.push(f);
      files.push(f);
    }
  }
  const cnf = files.find((f) => !f.dir && f.path.toUpperCase() === 'SYSTEM.CNF');
  const boot = cnf && cnf.size < 4096n ? (discBoot((await readSectors(blob, layout, Number(cnf.lba), 2)).subarray(0, Number(cnf.size))) ?? undefined) : undefined;
  const sorted = (discSort(files, boot) as DiscFile[]).filter((f) => !f.dir);
  disc = { blob, name, layout, files: sorted };
  const members = sorted.map((f, index) => ({
    index,
    name: f.path,
    offset: f.lba * layout.sector + layout.data,
    size: f.size,
    arch: boot && boot.toUpperCase() === f.path.toUpperCase() ? 'boots first' : undefined,
    contiguous: layout.sector === 2048n,
  }));
  return { kind: 'container', name, info: { kind: volume ? `CD image (${volume})` : 'CD image', fileSize: BigInt(blob.size), members } };
}

/** Opens a file of the CD image open: its sectors read, then the binary. */
async function openDiscFile(index: number) {
  const d = disc!;
  const f = d.files[index];
  if (!f) throw new Error('no such file on the disc');
  const bytes = await readSectors(d.blob, d.layout, Number(f.lba), Math.max(1, Math.ceil(Number(f.size) / 2048)));
  return session!.open(`${d.name} [${f.path}]`, bytes.subarray(0, Number(f.size)));
}

// --- Folders of binaries ------------------------------------------------------

interface ZipEntry { name: string; method: number; compressedSize: bigint; size: bigint; crc32: number; headerOffset: bigint; isDir: boolean; isSymlink: boolean }
interface Directory { offset: bigint; size: bigint; entries: bigint; zip64Record?: bigint }

/** A file in the folder: its listing, and how to read its bytes. */
interface Entry {
  path: string;
  size: number;
  compressedSize?: number;
  crc32?: number;
  stream(max?: number): Promise<ReadableStream<Uint8Array>>;
  /** Its bytes as a Blob, when they are stored as they are (not compressed). */
  blob(): Promise<Blob | null>;
}

/** How much of a file's start tells what binary it is. */
const HEADER_BYTES = 64 * 1024;
/** Zips inside zips (and inside those) are opened up to this deep. */
const NESTED_ZIPS = 2;

/** The open folder's files, and its binaries loaded in the session (current or kept aside) and with their debug file attached. */
let entries: Entry[] = [];
const loaded = new Set<number>();
const attached = new Set<number>();
/** Binaries whose debug map was looked at. */
const mapped = new Set<number>();
let current = -1;
/** What the last scan found, until it is opened. */
let scanned: { info: PackageInfo; entries: Entry[] } | null = null;

const IGNORED = /(^|\/)(__MACOSX|\.git|\.DS_Store)(\/|$)/;

async function zipEntries(blob: Blob, prefix: string): Promise<Entry[]> {
  const tailStart = Math.max(0, blob.size - 65_557 - 20);
  let dir = zipFindDirectory(new Uint8Array(await blob.slice(tailStart).arrayBuffer())) as Directory;
  if (dir.zip64Record !== undefined && dir.zip64Record !== null) {
    const at = Number(dir.zip64Record);
    dir = zipZip64Directory(new Uint8Array(await blob.slice(at, at + 56).arrayBuffer())) as Directory;
  }
  const cd = new Uint8Array(await blob.slice(Number(dir.offset), Number(dir.offset + dir.size)).arrayBuffer());
  const list = zipParseDirectory(cd) as ZipEntry[];
  const dataStart = async (e: ZipEntry) => {
    const at = Number(e.headerOffset);
    const header = new DataView(await blob.slice(at, at + 30).arrayBuffer());
    if (header.getUint32(0, true) !== 0x04034b50) throw new Error(`${e.name}: bad local header`);
    return at + 30 + header.getUint16(26, true) + header.getUint16(28, true);
  };
  return list
    .filter((e) => !e.isDir && !e.isSymlink && !IGNORED.test(e.name))
    .map((e) => ({
      path: prefix + e.name,
      size: Number(e.size),
      compressedSize: Number(e.compressedSize),
      crc32: e.crc32,
      async stream(max = Number(e.size)) {
        const start = await dataStart(e);
        const compressed = blob.slice(start, start + Number(e.compressedSize));
        const raw = compressed.stream();
        if (e.method === 0) return raw;
        if (e.method === 8) return raw.pipeThrough(new DecompressionStream('deflate-raw')) as ReadableStream<Uint8Array>;
        if (e.method === 93) {
          const bytes = zipDecompressZstandard(
            new Uint8Array(await compressed.arrayBuffer()),
            BigInt(Math.min(Number(e.size), max)),
          );
          return new Blob([bytes.buffer as ArrayBuffer]).stream();
        }
        throw new Error(`${e.name}: unsupported compression method ${e.method}`);
      },
      async blob() {
        if (e.method !== 0) return null;
        const start = await dataStart(e);
        return blob.slice(start, start + Number(e.size));
      },
    }));
}

function folderEntries(files: { path: string; file: File }[]): Entry[] {
  return files.map(({ path, file }) => ({
    path,
    size: file.size,
    stream: async () => file.stream(),
    blob: async () => file,
  }));
}

/** Up to `max` bytes of an entry. */
async function readBytes(e: Entry, max = Infinity): Promise<Uint8Array> {
  const whole = await e.blob();
  if (whole) return new Uint8Array(await whole.slice(0, Math.min(whole.size, max)).arrayBuffer());
  const reader = (await e.stream(max)).getReader();
  const parts: Uint8Array[] = [];
  let n = 0;
  while (n < max) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value);
    n += value.length;
  }
  if (n >= max) await reader.cancel().catch(() => {});
  const out = new Uint8Array(Math.min(n, max));
  let at = 0;
  for (const p of parts) {
    const take = Math.min(p.length, out.length - at);
    out.set(p.subarray(0, take), at);
    at += take;
    if (at >= out.length) break;
  }
  return out;
}

const isZip = (b: Uint8Array) => b[0] === 0x50 && b[1] === 0x4b && ((b[2] === 3 && b[3] === 4) || (b[2] === 5 && b[3] === 6));

const listing = (list: Entry[]) => list.map((e) => ({ path: e.path, size: e.size, compressedSize: e.compressedSize, crc32: e.crc32 }));

/**
 * Looks through folders and zips for binaries: reads the first bytes of every
 * file that might be one, opens zips found inside (as folders), reads the
 * Info.plist files, and pairs binaries with their debug files.
 */
async function scan(sources: PackageSource[], status: (s: string) => void, progress: (f: number) => void) {
  status('Reading the file list…');
  let list = (await Promise.all(sources.map((s) => (s.kind === 'zip' ? zipEntries(s.blob, '') : Promise.resolve(folderEntries(s.files)))))).flat();
  const headers = new Map<string, BinaryHeader>();
  const zips = new Set<string>();
  for (let depth = 0; depth <= NESTED_ZIPS; depth++) {
    const plan = packagePlan(listing(list)) as { headers: number[]; plists: number[] };
    const todo = plan.headers.filter((i) => !headers.has(list[i].path) && !zips.has(list[i].path));
    status('Looking for binaries…');
    const inner = new Map<number, Entry[]>();
    for (let k = 0; k < todo.length; k++) {
      const e = list[todo[k]];
      const bytes = await readBytes(e, HEADER_BYTES);
      progress((k + 1) / todo.length);
      if (isZip(bytes)) {
        zips.add(e.path);
        if (depth === NESTED_ZIPS) continue;
        try {
          // A stored zip is read in place; a compressed one is inflated first.
          const blob = (await e.blob()) ?? (await new Response(await e.stream()).blob());
          inner.set(todo[k], await zipEntries(blob, `${e.path}/`));
        } catch {
          /* not a readable zip: it stays a file */
        }
        continue;
      }
      const h = packageHeader(bytes) as BinaryHeader | null;
      if (h) headers.set(e.path, h);
    }
    if (inner.size === 0) break;
    list = list.flatMap((e, i) => inner.get(i) ?? [e]);
  }
  const plan = packagePlan(listing(list)) as { headers: number[]; plists: number[] };
  const bundles: [number, BundleInfo][] = [];
  for (const i of plan.plists) {
    const b = packageBundle(await readBytes(list[i])) as BundleInfo | null;
    if (b) bundles.push([i, b]);
  }
  const found: [number, BinaryHeader][] = [];
  list.forEach((e, i) => {
    const h = headers.get(e.path);
    if (h) found.push([i, h]);
  });
  const name = sources.map((s) => s.name).join(' + ');
  const container = sources.every((s) => s.kind === 'folder') ? 'folder' : 'zip';
  const info = packageDiscover(name, container, listing(list), found, bundles) as PackageInfo;
  return { info, entries: list };
}

/** Streams an entry into a fresh input buffer in WebAssembly memory. */
async function stream(memory: WebAssembly.Memory, e: Entry, onProgress: (f: number) => void) {
  const ptr = session!.beginDebugInput(e.size);
  const reader = (await e.stream()).getReader();
  let at = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (at + value.length > e.size) throw new Error(`${e.path}: longer than its directory says`);
    new Uint8Array(memory.buffer, ptr + at, value.length).set(value);
    at += value.length;
    onProgress(at / Math.max(1, e.size));
  }
  if (at !== e.size) throw new Error(`${e.path}: ${at} bytes read, ${e.size} expected`);
}

/** Only one folder operation at a time: they share the input buffer. */
let lock: Promise<unknown> = Promise.resolve();
function exclusive<T>(f: () => Promise<T>): Promise<T> {
  const run = lock.then(f, f);
  lock = run.catch(() => {});
  return run;
}

function info(): PackageInfo {
  return session!.packageInfo() as PackageInfo;
}

/** Loads binary `index` (as current or aside) and attaches its debug file. */
async function loadBinary(memory: WebAssembly.Memory, index: number, makeCurrent: boolean, status: (s: string) => void, progress: (f: number) => void) {
  const b = info().binaries[index];
  if (!loaded.has(index)) {
    status(`Reading ${b.name}…`);
    await stream(memory, entries[b.file], progress);
    session!.packageLoad(index, makeCurrent);
    loaded.add(index);
  } else if (makeCurrent) {
    session!.packageSelect(index);
  }
  if (makeCurrent) current = index;
  // Loading can pair it (through its debug link): look again.
  const now = info();
  const debug = now.binaries[index].debug;
  if (debug !== undefined && debug !== null && !attached.has(index)) {
    const d = now.debugFiles[debug];
    status(`Reading ${b.name}’s debug file…`);
    try {
      await stream(memory, entries[d.file], progress);
      session!.packageAttach(index, d.path);
    } catch (e) {
      post({ status: `Couldn’t attach ${d.path}: ${e instanceof Error ? e.message : e}` });
    }
    attached.add(index);
  }
  // Built without dsymutil: its DWARF is in the object files its debug map names, if the folder has them.
  if (!mapped.has(index)) {
    mapped.add(index);
    const map = session!.packageDebugMapNeeded(index) as DebugMapObject[];
    if (map.length) {
      status(`Reading ${b.name}’s object files…`);
      const found = await addDebugObjects(map, entries, archOf(b), (e) => readBytes(e));
      if (found) {
        try {
          post({ status: describeReport(session!.packageDebugMapLink(index) as DebugMapReport) });
        } catch (e) {
          post({ status: `${b.name}: ${e instanceof Error ? e.message : e}` });
        }
      }
    }
  }
}

/** The last part of a path. */
const baseName = (path: string) => path.slice(path.lastIndexOf('/') + 1);

/** A folder binary's architecture, as its header says (the first slice's). */
const archOf = (b: PackageBinary) => b.ids[0]?.arch ?? '';

/** A debug map object's file name: `main.o`, or a member's library, `libfoo.a`. */
const objectFile = (o: DebugMapObject) => baseName(o.path.replaceAll('\\', '/'));

/**
 * Of the files named `name`, the one most likely built for `arch`: Xcode keeps
 * each architecture's objects in a folder named after it (…/Objects-normal/arm64).
 */
function pickObject<T extends { path: string }>(candidates: T[] | undefined, arch: string): T | undefined {
  if (!candidates?.length) return undefined;
  const dirs = /arm64|aarch64/i.test(arch) ? ['arm64e', 'arm64'] : /x86.64/i.test(arch) ? ['x86_64h', 'x86_64'] : [];
  return candidates.find((c) => dirs.some((d) => c.path.includes(`/${d}/`))) ?? candidates[0];
}

/**
 * Hands the object files a debug map names to the session, from `files` (by
 * name); returns how many were found.
 */
async function addDebugObjects<T extends { path: string }>(map: DebugMapObject[], files: T[], arch: string, read: (f: T) => Promise<Uint8Array>) {
  const byName = new Map<string, T[]>();
  for (const f of files) {
    const n = baseName(f.path);
    if (!n.endsWith('.o') && !n.endsWith('.a')) continue;
    byName.set(n, [...(byName.get(n) ?? []), f]);
  }
  let found = 0;
  const done = new Set<string>();
  for (const o of map) {
    const name = objectFile(o);
    if (done.has(name)) continue;
    done.add(name);
    const f = pickObject(byName.get(name), arch);
    if (!f) continue;
    session!.debugMapAdd(name, await read(f));
    found++;
  }
  return found;
}

const describeReport = (r: DebugMapReport) =>
  `DWARF from ${r.linked} of ${r.objects} object files${r.missing.length ? ` (${r.missing.length} not found)` : ''}${r.failed.length ? ` (${r.failed.length} unusable)` : ''}`;

/** Frees binaries that aren't current once they (with their debug files) take much memory. */
function trim() {
  const pkg = info();
  for (const i of [...loaded]) {
    if (i === current) continue;
    const big = Number(pkg.binaries[i].size) > 64 * 1024 * 1024 || attached.has(i);
    if (big) {
      session!.packageUnload(i);
      loaded.delete(i);
      attached.delete(i);
      mapped.delete(i);
    }
  }
}

const packageMethods: Record<string, (memory: WebAssembly.Memory, id: number, args: unknown[]) => Promise<unknown>> = {
  async scanFolder(_memory, id, args) {
    const [sources] = args as [PackageSource[]];
    scanned = await scan(sources, (s) => post({ id, status: s }), (f) => post({ id, progress: f }));
    return scanned.info;
  },

  async openScanned(memory, id) {
    if (!scanned) throw new Error('nothing scanned to open');
    session!.packageOpen(scanned.info);
    entries = scanned.entries;
    scanned = null;
    loaded.clear();
    attached.clear();
    mapped.clear();
    current = -1;
    if (info().binaries.length === 0) return { info: info(), opened: null };
    await loadBinary(memory, 0, true, (s) => post({ id, status: s }), (f) => post({ id, progress: f }));
    post({ id, progress: -1 });
    return { info: info(), opened: session!.packageSelect(0) };
  },

  /** A scanned file attached to the open binary as its debug file. */
  async attachScanned(memory, id, args) {
    const [file, name] = args as [number, string];
    const e = scanned?.entries[file];
    if (!e) throw new Error('nothing scanned to attach');
    await stream(memory, e, (f) => post({ id, progress: f }));
    return session!.attachInput(name);
  },

  /** Reads an earlier build to compare sizes with: a binary, or folders and zips (each binary with its debug file). */
  async compareWith(memory, id, args) {
    const [source] = args as [BaselineSource];
    const status = (s: string) => post({ id, status: s });
    const progress = (f: number) => post({ id, progress: f });
    if (source.kind === 'file') {
      status(`Reading ${source.name}…`);
      const ptr = session!.beginDebugInput(source.blob.size);
      await copyInto(memory, ptr, source.blob, progress);
      session!.baselineBinary(source.name);
      return 'binary';
    }
    const other = await scan(source.sources, status, progress);
    session!.baselineFolder(listing(other.entries), other.info);
    const n = Math.min(other.info.binaries.length, 300);
    for (let i = 0; i < n; i++) {
      const b = other.info.binaries[i];
      status(`Reading ${b.name} of the earlier build (${i + 1} of ${n})…`);
      try {
        await stream(memory, other.entries[b.file], progress);
        session!.baselineLoad();
        if (b.debug !== undefined && b.debug !== null) {
          const d = other.info.debugFiles[b.debug];
          await stream(memory, other.entries[d.file], progress);
          try {
            session!.baselineAttach(d.path);
          } catch {
            /* names come from the binary alone */
          }
        }
        session!.baselineAdd(b.path, b.name);
      } catch (e) {
        post({ status: `${b.path}: ${e instanceof Error ? e.message : e}` });
      }
    }
    return 'folder';
  },

  /** What changed in size from the earlier build to what is open (reading each binary of an open folder). */
  async sizeDiff(memory, id, args) {
    const [top] = args as [number];
    if (session!.baselineKind() === 'folder' && session!.packageInfo()) {
      session!.packageSnapshotBegin(listing(entries));
      const pkg = info();
      const n = Math.min(pkg.binaries.length, 300);
      for (let i = 0; i < n; i++) {
        post({ id, status: `Reading ${pkg.binaries[i].name} (${i + 1} of ${n})…` });
        const here = !loaded.has(i);
        try {
          await loadBinary(memory, i, false, () => {}, () => {});
          session!.packageSnapshotAdd(i);
        } catch (e) {
          post({ status: `${pkg.binaries[i].path}: ${e instanceof Error ? e.message : e}` });
        }
        if (here && i !== current) {
          session!.packageUnload(i);
          loaded.delete(i);
          attached.delete(i);
        }
      }
      return { kind: 'folder', diff: session!.sizeDiffFolder(top) };
    }
    return { kind: 'binary', diff: session!.sizeDiffBinary(top) };
  },

  async crashParse(_memory, _id, args) {
    const [text] = args as [string];
    return crashParse(text);
  },

  /** Symbolicates a crash report with what is open: the folder's binaries it needs are loaded (with their debug files) first. */
  async symbolicateCrash(memory, id, args) {
    const [text] = args as [string];
    if (session!.packageInfo()) {
      for (const i of session!.crashNeeds(text)) {
        await loadBinary(memory, i, false, (s) => post({ id, status: s }), (f) => post({ id, progress: f }));
      }
    }
    return session!.symbolicate(text);
  },

  async sniff(_memory, _id, args) {
    const [blob] = args as [Blob];
    return packageHeader(new Uint8Array(await blob.slice(0, HEADER_BYTES).arrayBuffer()));
  },

  async selectPackageBinary(memory, id, args) {
    const [index] = args as [number];
    await loadBinary(memory, index, true, (s) => post({ id, status: s }), (f) => post({ id, progress: f }));
    trim();
    return { info: info(), opened: session!.packageSelect(index) };
  },

  /** The size report of every binary, with debug files attached; each loaded in turn and let go again. */
  async analyzePackage(memory, id, args) {
    const [top] = args as [number];
    const pkg = info();
    const reports: { index: number; report: unknown }[] = [];
    for (let i = 0; i < pkg.binaries.length; i++) {
      post({ id, status: `Analyzing ${pkg.binaries[i].name} (${i + 1} of ${pkg.binaries.length})…` });
      const here = !loaded.has(i);
      try {
        await loadBinary(memory, i, false, () => {}, () => {});
        reports.push({ index: i, report: session!.packageSizeReport(i, top) });
      } catch (e) {
        post({ status: `${pkg.binaries[i].path}: ${e instanceof Error ? e.message : e}` });
      }
      if (here && i !== current) {
        session!.packageUnload(i);
        loaded.delete(i);
        attached.delete(i);
        mapped.delete(i);
      }
    }
    return { info: info(), reports };
  },

  /** Links the open binary's debug map from the object files of a chosen folder. */
  async linkDebugMap(_memory, id, args) {
    const [files] = args as [{ path: string; file: File }[]];
    const map = session!.debugMap() as DebugMapObject[];
    const summary = session!.summary() as Summary;
    post({ id, status: 'Reading the object files…' });
    const found = await addDebugObjects(map, files, summary.arch, async (f) => new Uint8Array(await f.file.arrayBuffer()));
    if (!found) throw new Error(`None of the ${map.length} object files the debug map names is in that folder`);
    const report = session!.debugMapLink() as DebugMapReport;
    return { report, summary: session!.summary() };
  },

  /** A file's bytes (a stored zip entry's, or a dropped file's) as a Blob, if it has one. */
  async packageFileBlob(_memory, _id, args) {
    const [file] = args as [number];
    return entries[file]?.blob() ?? null;
  },
};

self.onmessage = async (event: MessageEvent<Request>) => {
  const { id, method, args } = event.data;
  try {
    const wasm = await ready;
    session ??= new Session();
    let result: unknown;
    const [discName, discBlob] = args as [string, Blob];
    const layout =
      method === 'openBlob' && discBlob.size > discPrefix() ? (discLayout(new Uint8Array(await discBlob.slice(0, discPrefix()).arrayBuffer())) as DiscLayout | null) : null;
    if (method === 'openBlob') disc = null;
    if (layout) {
      result = await exclusive(() => openDisc(discName, discBlob, layout));
    } else if (method === 'openMember' && disc) {
      result = await exclusive(() => openDiscFile(args[0] as number));
    } else if (method === 'openBlob' || method === 'attachBlob') {
      const [name, blob] = args as [string, Blob];
      result = await exclusive(async () => {
        const ptr = method === 'openBlob' ? session!.beginInput(blob.size) : session!.beginDebugInput(blob.size);
        await copyInto(wasm.memory, ptr, blob, (f) => post({ id, progress: f }));
        post({ id, progress: -1 });
        if (method === 'openBlob') {
          entries = [];
          loaded.clear();
          attached.clear();
          mapped.clear();
          current = -1;
        }
        return method === 'openBlob' ? session!.openInput(name) : session!.attachInput(name);
      });
    } else if (method in packageMethods) {
      result = await exclusive(() => packageMethods[method](wasm.memory, id, args));
    } else {
      const fn = (session as unknown as Record<string, unknown>)[method];
      if (typeof fn !== 'function') throw new Error(`unknown method ${method}`);
      result = (fn as (...a: unknown[]) => unknown).apply(session, args);
    }
    const transfer = ArrayBuffer.isView(result) ? [result.buffer as ArrayBuffer] : [];
    post({ id, result }, transfer);
  } catch (err) {
    post({ id, error: err instanceof Error ? err.message : String(err) });
  }
};
