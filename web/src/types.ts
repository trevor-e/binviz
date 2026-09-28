// Mirrors of the Rust model (crates/binviz/src/model.rs and friends).
// Every Rust u64 (address, offset, size) arrives as a bigint.

export type RegionKind =
  | 'container' | 'header' | 'metadata' | 'code' | 'rodata' | 'data' | 'bss' | 'tls'
  | 'symbols' | 'strings' | 'relocations' | 'linking' | 'debug' | 'unwind' | 'resources'
  | 'notes' | 'signature' | 'padding' | 'unknown' | 'overlay';

export interface Property { key: string; value: string }

export interface Summary {
  format: 'elf' | 'mach-o' | 'pe' | 'coff' | 'xcoff' | 'wasm' | 'unknown';
  formatName: string;
  kind: string;
  arch: string;
  bits: number;
  littleEndian: boolean;
  fileSize: bigint;
  entry?: bigint;
  imageBase?: bigint;
  buildId?: string;
  debugLink?: string;
  hasDwarf: boolean;
  hasSymbols: boolean;
  syntheticAddresses: boolean;
  sectionCount: number;
  segmentCount: number;
  symbolCount: number;
  properties: Property[];
  /** Identifies the file's contents; saved notes are keyed by it. */
  fingerprint: string;
}

export interface Section {
  index: number;
  name: string;
  segmentName?: string;
  kind: RegionKind;
  address: bigint;
  size: bigint;
  fileOffset?: bigint;
  fileSize: bigint;
  align: bigint;
  flags: string;
  perms: string;
  compressed: boolean;
  segment?: number;
  loaded: boolean;
}

export interface Segment {
  index: number;
  name: string;
  kind: string;
  address: bigint;
  memSize: bigint;
  fileOffset: bigint;
  fileSize: bigint;
  align: bigint;
  perms: string;
  mapped: boolean;
}

export type SymbolKind = 'function' | 'data' | 'section' | 'file' | 'label' | 'tls' | 'debug' | 'unknown';

export type SymbolSource = 'symtab' | 'dynsym' | 'export' | 'dwarf' | 'discovered' | 'user' | 'import';

export interface Sym {
  index: number;
  name: string;
  demangled?: string;
  address: bigint;
  size: bigint;
  sizeInferred: boolean;
  kind: SymbolKind;
  binding: string;
  section?: number;
  source: SymbolSource;
  defined: boolean;
}

export interface SymbolPage { total: number; offset: number; symbols: Sym[] }

export interface FunctionPage { total: number; offset: number; functions: [bigint, bigint, string][] }

export interface SymbolQuery {
  filter?: string;
  kind?: string;
  sort?: 'address' | 'name' | 'size';
  descending?: boolean;
  definedOnly?: boolean;
  offset?: number;
  limit?: number;
}

export interface Import { library: string; name: string; demangled?: string; ordinal?: number; address?: bigint }
export interface Export { name: string; demangled?: string; address: bigint; ordinal?: number; forwarder?: string }

export interface SymbolRef { index: number; name: string; demangled?: string; address: bigint; size: bigint; offset: bigint }

export interface PathEntry {
  id?: number;
  start: bigint;
  end: bigint;
  kind: RegionKind;
  name: string;
  value?: string;
  note?: string;
}

export interface RegionInfo {
  id: number;
  parent?: number;
  start: bigint;
  end: bigint;
  kind: RegionKind;
  name: string;
  value?: string;
  note?: string;
  childCount: number;
  entryCount?: number;
  decoded: boolean;
  section?: number;
}

export interface Span { start: bigint; end: bigint; kind: RegionKind; depth: number; shade: number }

export interface SourceLoc { file: number; path: string; line: number; column: number }

export interface Frame {
  function?: string;
  demangled?: string;
  file?: string;
  line?: number;
  column?: number;
  fileIndex?: number;
  unit?: number;
  die?: bigint;
  inlined: boolean;
}

