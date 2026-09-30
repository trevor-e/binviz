// Mirrors of the Rust model (crates/binviz/src/model.rs and friends).
// Every Rust u64 (address, offset, size) arrives as a bigint.

export type RegionKind =
  | 'container' | 'header' | 'metadata' | 'code' | 'rodata' | 'data' | 'bss' | 'tls'
  | 'symbols' | 'strings' | 'relocations' | 'linking' | 'debug' | 'unwind' | 'resources'
  | 'notes' | 'signature' | 'padding' | 'unknown' | 'overlay';

export interface Property { key: string; value: string }

export interface Summary {
  format: 'elf' | 'mach-o' | 'pe' | 'coff' | 'xcoff' | 'wasm' | 'rom' | 'xbe' | 'unknown';
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

export type SymbolSource = 'symtab' | 'dynsym' | 'export' | 'dwarf' | 'discovered' | 'user' | 'import' | 'debug-file' | 'objc' | 'rtti';

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
  /** The string this byte is part of, if it is text. */
  string?: StringHere;
}

/** A string a location is part of. */
export interface StringHere {
  offset: bigint;
  address?: bigint;
  /** Bytes it occupies (a table's end marker included). */
  size: number;
  text: string;
  encoding: 'ascii' | 'utf-16' | 'table';
}

/** A note the user attached to an address range. `size` 0 means "the symbol or instruction there". */
export interface Annotation {
  address: bigint;
  size: bigint;
  name: string;
  comment: string;
  reviewed: boolean;
  /** Where decompiling the function here stands (set by agents through the MCP server). */
  decomp?: Decomp;
  /** Who wrote it, when not you: an agent mapping the binary. Its names are guesses until confirmed. */
  author?: string;
}

export type DecompState = 'todo' | 'in-progress' | 'matched' | 'nonmatching' | 'skipped' | 'library';

export interface Decomp {
  state: DecompState;
  /** How much of it matched, 0–100, at best. */
  percent?: number;
  attempts: number;
  /** Who is working on it, while in progress. */
  by: string;
  /** When its state last changed, in seconds since 1970. */
  since: bigint;
  /** The source file its C is in. */
  source: string;
}

/** A line shown before an instruction: where a piece of the function, a switch's case or a jump table starts. */
export interface Mark { address: bigint; text: string }
export interface Disassembly {
  start: bigint;
  end: bigint;
  function?: SymbolRef;
  instructions: Instruction[];
  truncated: boolean;
  supported: boolean;
  marks?: Mark[];
}

/** `contiguous`: its bytes are `offset..offset + size` of the file (not on a raw CD image). */
export interface Member { index: number; name: string; offset: bigint; size: bigint; arch?: string; contiguous?: boolean }
/** A file on a CD image. */
export interface DiscFile { path: string; lba: bigint; size: bigint; dir: boolean }
export interface ContainerInfo { kind: string; fileSize: bigint; members: Member[] }

// --- Text in games ------------------------------------------------------------

export interface RelativeHit { offset: bigint; preview: string; at: number }
/** An encoding relative search found: the value standing for A (or a, or 0). */
export interface TextEncoding { first: number; letter: string; hits: RelativeHit[] }
export interface RelativeSearch { word: string; width: number; encodings: TextEncoding[]; total: number }
export interface TableText { offset: bigint; len: number; text: string }

// --- Emulators ----------------------------------------------------------------

export type LogFormat = 'fceux' | 'mesen' | 'mesen2';
/** What a code/data log covers (see binviz::rom::cdl). */
export interface LogSummary {
  format: LogFormat;
  bytes: bigint;
  code: bigint;
  data: bigint;
  both: bigint;
  entries: bigint;
  jumps: bigint;
  chrSeen: bigint;
  crcMatches?: boolean;
  pagesPlaced: number;
}
export type LabelFormat = 'mlb' | 'nl' | 'sym' | 'no-cash';
export interface LabelImport { format: LabelFormat; labels: Annotation[]; skipped: number; directives: number }
export interface LabelFile { suffix: string; text: string }

// --- Patches ----------------------------------------------------------------------

