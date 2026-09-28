// Runs the WebAssembly session off the main thread so parsing large binaries
// never freezes the UI. Messages are {id, method, args} → {id, result | error},
// with {id, progress} / {id, status} updates while files are being read in.
//
// Folders and zips of binaries are read here too: zip entries are inflated
// in the worker (zips inside open like folders), and binaries are streamed
// straight into WebAssembly memory, one at a time.
import init, {
  Session, packageBundle, packageDiscover, packageHeader, packagePlan, zipDecompressZstandard, zipFindDirectory,
  zipParseDirectory, zipZip64Directory,
} from './pkg/binviz_wasm.js';
import type { BinaryHeader, BundleInfo, PackageInfo, PackageSource } from './types';

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
}

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
      }
    }
    return { info: info(), reports };
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
    if (method === 'openBlob' || method === 'attachBlob') {
      const [name, blob] = args as [string, Blob];
      result = await exclusive(async () => {
        const ptr = method === 'openBlob' ? session!.beginInput(blob.size) : session!.beginDebugInput(blob.size);
        await copyInto(wasm.memory, ptr, blob, (f) => post({ id, progress: f }));
        post({ id, progress: -1 });
        if (method === 'openBlob') {
          entries = [];
          loaded.clear();
          attached.clear();
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
