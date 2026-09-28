// Runs the WebAssembly session off the main thread so parsing large binaries
// never freezes the UI. Messages are {id, method, args} → {id, result | error}.
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

self.onmessage = async (event: MessageEvent<Request>) => {
  const { id, method, args } = event.data;
  try {
    await ready;
    session ??= new Session();
    const fn = (session as unknown as Record<string, unknown>)[method];
    if (typeof fn !== 'function') throw new Error(`unknown method ${method}`);
    const result = (fn as (...a: unknown[]) => unknown).apply(session, args);
    const transfer = ArrayBuffer.isView(result) ? [result.buffer as ArrayBuffer] : [];
    post({ id, result }, transfer);
  } catch (err) {
    post({ id, error: err instanceof Error ? err.message : String(err) });
  }
};
