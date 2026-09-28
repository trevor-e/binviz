// Reading and writing annotations: binviz's own JSON, and symbol lists from
// other tools (CSV with a header, `nm` output, IDA/Ghidra exports, or plain
// "address name" lines).
import type { Annotation } from './types';

interface Stored {
  address: string;
  size?: string;
  name?: string;
  comment?: string;
  reviewed?: boolean;
}

export function serializeAnnotations(list: Annotation[], file: string, sha256: string): string {
  const annotations: Stored[] = list.map((a) => ({
    address: '0x' + a.address.toString(16),
    ...(a.size > 0n ? { size: '0x' + a.size.toString(16) } : {}),
    ...(a.name ? { name: a.name } : {}),
    ...(a.comment ? { comment: a.comment } : {}),
    ...(a.reviewed ? { reviewed: true } : {}),
  }));
  return JSON.stringify({ format: 'binviz-annotations', version: 1, file, sha256, annotations }, null, 2);
}

/** `0x401000`, `401000h`, `ram:00401000`, `0000000000401000`, or a JSON number. */
function parseAddress(v: unknown): bigint | undefined {
  if (typeof v === 'number') return Number.isSafeInteger(v) && v >= 0 ? BigInt(v) : undefined;
  if (typeof v !== 'string') return undefined;
  let t = v.trim().replace(/^[a-z_]+:/i, '').replace(/[`_]/g, '');
  if (/h$/i.test(t) && /^[0-9a-f]+h$/i.test(t)) t = t.slice(0, -1);
  if (/^0x[0-9a-f]{1,16}$/i.test(t)) return BigInt(t);
  if (/^[0-9a-f]{1,16}$/i.test(t) && /[0-9]/.test(t)) return BigInt('0x' + t);
  return undefined;
}

function parseSize(v: unknown): bigint {
  if (typeof v === 'number') return Number.isSafeInteger(v) && v > 0 ? BigInt(v) : 0n;
  if (typeof v !== 'string' || !v.trim()) return 0n;
  const t = v.trim();
  try {
    return /^0x/i.test(t) ? BigInt(t) : /^[0-9]+$/.test(t) ? BigInt(t) : 0n;
  } catch {
    return 0n;
  }
}

function fromJson(value: unknown): Annotation[] | undefined {
  const list = Array.isArray(value) ? value : (value as { annotations?: unknown })?.annotations;
  if (!Array.isArray(list)) return undefined;
  const out: Annotation[] = [];
  for (const item of list) {
    if (!item || typeof item !== 'object') continue;
    const o = item as Record<string, unknown>;
    const address = parseAddress(o.address ?? o.addr ?? o.ea ?? o.location);
    if (address === undefined) continue;
    out.push({
      address,
      size: parseSize(o.size ?? o.length),
      name: typeof o.name === 'string' ? o.name : '',
      comment: typeof o.comment === 'string' ? o.comment : '',
      reviewed: o.reviewed === true,
    });
  }
  return out;
}

/** Splits a CSV line, honouring double quotes. */
function splitCsv(line: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quoted) {
      if (c === '"' && line[i + 1] === '"') {
        cur += '"';
        i++;
      } else if (c === '"') quoted = false;
      else cur += c;
    } else if (c === '"') quoted = true;
    else if (c === ',' || c === '\t' || c === ';') {
      out.push(cur.trim());
      cur = '';
    } else cur += c;
  }
  out.push(cur.trim());
  return out;
}

const HEADER = {
  address: /^(address|addr|location|ea|va|rva|start|offset)$/i,
  name: /^(name|symbol|label|function|function name)$/i,
  size: /^(size|length|len)$/i,
  comment: /^(comment|note|notes|description)$/i,
};

/** Parses annotations from any of the supported text formats. */
export function parseAnnotations(text: string): Annotation[] {
  const trimmed = text.trim();
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    try {
      const parsed = fromJson(JSON.parse(trimmed));
      if (parsed) return parsed;
    } catch {
      /* not JSON after all */
    }
  }
  const lines = trimmed.split(/\r?\n/).filter((l) => l.trim() && !/^\s*(#|\/\/)/.test(l));
  if (lines.length === 0) return [];
  // A CSV header names the columns.
  const first = splitCsv(lines[0]);
  const col = (re: RegExp) => first.findIndex((c) => re.test(c.replace(/^"|"$/g, '')));
  const cols = { address: col(HEADER.address), name: col(HEADER.name), size: col(HEADER.size), comment: col(HEADER.comment) };
  const out: Annotation[] = [];
  if (cols.address >= 0) {
    for (const line of lines.slice(1)) {
      const cells = splitCsv(line);
      const address = parseAddress(cells[cols.address]);
      if (address === undefined) continue;
      out.push({
        address,
        size: cols.size >= 0 ? parseSize(cells[cols.size]) : 0n,
        name: cols.name >= 0 ? (cells[cols.name] ?? '') : '',
        comment: cols.comment >= 0 ? (cells[cols.comment] ?? '') : '',
        reviewed: false,
      });
    }
    return out;
  }
  // No header: "address name", "name address", `nm` lines ("0000000000401000 T main"),
  // IDA "name = 0x401000"...
  for (const line of lines) {
    const tokens = line.includes(',') || line.includes('\t') ? splitCsv(line) : line.trim().split(/\s+/);
    const cleaned = tokens.map((t) => t.replace(/^"|"$/g, '')).filter((t) => t && t !== '=');
    const ai = cleaned.findIndex((t) => parseAddress(t) !== undefined && (/^0x/i.test(t) || /[0-9]/.test(t)) && t.replace(/^0x/i, '').length >= 4);
    if (ai < 0) continue;
    const address = parseAddress(cleaned[ai])!;
    // The name is the last other token, skipping nm's one-letter type codes.
    const rest = cleaned.filter((_, i) => i !== ai);
    const name = [...rest].reverse().find((t) => !/^[a-zA-Z?]$/.test(t)) ?? '';
    out.push({ address, size: 0n, name, comment: '', reviewed: false });
  }
  return out;
}