export type FlowKind = 'normal' | 'call' | 'jump' | 'cond-jump' | 'return' | 'interrupt' | 'invalid';

export interface Instruction {
  address: bigint;
  offset?: bigint;
  len: number;
  bytes: string;
  mnemonic: string;
  operands: string;
  flow: FlowKind;
  target?: bigint;
  targetSymbol?: string;
  source?: SourceLoc;
}

export interface Inspection {
  offset?: bigint;
  address?: bigint;
  byte?: number;
  path: PathEntry[];
  segment?: number;
  section?: number;
  symbol?: SymbolRef;
  source?: SourceLoc;
  frames: Frame[];
  instruction?: Instruction;
  unit?: number;
  annotation?: Annotation;
}

/** A note the user attached to an address range. `size` 0 means "the symbol or instruction there". */
export interface Annotation {
  address: bigint;
  size: bigint;
  name: string;
  comment: string;
  reviewed: boolean;
}

export interface Disassembly {
  start: bigint;
  end: bigint;
  function?: SymbolRef;
  instructions: Instruction[];
  truncated: boolean;
  supported: boolean;
}

export interface Member { index: number; name: string; offset: bigint; size: bigint; arch?: string }
export interface ContainerInfo { kind: string; fileSize: bigint; members: Member[] }

export type Opened =
  | { kind: 'binary'; name: string; summary: Summary }
  | { kind: 'container'; name: string; info: ContainerInfo };

export interface Resolved { kind: 'address' | 'offset'; value: bigint; label: string }

// --- DWARF -----------------------------------------------------------------

export interface DwarfSummary {
  source: string;
  versions: number[];
  unitCount: number;
  sections: { name: string; size: bigint }[];
  producers: string[];
  languages: string[];
  splitUnits: number;
}

export interface UnitInfo {
  index: number;
  offset: bigint;
  section: string;
  kind: string;
  version: number;
  addressSize: number;
  dwarf64: boolean;
  size: bigint;
  name?: string;
  compDir?: string;
  producer?: string;
  language?: string;
  lowPc: bigint;
  ranges: [bigint, bigint][];
  codeSize: bigint;
  dwoName?: string;
}

export interface DieSummary {
  unit: number;
  offset: bigint;
  sectionOffset: bigint;
  tag: string;
  name?: string;
  hasChildren: boolean;
  detail?: string;
  lowPc?: bigint;
  highPc?: bigint;
}

export type Link =
  | { type: 'die'; unit: number; offset: bigint }
  | { type: 'address'; address: bigint }
  | { type: 'source'; file: number; line: number };

export interface AttrInfo { name: string; form: string; value: string; link?: Link }

export interface DieDetails {
  die: DieSummary;
  attributes: AttrInfo[];
  parents: DieSummary[];
  ranges: [bigint, bigint][];
  typeName?: string;
  decl?: SourceLoc;
  byteStart: bigint;
  byteEnd: bigint;
  section: string;
}

export interface SourceFile {
  id: number;
  path: string;
  name: string;
  dir: string;
  units: number[];
  embeddedSource: boolean;
  md5?: string;
}

export interface LineRange { line: number; column: number; start: bigint; end: bigint; unit: number; isStmt: boolean }

export interface LineFileEntry { index: bigint; path: string; directoryIndex: bigint; file?: number; md5?: string }

export interface LineProgramInfo {
  unit: number;
  offset: bigint;
  version: number;
  addressSize: number;
  dwarf64: boolean;
  minimumInstructionLength: number;
  maximumOperationsPerInstruction: number;
  defaultIsStmt: boolean;
  lineBase: number;
  lineRange: number;
  opcodeBase: number;
  includeDirectories: string[];
  files: LineFileEntry[];
  rowCount: number;
}

export interface LineRow {
  address: bigint;
  file?: number;
  fileIndex: bigint;
  line: number;
  column: number;
  flags: number;
  discriminator: bigint;
}

