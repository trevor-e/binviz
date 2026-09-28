// Runs the WebAssembly session off the main thread so parsing large binaries
// never freezes the UI. Messages are {id, method, args} → {id, result | error},
// with {id, progress} updates while a file is being read in.
import init, { Session } from './pkg/binviz_wasm.js';

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

self.onmessage = async (event: MessageEvent<Request>) => {
  const { id, method, args } = event.data;
  try {
    const wasm = await ready;
    session ??= new Session();
    let result: unknown;
    if (method === 'openBlob' || method === 'attachBlob') {
      const [name, blob] = args as [string, Blob];
      const ptr = method === 'openBlob' ? session.beginInput(blob.size) : session.beginDebugInput(blob.size);
      await copyInto(wasm.memory, ptr, blob, (f) => post({ id, progress: f }));
      post({ id, progress: -1 });
      result = method === 'openBlob' ? session.openInput(name) : session.attachInput(name);
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
