// Typed client for the WebAssembly session running in a worker.
import type { Annotation, AttributedRange, Attribution, AttributionMode, BaselineSource, BinaryHeader, CallEdge, CallGraph, Comparison, ContainerInfo, Coverage, CrashReport, DebugMapObject, DebugMapReport, DieDetails, DiePage, DieSummary, Disassembly, DwarfCheck, DwarfProblem, DwarfSummary, Export, FunctionPage, FunctionSummary, HitKind, Import, Inspection, LabelFile, LabelFormat, LabelImport, LogSummary, PatchFormat, PatchState, FunctionDiff, DiffLine, LineProgramInfo, LineRange, LineRow, MapStatus, ObjcCounts, ObjcEntry, ObjcInterface, ObjcKind, Opened, PackageInfo, PackageSource, PathEntry, PathStep, RefCounts, Reference, RefPage, RegionInfo, RegionKind, RelativeSearch, Resolved, ScopeInfo, SearchResults, Section, Segment, SelectorUses, SizeReport, SourceFile, Span, StringPage, Summary, Sym, Symbolicated, SymbolPage, SymbolQuery, TableText, TagCount, UnitInfo } from './types';

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
  /** What a long operation is doing (reading a package's binaries…). */
  onStatus: (text: string) => void = () => {};
  /** Something the user should know that didn't stop an operation (a dSYM that didn't attach…). */
  onNotice: (text: string) => void = () => {};

  constructor() {
    this.worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<{ id?: number; result?: unknown; error?: string; progress?: number; status?: string }>) => {
      if (e.data.progress !== undefined) {
        this.onProgress(e.data.progress);
        return;
      }
      if (e.data.status !== undefined) {
        if (e.data.id === undefined) this.onNotice(e.data.status);
        else this.onStatus(e.data.status);
        return;
      }
      if (e.data.id === undefined) return;
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
  /** The object files the open binary's debug map names. */
  debugMap() { return this.call<DebugMapObject[]>('debugMap'); }
  /** Links the open binary's debug map from the object files in a chosen folder. */
  linkDebugMap(files: { path: string; file: File }[]) { return this.call<{ report: DebugMapReport; summary: Summary }>('linkDebugMap', files); }
  /** A little of the open binary's bytes (for binaries with no Blob of their own). */
  read(offset: bigint, count: number) { return this.call<Uint8Array>('read', offset, count); }
  sizeReport(top: number) { return this.call<SizeReport>('sizeReport', top); }

  /** Looks through folders and zips (and zips inside them) for binaries; nothing is opened yet. */
  scanFolder(sources: PackageSource[]) { return this.call<PackageInfo>('scanFolder', sources); }
  /** Opens what the last scan found: the first binary is loaded, with its debug file. */
  openScanned() { return this.call<{ info: PackageInfo; opened: Opened | null }>('openScanned'); }
  /** Attaches a file the last scan found to the open binary, as its debug file. */
  attachScanned(file: number, name: string) { return this.call<Opened>('attachScanned', file, name); }
  /** Reads an earlier build (a binary, or folders and zips) to compare sizes with; says which kind it was. */
  compareWith(source: BaselineSource) { return this.call<'binary' | 'folder'>('compareWith', source); }
  /** What changed in size from that build to what is open. */
  sizeDiff(top: number) { return this.call<Comparison>('sizeDiff', top); }
  clearBaseline() { return this.call<void>('baselineClear'); }
  /** A crash report read from text (Apple .crash or .ips, Android tombstone, stack trace), or null. */
  crashParse(text: string) { return this.call<CrashReport | null>('crashParse', text); }
  /** Symbolicates a crash report with what is open (a folder's binaries it needs are loaded first). */
  symbolicateCrash(text: string) { return this.call<Symbolicated>('symbolicateCrash', text); }
  /** What a file's header says it is (null: not a binary). */
  sniff(blob: Blob) { return this.call<BinaryHeader | null>('sniff', blob); }
  /** Makes another binary of the folder current, loading it (and its debug file) if needed. */
  selectPackageBinary(index: number) { return this.call<{ info: PackageInfo; opened: Opened }>('selectPackageBinary', index); }
  /** Every binary's size report, with its debug file attached. */
  analyzePackage(top: number) { return this.call<{ info: PackageInfo; reports: { index: number; report: SizeReport }[] }>('analyzePackage', top); }
  packageFileBlob(file: number) { return this.call<Blob | null>('packageFileBlob', file); }
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

  relativeSearch(word: string, width: number, limit: number) { return this.call<RelativeSearch>('relativeSearch', word, width, limit); }
  tableFromAlphabet(first: number, letter: string, width: number) { return this.call<string>('tableFromAlphabet', first, letter, width); }
  /** Reads text with this table file from now on; returns its entry count. */
  tableSet(text: string) { return this.call<number>('tableSet', text); }
  tableClear() { return this.call<void>('tableClear'); }
  tableChars() { return this.call<string[]>('tableChars'); }
  tableDecode(offset: bigint, len: number) { return this.call<TableText>('tableDecode', offset, len); }
  tableFind(text: string, limit: number) { return this.call<TableText[]>('tableFind', text, limit); }
  tableStrings(min: number, limit: number) { return this.call<TableText[]>('tableStrings', min, limit); }
  /** Reads the open ROM again with a code/data log (FCEUX's or Mesen's). */
  codeLog(bytes: Uint8Array) { return this.call<LogSummary>('codeLog', bytes); }
  codeLogSummary() { return this.call<LogSummary | null>('codeLogSummary'); }
  codeLogFlags(offset: bigint, count: number) { return this.call<Uint16Array>('codeLogFlags', offset, count); }
  readLabels(name: string, text: string) { return this.call<LabelImport>('readLabels', name, text); }
  labelFormats() { return this.call<LabelFormat[]>('labelFormats'); }
  writeLabels(format: LabelFormat) { return this.call<LabelFile[]>('writeLabels', format); }
  /** Applies a patch file to the open file (replacing any patch or edits). */
  patchApply(name: string, bytes: Uint8Array, limit: number) { return this.call<PatchState>('patchApply', name, bytes, limit); }
  /** Writes bytes into the patched file (the open file's, the first time). */
  patchEdit(offset: bigint, bytes: Uint8Array, limit: number) { return this.call<PatchState>('patchEdit', offset, bytes, limit); }
  patchState(limit: number) { return this.call<PatchState | null>('patchState', limit); }
  patchRead(offset: bigint, count: number) { return this.call<Uint8Array>('patchRead', offset, count); }
  patchTarget() { return this.call<Uint8Array>('patchTarget'); }
  patchCreate(format: PatchFormat) { return this.call<Uint8Array>('patchCreate', format); }
  patchClear() { return this.call<void>('patchClear'); }
  /** Functions compared: the earlier build with the open binary (`baseline`), or the open file with its patched copy (`patch`). */
  functionDiff(side: 'baseline' | 'patch') { return this.call<FunctionDiff>('functionDiff', side); }
  functionCode(side: 'baseline' | 'patch', old: bigint, updated: bigint) { return this.call<DiffLine[]>('functionCode', side, old, updated); }

  objcCounts() { return this.call<ObjcCounts>('objcCounts'); }
  objcEntries() { return this.call<ObjcEntry[]>('objcEntries'); }
  objcInterface(kind: ObjcKind, name: string) { return this.call<ObjcInterface | null>('objcInterface', kind, name); }
  /** Where a selector is implemented and who sends it (builds the reference index). */
  objcSelector(selector: string) { return this.call<SelectorUses | null>('objcSelector', selector); }

  dwarfSummary() { return this.call<DwarfSummary | null>('dwarfSummary'); }
  dwarfUnits() { return this.call<UnitInfo[]>('dwarfUnits'); }
  unitRoot(unit: number) { return this.call<DieSummary | undefined>('unitRoot', unit); }
  dieChildren(unit: number, offset?: bigint) { return this.call<DieSummary[]>('dieChildren', unit, offset); }
  die(unit: number, offset: bigint) { return this.call<DieDetails | undefined>('die', unit, offset); }
  dieAt(address: bigint) { return this.call<[number, bigint] | undefined>('dieAt', address); }
  functionDieAt(address: bigint) { return this.call<[number, bigint] | undefined>('functionDieAt', address); }
  dieSearch(query: string, limit: number) { return this.call<DieSummary[]>('dieSearch', query, limit); }
  dieSearchAll(query: string, limit: number) { return this.call<DieSummary[]>('dieSearchAll', query, limit); }
  listDies(unit: number, filter: string, name: string, offset: number, limit: number) { return this.call<DiePage>('listDies', unit, filter, name, offset, limit); }
  tagCounts(unit: number) { return this.call<TagCount[]>('tagCounts', unit); }
  dieAtOffset(offset: bigint) { return this.call<[number, bigint] | undefined>('dieAtOffset', offset); }
  scopeAt(address: bigint) { return this.call<ScopeInfo | undefined>('scopeAt', address); }
  dwarfCheck() { return this.call<DwarfCheck>('dwarfCheck'); }
  dwarfLoadProblems() { return this.call<DwarfProblem[]>('dwarfLoadProblems'); }
  lineProgram(unit: number) { return this.call<LineProgramInfo | undefined>('lineProgram', unit); }
  lineRows(unit: number, first: number, count: number) { return this.call<LineRow[]>('lineRows', unit, first, count); }
  sourceFiles() { return this.call<SourceFile[]>('sourceFiles'); }
  fileLines(file: number) { return this.call<LineRange[]>('fileLines', file); }
  embeddedSource(file: number) { return this.call<string | undefined>('embeddedSource', file); }
  fileLineCounts() { return this.call<Uint32Array>('fileLineCounts'); }
}

export type { ContainerInfo };