export type PatchFormat = 'ips' | 'ups' | 'bps';
export interface PatchInfo {
  format: PatchFormat;
  records: bigint;
  sourceSize?: bigint;
  sourceCrc32?: number;
  targetSize: bigint;
  targetCrc32?: number;
  metadata?: string;
  truncate?: bigint;
}
/** What applying a patch file said. */
export interface Applied {
  info: PatchInfo;
  differ: bigint;
  sourceMatches?: boolean;
  targetMatches?: boolean;
  skippedHeader: bigint;
  reversed: boolean;
  warnings: string[];
}
export type ChangeKind = 'changed' | 'added' | 'removed';
/** A run of changed bytes, placed: its bank or section, address, function and regions, and its bytes before and after. */
export interface PatchRow {
  kind: ChangeKind;
  offset: bigint;
  len: bigint;
  differ: bigint;
  section?: string;
  address?: bigint;
  function?: string;
  region: string[];
  before: Uint8Array | number[];
  after: Uint8Array | number[];
  beforeText?: string;
  afterText?: string;
}
// --- Functions compared ---------------------------------------------------------------

export interface FnInfo { address: bigint; name: string; size: bigint; instructions?: number }
export interface FunctionPair {
  old: FnInfo;
  new: FnInfo;
  how: 'name' | 'bytes' | 'instructions' | 'calls' | 'address';
  status: 'identical' | 'relocated' | 'changed';
  similarity: number;
}
/** Which functions of two versions are which (identical pairs only counted). */
export interface FunctionDiff {
  pairs: FunctionPair[];
  added: FnInfo[];
  removed: FnInfo[];
  identical: number;
  relocated: number;
  changed: number;
}
export interface DiffLine { kind: 'same' | 'changed' | 'removed' | 'added'; old?: Instruction; new?: Instruction }

export interface PatchState {
  name?: string;
  applied?: Applied;
  changes: PatchRow[];
  total: number;
  differ: bigint;
  targetSize: bigint;
}

/** An object file a Mach-O debug map names (where a binary linked without dsymutil keeps its DWARF). */
export interface DebugMapObject { index: number; path: string; member?: string; modified: bigint; symbols: number }
export interface DebugMapReport { objects: number; linked: number; missing: string[]; failed: string[]; units: number }

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
  /** Enclosing named scopes (`geo::Rect`), in listings and search results. */
  scope?: string;
}

export type Link =
  | { type: 'die'; unit: number; offset: bigint }
  | { type: 'address'; address: bigint }
  | { type: 'source'; file: number; line: number };

export interface AttrInfo {
  name: string;
  form: string;
  value: string;
  link?: Link;
  /** Section offsets of the encoded value. */
  byteStart: bigint;
  byteEnd: bigint;
}

export interface CodeLine { file: number; path: string; line: number; bytes: bigint; first: bigint; rows: number }

export interface MemberLayout {
  kind: 'member' | 'base' | 'static';
  name?: string;
  typeName: string;
  offset?: bigint;
  size?: bigint;
  bitOffset?: bigint;
  bitSize?: bigint;
  hole: bigint;
  artificial: boolean;
  unit: number;
  die: bigint;
}

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
  childCount: number;
  callSite?: SourceLoc;
  codeLines: CodeLine[];
  layout: MemberLayout[];
  byteSize?: bigint;
  tailPadding?: bigint;
}

export interface DiePage { total: number; offset: number; dies: DieSummary[] }
export interface TagCount { tag: string; count: number }

export type Severity = 'error' | 'warning';
export interface DwarfProblem {
  severity: Severity;
  area: string;
  message: string;
  unit?: number;
  die?: bigint;
  tag?: string;
  section: string;
  offset?: bigint;
}
export interface DwarfCheck {
  units: number;
  dies: bigint;
  lineRows: bigint;
  errors: number;
  warnings: number;
  byUnit: { unit: number; errors: number; warnings: number }[];
  problems: DwarfProblem[];
  truncated: boolean;
}

export interface ScopeVar { name: string; kind: string; typeName?: string; location: string; decl?: SourceLoc; unit: number; die: bigint; scope: number }
export interface ScopeInfo { address: bigint; unit: number; scopes: DieSummary[]; variables: ScopeVar[] }

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

