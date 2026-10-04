// Real WebAssembly execution; source-instrumented device observations.
import { readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
let exports;
const word = v => '0x' + (v >>> 0).toString(16);
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.op === 'stop') break;
  let reply;
  if (request.op === 'init') {
    ({ instance: { exports } } = await WebAssembly.instantiate(readFileSync(request.artifacts['wasm-module'])));
    reply = { ready: true, version: 'synthetic-wasm-1', architecture: 'wasm32', coverageKind: 'instrumented-source', providerProfile: 'real WASM compute; controlled synthetic device' };
  } else {
    const value = Number(BigInt(request.arguments[0]));
    const result = exports.compute(value);
    const events = [{ kind: 'call', pc: '0x0', provider: 'compute', profile: 'real', arguments: [word(value)], address: null, value: null, delaySlot: false }];
    for (let i = 0; i < exports.get_trace_count(); i++) events.push({ kind: 'device', pc: '0x4', provider: null, profile: 'controlled', arguments: [], address: '0x1', value: word(exports.get_trace(i)), delaySlot: false });
    events.push({ kind: 'return', pc: '0x8', provider: null, profile: null, arguments: [], address: null, value: word(result), delaySlot: false });
    reply = { id: request.id, execution: { status: 'executed', reason: null, instructions: 0, returnWord: word(result), ram: [{ id: 'state', start: '0x0', data: '0x' + [exports.get_ram(0), exports.get_ram(1)].map(v => v.toString(16).padStart(2, '0')).join('') }], registers: {}, events, checkpoints: { 'after-device': { count: exports.get_trace_count() } }, coverage: ['compute:short-extension'], localObjects: [] } };
  }
  console.log(JSON.stringify(reply));
}