export const LINE_FLAGS = { isStmt: 1, basicBlock: 2, prologueEnd: 4, epilogueBegin: 8, endSequence: 16 } as const;

// --- Search ------------------------------------------------------------------

export type HitKind = 'address' | 'offset' | 'symbol' | 'import' | 'export' | 'section' | 'source' | 'dwarf' | 'note' | 'string' | 'bytes';

export interface SearchHit {
  kind: HitKind;
  label: string;
  detail: string;
  address?: bigint;
  offset?: bigint;
  size?: bigint;
  symbol?: number;
  section?: number;
  file?: number;
  line?: number;
  unit?: number;
  die?: bigint;
  score: number;
}

export interface SearchResults { query: string; hits: SearchHit[]; counts: { kind: HitKind; count: number }[] }

export interface FoundString { offset: bigint; address?: bigint; size: number; wide: boolean; text: string; section?: number }
export interface StringPage { total: number; offset: number; strings: FoundString[] }

// --- Attribution -------------------------------------------------------------

export type AttributionMode = 'file' | 'unit';
export interface SectionShare { section: number; code: bigint; data: bigint }
export interface Contributor { id: number; name: string; path: string; code: bigint; data: bigint; functions: number; variables: number; sections: SectionShare[] }
export interface Attribution { mode: AttributionMode; contributors: Contributor[]; sections: { section: number; attributed: bigint }[] }
export interface AttributedRange { start: bigint; end: bigint; section?: number; data: boolean; label?: string; line: number }

// --- Coverage ----------------------------------------------------------------

export type MapStatus = 'unexplored' | 'padding' | 'recovered' | 'structure' | 'named' | 'annotated' | 'reviewed';
export type StatusBytes = Record<MapStatus, bigint>;
export interface SectionCoverage { section: number; name: string; kind: RegionKind; address: bigint; size: bigint; bytes: StatusBytes }
export interface Gap { start: bigint; end: bigint; section: number; offset?: bigint; after?: string; hint: string; preview: string }
export interface Coverage {
  sections: SectionCoverage[];
  totals: StatusBytes;
  gaps: Gap[];
  gapCount: number;
  functions: { named: number; recovered: number; user: number };
  annotations: number;
  reviewed: number;
}

// --- Cross-references and the call graph ------------------------------------

export type RefKind = 'call' | 'jump' | 'read' | 'write' | 'address' | 'pointer';
export type RefCounts = Record<RefKind, number>;

export interface Reference {
  source: bigint;
  target: bigint;
  kind: RefKind;
  /** Start of the function containing the source. */
  function?: bigint;
  /** Where the source is: `main+0x1c`, `__data+0x40`. */
  from?: string;
  /** What the target is: a string, `symbol+offset`, `-> pointee`. */
  to?: string;
}

export interface RefPage { total: number; offset: number; counts: RefCounts; refs: Reference[] }

export type NodeKind = 'function' | 'import' | 'code' | 'data';

export interface CallEdge { address: bigint; name: string; kind: NodeKind; calls: number; site: bigint }

export interface GraphNode {
  address: bigint;
  name: string;
  kind: NodeKind;
  source?: SymbolSource;
  size: bigint;
  /** 0 for the centre, negative for callers, positive for callees. */
  depth: number;
  callers?: number;
  callees?: number;
}

export interface GraphEdge { from: bigint; to: bigint; calls: number }
export interface CallGraph { center: bigint; nodes: GraphNode[]; edges: GraphEdge[]; hidden: number }
export interface PathStep { address: bigint; name: string; site?: bigint }
export interface StringUse { address: bigint; text: string; site: bigint }

export interface FunctionSummary {
  address: bigint;
  name: string;
  size: bigint;
  callerCount: number;
  callers: CallEdge[];
  calleeCount: number;
  callees: CallEdge[];
  strings: StringUse[];
  data: Reference[];
  referencedBy: RefCounts;
}
