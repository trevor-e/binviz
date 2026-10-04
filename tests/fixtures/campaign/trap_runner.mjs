import { readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
let exports;
const word = v => '0x' + (v >>> 0).toString(16);
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.op === 'stop') break;
  if (request.op === 'init') {
    ({ instance: { exports } } = await WebAssembly.instantiate(readFileSync(request.artifacts[request.profile?.moduleArtifact ?? 'wasm-module'])));
    console.log(JSON.stringify({ ready: true, version: 'trap-observation-1', architecture: 'wasm32', runnerKind: 'wasm-runtime-trap' }));
    continue;
  }
  exports.reset_cursor();
  let result, exception;
  try { result = exports.checked_divide(parseInt(request.arguments[0], 16)); }
  catch (error) {
    if (!(error instanceof WebAssembly.RuntimeError) || !error.message.includes('unreachable')) throw error;
    exception = { kind: 'checked-divide-trap', operation: 'checked-divide', guestPc: null, checkpoint: 'exception-point' };
  }
  const address = exports.cursor.value;
  const value = new DataView(exports.memory.buffer).getUint32(address, true);
  const bytes = [...new Uint8Array(exports.memory.buffer, address, 4)];
  console.log(JSON.stringify({ id: request.id, execution: {
    status: exception ? 'exception' : 'executed', instructions: 0, returnWord: exception ? null : word(result),
    exception, ram: [{ id: 'cursor', start: word(0x80000000 + address), data: '0x' + bytes.map(b => b.toString(16).padStart(2, '0')).join('') }],
    registers: {}, events: [{ kind: exception ? 'exception' : 'return', pc: '0x0', provider: 'checked-divide' }],
    checkpoints: { 'exception-point': { cursor: word(value) } }, coverage: ['checked-divide:zero-divisor']
  }}));
}
