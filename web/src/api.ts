// Typed client for the WebAssembly session running in a worker.
import type {
  Annotation, Attribution, AttributedRange, AttributionMode, ContainerInfo, Coverage, DieDetails, DieSummary,
  Disassembly, DwarfSummary, Export, HitKind, Import, Inspection, LineProgramInfo, LineRange, LineRow, MapStatus,
  Opened, PathEntry, RegionInfo, RegionKind, Resolved, SearchResults, Section, Segment, SourceFile, Span,
  StringPage, Summary, SymbolPage, SymbolQuery, Sym, UnitInfo,
} from './types';

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

export class Api {
  private worker: Worker;
  private next = 1;
  private pending = new Map<number, Pending>();
  /** Number of calls in flight, for the busy indicator. */
  inFlight = 0;
  onBusyChange: (busy: boolean) => void = () => {};

  constructor() {
    this.worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<{ id: number; result?: unknown; error?: string }>) => {
      const p = this.pending.get(e.data.id);
      if (!p) return;
      this.pending.delete(e.data.id);
      this.setBusy(-1);
      if (e.data.error !== undefined) p.reject(new Error(e.data.error));
      else p.resolve(e.data.result);
    };
  }

  private setBusy(delta: number) {
    const was = this.inFlight > 0;
    this.inFlight += delta;
    if (was !== this.inFlight > 0) this.onBusyChange(this.inFlight > 0);
  }

  call<T>(method: string, ...args: unknown[]): Promise<T> {
    const id = this.next++;
    this.setBusy(1);
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.worker.postMessage({ id, method, args });
    });
  }

  open(name: string, bytes: Uint8Array) { return this.call<Opened>('open', name, bytes); }
  openMember(index: number) { return this.call<Opened>('openMember', index); }
  attachDebug(name: string, bytes: Uint8Array) { return this.call<Opened>('attachDebug', name, bytes); }
  bytes() { return this.call<Uint8Array>('bytes'); }
  summary() { return this.call<Summary>('summary'); }
  sections() { return this.call<Section[]>('sections'); }
  segments() { return this.call<Segment[]>('segments'); }
  imports() { return this.call<Import[]>('imports'); }
  exports() { return this.call<Export[]>('exports'); }
  regions(parent?: number) { return this.call<RegionInfo[]>('regions', parent); }
  regionEntries(id: number, first: number, count: number) { return this.call<PathEntry[]>('regionEntries', id, first, count); }
  spans(start: bigint, end: bigint) { return this.call<Span[]>('spans', start, end); }
  fileMap(buckets: number) { return this.call<RegionKind[]>('fileMap', buckets); }
  entropyMap(buckets: number) { return this.call<Float32Array>('entropyMap', buckets); }
  composition() { return this.call<[RegionKind, bigint][]>('composition'); }
  inspectOffset(offset: bigint) { return this.call<Inspection>('inspectOffset', offset); }
  inspectAddress(address: bigint) { return this.call<Inspection>('inspectAddress', address); }
  addressToOffset(address: bigint) { return this.call<bigint | undefined>('addressToOffset', address); }
  offsetToAddress(offset: bigint) { return this.call<bigint | undefined>('offsetToAddress', offset); }
  symbols(query: SymbolQuery) { return this.call<SymbolPage>('symbols', query); }
  symbol(index: number) { return this.call<Sym | undefined>('symbol', index); }
  functions() { return this.call<[bigint, bigint, string][]>('functions'); }
  disassemble(start: bigint, end: bigint, limit: number) { return this.call<Disassembly>('disassemble', start, end, limit); }
  disassembleFunction(address: bigint, limit: number) { return this.call<Disassembly>('disassembleFunction', address, limit); }
  resolve(query: string) { return this.call<Resolved>('resolve', query); }

  search(query: string, perKind: number, only?: HitKind) { return this.call<SearchResults>('search', query, perKind, only); }
  prepareSearch() { return this.call<void>('prepareSearch'); }
  strings(filter: string, offset: number, limit: number) { return this.call<StringPage>('strings', filter, offset, limit); }
  attribution(mode: AttributionMode) { return this.call<Attribution | undefined>('attribution', mode); }
  attributedRanges(mode: AttributionMode, id: number) { return this.call<AttributedRange[]>('attributedRanges', mode, id); }
  coverage(maxGaps: number) { return this.call<Coverage>('coverage', maxGaps); }
  coverageStrip(section: number, buckets: number) { return this.call<MapStatus[]>('coverageStrip', section, buckets); }
  coverageMap(buckets: number) { return this.call<MapStatus[]>('coverageMap', buckets); }
  setAnnotations(list: Annotation[]) { return this.call<Summary>('setAnnotations', list); }
  annotations() { return this.call<Annotation[]>('annotations'); }

  dwarfSummary() { return this.call<DwarfSummary | null>('dwarfSummary'); }
  dwarfUnits() { return this.call<UnitInfo[]>('dwarfUnits'); }
  unitRoot(unit: number) { return this.call<DieSummary | undefined>('unitRoot', unit); }
  dieChildren(unit: number, offset?: bigint) { return this.call<DieSummary[]>('dieChildren', unit, offset); }
  die(unit: number, offset: bigint) { return this.call<DieDetails | undefined>('die', unit, offset); }
  dieAt(address: bigint) { return this.call<[number, bigint] | undefined>('dieAt', address); }
  functionDieAt(address: bigint) { return this.call<[number, bigint] | undefined>('functionDieAt', address); }
  dieSearch(query: string, limit: number) { return this.call<DieSummary[]>('dieSearch', query, limit); }
  lineProgram(unit: number) { return this.call<LineProgramInfo | undefined>('lineProgram', unit); }
  lineRows(unit: number, first: number, count: number) { return this.call<LineRow[]>('lineRows', unit, first, count); }
  sourceFiles() { return this.call<SourceFile[]>('sourceFiles'); }
  fileLines(file: number) { return this.call<LineRange[]>('fileLines', file); }
  embeddedSource(file: number) { return this.call<string | undefined>('embeddedSource', file); }
  fileLineCounts() { return this.call<Uint32Array>('fileLineCounts'); }
}

export type { ContainerInfo };
