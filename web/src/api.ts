// Typed client for the WebAssembly session running in a worker.
import type {
  Annotation, Attribution, AttributedRange, AttributionMode, CallEdge, CallGraph, ContainerInfo, Coverage, DieDetails,
  DieSummary, Disassembly, DwarfSummary, Export, FunctionPage, FunctionSummary, HitKind, Import, Inspection,
  LineProgramInfo, LineRange, LineRow, MapStatus, Opened, PathEntry, PathStep, RefCounts, Reference, RefPage,
  RegionInfo, RegionKind, Resolved, SearchResults, Section, Segment, SourceFile, Span, StringPage, Summary,
  SymbolPage, SymbolQuery, Sym, UnitInfo,
} from './types';

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

export class Api {
  private worker: Worker;
  private next = 1;
  private pending = new Map<number, Pending>();
  /** Number of calls in flight, for the busy indicator. */
  inFlight = 0;
  onBusyChange: (busy: boolean) => void = () => {};
  /** Reading a file in: fraction done, or -1 once it is being parsed. */
  onProgress: (fraction: number) => void = () => {};

  constructor() {
    this.worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<{ id: number; result?: unknown; error?: string; progress?: number }>) => {
      if (e.data.progress !== undefined) {
        this.onProgress(e.data.progress);
        return;
      }
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

  /** Opens a file; the worker copies it into WebAssembly memory in chunks. */
  open(name: string, blob: Blob) { return this.call<Opened>('openBlob', name, blob); }
  openMember(index: number) { return this.call<Opened>('openMember', index); }
  memoryBytes() { return this.call<number>('memoryBytes'); }
  attachDebug(name: string, blob: Blob) { return this.call<Opened>('attachBlob', name, blob); }
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
  functionsPage(filter: string, offset: number, limit: number) { return this.call<FunctionPage>('functionsPage', filter, offset, limit); }
  functionIndex(filter: string, address: bigint) { return this.call<number | undefined>('functionIndex', filter, address); }
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

  xrefsSupported() { return this.call<boolean>('xrefsSupported'); }
  xrefsReady() { return this.call<boolean>('xrefsReady'); }
  /** Builds the reference index (once per file); returns counts by kind. */
  prepareXrefs() { return this.call<RefCounts>('prepareXrefs'); }
  referencesTo(lo: bigint, hi: bigint, offset: number, limit: number) { return this.call<RefPage>('referencesTo', lo, hi, offset, limit); }
  referenceCounts(lo: bigint, hi: bigint) { return this.call<RefCounts>('referenceCounts', lo, hi); }
  referencesFrom(lo: bigint, hi: bigint) { return this.call<Reference[]>('referencesFrom', lo, hi); }
  callers(address: bigint) { return this.call<CallEdge[]>('callers', address); }
  callees(address: bigint) { return this.call<CallEdge[]>('callees', address); }
  callGraph(center: bigint, up: number, down: number, fanout: number) { return this.call<CallGraph>('callGraph', center, up, down, fanout); }
  callPath(from: bigint, to: bigint, maxDepth: number) { return this.call<PathStep[] | undefined>('callPath', from, to, maxDepth); }
  functionSummary(address: bigint, limit: number) { return this.call<FunctionSummary | undefined>('functionSummary', address, limit); }

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