export type MapStatus = 'unexplored' | 'padding' | 'recovered' | 'structure' | 'named' | 'annotated' | 'reviewed' | 'matched';
export type StatusBytes = Record<MapStatus, bigint>;
export interface SectionCoverage { section: number; name: string; kind: RegionKind; address: bigint; size: bigint; bytes: StatusBytes }
export interface Gap { start: bigint; end: bigint; section: number; offset?: bigint; after?: string; hint: string; preview: string }
export interface Coverage {
  sections: SectionCoverage[];
  totals: StatusBytes;
  gaps: Gap[];
  gapCount: number;
  /** Functions named by the file, recovered unnamed, named by you, and named by agents (unconfirmed). */
  functions: { named: number; recovered: number; user: number; agents: number };
  annotations: number;
  reviewed: number;
  /** Notes an agent wrote. */
  agentNotes: number;
}

/** A function still to name, and what to know about it. */
export interface WorkItem {
  address: bigint;
  name: string;
  size: bigint;
  callers: number;
  callees: number;
  unnamedCallees: number;
}

/** What to name next: leaves first, then the most called. */
export interface Worklist {
  functions: number;
  named: number;
  remaining: number;
  items: WorkItem[];
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

// --- Objective-C metadata ----------------------------------------------------

export interface ObjcCounts { classes: number; swiftClasses: number; categories: number; protocols: number; methods: number; selectors: number }
export type ObjcKind = 'class' | 'category' | 'protocol';
/** A class (with its superclass as base), a category (base: the class it adds to) or a protocol. */
export interface ObjcEntry { kind: ObjcKind; name: string; address: bigint; base?: string; methods: number; swift: boolean }
/** A line of a header: a method's has its implementation and selector. */
export interface InterfaceLine { text: string; address?: bigint; selector?: string }
export interface ObjcInterface { kind: ObjcKind; name: string; address: bigint; lines: InterfaceLine[] }
export interface Implementation { name: string; address: bigint }
export interface SelectorUses { selector: string; implementations: Implementation[]; references: bigint[]; stubs: bigint[]; senders: CallEdge[] }

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

// --- Folders of binaries and size reports -------------------------------------

/** What a binary is, from its header. */
export type BinaryKind = 'executable' | 'library' | 'plugin' | 'other' | 'object' | 'debug';
/** An architecture and its build ID (a Mach-O UUID, an ELF build ID; empty when unknown). */
export interface BuildId { arch: string; id: string }
export interface BinaryHeader { format: string; kind: BinaryKind; ids: BuildId[] }
export interface BundleInfo {
  path: string;
  name?: string;
  bundleId?: string;
  version?: string;
  build?: string;
  minOs?: string;
  platforms: string[];
  executable?: string;
}
export interface PackageBinary {
  index: number;
  file: number;
  path: string;
  name: string;
  /** "Mach-O", "ELF" or "PE". */
  format: string;
  kind: BinaryKind;
  bundle?: BundleInfo;
  size: bigint;
  compressedSize?: bigint;
  ids: BuildId[];
  /** Index into the debug files. */
  debug?: number;
}
export interface DebugFile { index: number; file: number; path: string; size: bigint; compressedSize?: bigint; ids: BuildId[]; binary?: number }
export type FileCategory =
  | 'binaries' | 'asset-catalogs' | 'images' | 'interface' | 'localization' | 'fonts' | 'media' | 'ml-models'
  | 'web' | 'data' | 'developer-files' | 'code-signature' | 'debug-symbols' | 'other';
export interface CategorySize { category: FileCategory; files: number; size: bigint; compressedSize?: bigint }
export interface FileRef { file: number; path: string; size: bigint; compressedSize?: bigint; category: FileCategory }
export interface DuplicateGroup { size: bigint; paths: string[]; wasted: bigint }
export interface PackageInfo {
  /** "folder" or "zip". */
  kind: string;
  name: string;
  binaries: PackageBinary[];
  debugFiles: DebugFile[];
  files: number;
  /** Every file but debug files. */
  size: bigint;
  compressedSize?: bigint;
  debugSize: bigint;
  categories: CategorySize[];
  largest: FileRef[];
  duplicates: DuplicateGroup[];
  duplicateBytes: bigint;
}
/** Where a folder's files come from (sent to the worker): a zip, or files dropped or picked. */
export type PackageSource = { kind: 'zip'; name: string; blob: Blob } | { kind: 'folder'; name: string; files: { path: string; file: File }[] };

export type GroupKind = 'swift-module' | 'objc-class' | 'namespace' | 'c-prefix' | 'compiler-generated' | 'unnamed' | 'other';
export interface SizeGroup { kind: GroupKind; name: string; functions: number; codeBytes: bigint; dataSymbols: number; dataBytes: bigint }
export interface SizedSymbol { name: string; address: bigint; size: bigint; approximate: boolean }
export interface SizeReport {
  fileSize: bigint;
  byKind: [RegionKind, bigint][];
  byGroupKind: [GroupKind, bigint][];
  groups: SizeGroup[];
  groupCount: number;
  largestFunctions: SizedSymbol[];
  largestData: SizedSymbol[];
  symbolizedBytes: bigint;
  strings: number;
  stringBytes: bigint;
}

// --- Crash reports ----------------------------------------------------------------

export interface CrashImage { name: string; path?: string; id?: string; arch?: string; loadAddress?: bigint; size?: bigint }
export interface CrashFrame { index: number; image?: number; imageName: string; address?: bigint; offset?: bigint; linked?: bigint; symbol?: string }
export interface CrashThread { name: string; crashed: boolean; frames: CrashFrame[] }
export interface CrashReport {
  format: string;
  process?: string;
  identifier?: string;
  version?: string;
  os?: string;
  arch?: string;
  exception?: string;
  reason?: string;
  images: CrashImage[];
  /** The crashed thread first. */
  threads: CrashThread[];
}
/** One function of a frame: innermost first, the calls inlined there, then the function around them. */
export interface SymbolLine { function?: string; offset?: bigint; file?: string; line?: number; column?: number; inlined: boolean }
export interface SymbolicatedFrame {
  index: number;
  image?: number;
  imageName: string;
  /** As the report gives it (at run time). */
  address?: bigint;
  /** In the binary that symbolicated it. */
  binaryAddress?: bigint;
  /** That binary: an index into the open folder's binaries (0 for a lone binary). */
  binary?: number;
  lines: SymbolLine[];
  reported?: string;
}
export interface SymbolicatedThread { name: string; crashed: boolean; frames: SymbolicatedFrame[] }
export interface ImageStatus { image: number; frames: number; binary?: number; note?: string }
export interface Symbolicated { report: CrashReport; images: ImageStatus[]; threads: SymbolicatedThread[] }

// --- Comparing sizes with another build ---------------------------------------------

export interface Change { name: string; old: bigint; new: bigint }
export interface KindChange { kind: RegionKind; old: bigint; new: bigint }
export interface CategoryChange { category: FileCategory; old: bigint; new: bigint }
export interface OwnerChange { kind: GroupKind; name: string; old: bigint; new: bigint }
export interface SymbolChange { name: string; code: boolean; old: bigint; new: bigint }
export interface SizeDiff {
  oldName: string;
  newName: string;
  oldSize: bigint;
  newSize: bigint;
  byKind: KindChange[];
  sections: Change[];
  owners: OwnerChange[];
  ownersChanged: number;
  symbols: SymbolChange[];
  symbolsAdded: number;
  symbolsRemoved: number;
  symbolsChanged: number;
}
export interface BinaryChange { path: string; name: string; old: bigint; new: bigint; diff?: SizeDiff }
export interface FolderDiff {
  oldName: string;
  newName: string;
  oldSize: bigint;
  newSize: bigint;
  categories: CategoryChange[];
  files: Change[];
  filesAdded: number;
  filesRemoved: number;
  filesChanged: number;
  binaries: BinaryChange[];
  owners: OwnerChange[];
  ownersChanged: number;
}
export type Comparison = { kind: 'binary'; diff: SizeDiff } | { kind: 'folder'; diff: FolderDiff };
/** The earlier build to compare with: a binary, or folders and zips. */
export type BaselineSource = { kind: 'file'; name: string; blob: Blob } | { kind: 'folder'; sources: PackageSource[] };
