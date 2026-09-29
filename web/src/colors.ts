// Region kinds fold into eight colour families (the validated categorical
// palette, in its fixed slot order) plus neutral padding / unknown.
import type { MapStatus, RegionKind } from './types';

export type Family = 'code' | 'header' | 'rodata' | 'data' | 'symbols' | 'linking' | 'debug' | 'other' | 'padding' | 'unknown';

export interface FamilyInfo {
  id: Family;
  label: string;
  kinds: RegionKind[];
  description: string;
}

export const FAMILIES: FamilyInfo[] = [
  { id: 'code', label: 'Code', kinds: ['code'], description: 'Machine instructions' },
  { id: 'header', label: 'Headers & tables', kinds: ['container', 'header', 'metadata'], description: 'File headers, load commands, program/section header tables' },
  { id: 'rodata', label: 'Read-only data', kinds: ['rodata'], description: 'Constants and string literals' },
  { id: 'data', label: 'Writable data', kinds: ['data', 'bss', 'tls'], description: 'Initialized, zero-filled and thread-local data' },
  { id: 'symbols', label: 'Symbols & strings', kinds: ['symbols', 'strings'], description: 'Symbol tables and the string tables naming them' },
  { id: 'linking', label: 'Linking', kinds: ['linking', 'relocations'], description: 'Dynamic linking info, imports/exports, relocations' },
  { id: 'debug', label: 'Debug info', kinds: ['debug'], description: 'DWARF, CodeView and other debug information' },
  { id: 'other', label: 'Other', kinds: ['unwind', 'resources', 'notes', 'signature', 'overlay'], description: 'Unwind tables, resources, notes, signatures, overlays' },
  { id: 'padding', label: 'Padding', kinds: ['padding'], description: 'Alignment filler (zero bytes)' },
  { id: 'unknown', label: 'Unclaimed', kinds: ['unknown'], description: 'Bytes no known structure accounts for' },
];

const byKind = new Map<RegionKind, FamilyInfo>();
for (const f of FAMILIES) for (const k of f.kinds) byKind.set(k, f);

export function familyOf(kind: RegionKind): Family {
  return byKind.get(kind)?.id ?? 'unknown';
}

export const KIND_LABELS: Record<RegionKind, string> = {
  container: 'Container',
  header: 'Header',
  metadata: 'Structure',
  code: 'Code',
  rodata: 'Read-only data',
  data: 'Data',
  bss: 'Zero-filled data',
  tls: 'Thread-local data',
  symbols: 'Symbols',
  strings: 'Strings',
  relocations: 'Relocations',
  linking: 'Linking',
  debug: 'Debug info',
  unwind: 'Unwind info',
  resources: 'Resources',
  notes: 'Notes',
  signature: 'Signature',
  padding: 'Padding',
  unknown: 'Unclaimed',
  overlay: 'Overlay',
};

/** Resolved colour for a family (for canvas drawing); re-read after theme changes. */
export function familyColor(f: Family): string {
  return getComputedStyle(document.documentElement).getPropertyValue(`--f-${f}`).trim() || '#888';
}

export function familyColors(): Record<Family, string> {
  const style = getComputedStyle(document.documentElement);
  const out = {} as Record<Family, string>;
  for (const f of FAMILIES) out[f.id] = style.getPropertyValue(`--f-${f.id}`).trim() || '#888';
  return out;
}

/** Sequential single-hue ramp (blue) for entropy, light→dark; flipped in dark mode. */
const RAMP = ['#cde2fb', '#b7d3f6', '#9ec5f4', '#86b6ef', '#6da7ec', '#5598e7', '#3987e5', '#2a78d6', '#256abf', '#1c5cab', '#184f95', '#104281', '#0d366b'];

export function isDark(): boolean {
  return document.documentElement.dataset.resolvedTheme === 'dark';
}

export function entropyColor(t: number): string {
  const x = Math.max(0, Math.min(1, t));
  const i = Math.round(x * (RAMP.length - 1));
  return isDark() ? RAMP[RAMP.length - 1 - i] : RAMP[i];
}

export function entropyRamp(): string[] {
  return isDark() ? [...RAMP].reverse() : RAMP;
}

// --- Reverse-engineering coverage ---------------------------------------------
// An ordinal blue ramp for how well a byte is understood (validated with
// --ordinal in both themes), neutrals for structure and filler, and the status
// warning amber for what nobody has looked at yet.

export interface StatusInfo {
  id: MapStatus;
  label: string;
  description: string;
}

/** Strongest first — the order legends and stacked bars use. */
export const STATUSES: StatusInfo[] = [
  { id: 'matched', label: 'Decompiled', description: 'A function whose decompiled C compiles to these bytes (a matching decompilation)' },
  { id: 'reviewed', label: 'Reviewed', description: 'Inside a range you marked as reviewed' },
  { id: 'annotated', label: 'Annotated', description: 'Inside a range you named or commented' },
  { id: 'named', label: 'Named', description: 'Covered by a symbol from the file or its debug info' },
  { id: 'recovered', label: 'Recovered', description: 'Found by analysis but unnamed: functions from unwind tables or function-start lists, and strings' },
  { id: 'structure', label: 'Format structure', description: 'Tables binviz decodes itself (import tables, directories…)' },
  { id: 'padding', label: 'Padding', description: 'Alignment filler: zeros, int3, nops' },
  { id: 'unexplored', label: 'Unexplored', description: 'Nothing accounts for these bytes yet' },
];

export const STATUS_LABELS = Object.fromEntries(STATUSES.map((s) => [s.id, s.label])) as Record<MapStatus, string>;

export function statusColors(): Record<MapStatus, string> {
  const style = getComputedStyle(document.documentElement);
  const out = {} as Record<MapStatus, string>;
  for (const s of STATUSES) out[s.id] = style.getPropertyValue(`--s-${s.id}`).trim() || '#888';
  return out;
}
