// The right-hand panel: everything known about the current selection.
import { familyOf } from './colors';
import { store } from './store';
import type { Annotation, Inspection, PathEntry, RefCounts, RefKind, Reference } from './types';
import { basename, copyText, formatCount, formatSize, h, hex, icon, num } from './util';

const REF_LABELS: Record<RefKind, [string, string]> = {
  call: ['call', 'calls'],
  jump: ['tail call', 'tail calls'],
  read: ['read', 'reads'],
  write: ['write', 'writes'],
  address: ['address taken', 'addresses taken'],
  pointer: ['pointer in data', 'pointers in data'],
};

/** "3 calls · 1 pointer in data". */
export function describeRefs(c: RefCounts): string {
  const parts = (Object.keys(REF_LABELS) as RefKind[]).filter((k) => c[k] > 0).map((k) => `${formatCount(c[k])} ${REF_LABELS[k][c[k] === 1 ? 0 : 1]}`);
  return parts.join(' · ');
}

export class Inspector {
  readonly el = h('aside', { class: 'inspector', 'aria-label': 'Inspector' });
  /** The note form is open. */
  private editing = false;

  constructor() {
    store.on('selection', () => {
      this.editing = false;
      this.render();
    });
    store.on('file', () => {
      this.editing = false;
      this.render();
    });
    store.on('sources', () => this.render());
    store.on('xrefs', () => this.render());
    this.render();
  }

  private render() {
    const sel = store.selection;
    const ins = sel.inspection;
    const head = h('div', { class: 'insp-head' }, h('h2', null, 'Inspector'));
    if (!store.file || !ins) {
      this.el.replaceChildren(head, h('div', { class: 'insp-section muted' }, 'Click a byte, instruction, symbol or line to see what it is.'));
      return;
    }
    const sections = [this.location(ins), this.path(ins), this.placement(ins), this.references(ins), this.notes(ins), this.source(ins), this.instruction(ins), this.actions(ins)];
    this.el.replaceChildren(head, ...sections.filter((s): s is HTMLElement => s !== null));
  }

  private location(ins: Inspection): HTMLElement {
    const copy = (v: string) => {
      const b = h('button', { class: 'btn ghost small icon-only', title: 'Copy', 'aria-label': 'Copy' }, icon('copy'));
      b.addEventListener('click', () => copyText(v));
      return b;
    };
    const rows: Node[] = [];
    const row = (k: string, v: string, extra?: Node) => rows.push(h('div', { class: 'k' }, k), h('div', { class: 'v' }, v, extra ? ' ' : null, extra));
    if (ins.offset !== undefined) row('File offset', hex(ins.offset), copy(hex(ins.offset)));
    else row('File offset', 'none — not backed by file bytes');
    if (ins.address !== undefined) row('Address', hex(ins.address), copy(hex(ins.address)));
    else row('Address', 'not mapped into memory');
    if (ins.byte !== undefined) {
      const b = ins.byte;
      const ch = b >= 0x20 && b < 0x7f ? `'${String.fromCharCode(b)}'` : '·';
      row('Byte', `0x${b.toString(16).padStart(2, '0')}  ${b}  ${ch}  0b${b.toString(2).padStart(8, '0')}`);
    }
    return h('div', { class: 'insp-section' }, h('h3', null, 'Location'), h('div', { class: 'loc-grid' }, rows));
  }

  private path(ins: Inspection): HTMLElement {
    const section = h('div', { class: 'insp-section' }, h('h3', null, 'What is this?'));
    if (ins.path.length === 0) {
      section.appendChild(h('div', { class: 'muted' }, ins.offset === undefined ? 'This address has no file bytes (zero-initialized memory).' : 'Outside the file.'));
      return section;
    }
    const list = h('ul', { class: 'path' });
    ins.path.forEach((p: PathEntry, i) => {
      const last = i === ins.path.length - 1;
      const name = h('span', { class: 'p-name', title: 'Select this region' }, p.name);
      name.addEventListener('click', () => void store.select({ offset: p.start }, { view: store.view === 'layout' ? 'layout' : 'hex' }));
      const size = p.end - p.start;
      list.appendChild(
        h(
          'li',
          { class: `fam-${familyOf(p.kind)}${last ? ' leaf' : ''}`, style: `--depth:${i}` },
          name,
          p.value ? h('div', { class: 'p-value' }, p.value) : null,
          h('div', { class: 'p-meta' }, `${hex(p.start)}..${hex(p.end)} · ${size === 1n ? '1 byte' : formatSize(size)}`),
          p.note && last ? h('div', { class: 'p-note' }, p.note) : null,
        ),
      );
    });
    section.appendChild(list);
    return section;
  }

  private placement(ins: Inspection): HTMLElement | null {
    const f = store.file!;
    const seg = ins.segment !== undefined ? f.segments[ins.segment] : undefined;
    const sec = ins.section !== undefined ? f.sections[ins.section] : undefined;
    if (!seg && !sec && !ins.symbol) return null;
    const rows: Node[] = [];
    const row = (k: string, ...v: (Node | string)[]) => rows.push(h('div', { class: 'k' }, k), h('div', { class: 'v' }, ...v));
    if (seg) row('Segment', `${seg.name} `, h('span', { class: 'muted' }, `${seg.perms} ${hex(seg.address)}..${hex(seg.address + seg.memSize)}`));
    if (sec) row('Section', `${sec.segmentName ? sec.segmentName + ',' : ''}${sec.name}`);
    if (ins.symbol) {
      const s = ins.symbol;
      const name = s.demangled ?? s.name;
      const link = h('span', { class: 'link', title: s.demangled ? s.name : 'Show code' }, name);
      link.addEventListener('click', () => void store.select({ address: s.address }, { view: 'code' }));
      row('Symbol', link, s.offset > 0n ? h('span', { class: 'muted' }, ` + ${hex(s.offset)}`) : '', h('span', { class: 'muted' }, s.size > 0n ? `  (${num(s.size)} bytes)` : ''));
    }
    return h('div', { class: 'insp-section' }, h('h3', null, 'Placement'), h('div', { class: 'loc-grid' }, rows));
  }

  /** Who calls, reads, writes or points to the selection. */
  private references(ins: Inspection): HTMLElement | null {
    if (ins.address === undefined || store.xrefs === 'unsupported') return null;
    const section = h('div', { class: 'insp-section' }, h('h3', null, 'References'));
    if (store.xrefs !== 'ready') {
      const building = store.xrefs === 'building';
      const b = h('button', { class: 'btn small', type: 'button', disabled: building }, building ? 'Indexing…' : 'Find references');
      b.addEventListener('click', () => void store.ensureXrefs());
      section.append(h('div', { class: 'muted', style: 'margin-bottom:6px' }, building ? 'Finding every call, data reference and stored pointer…' : 'Who calls this, reads it, or points to it?'), b);
      return section;
    }
    const body = h('div', { class: 'refs' }, h('div', { class: 'muted' }, 'Looking…'));
    section.appendChild(body);
    void this.fillReferences(ins, body);
    return section;
  }

  private async fillReferences(ins: Inspection, body: HTMLElement) {
    const sym = ins.symbol;
    // A function's callers refer to its start; data is referred to anywhere inside.
    let lo = ins.address!;
    let hi = lo + 1n;
    let what = 'this address';
    const code = ins.instruction !== undefined;
    if (sym && code) {
      lo = sym.address;
      hi = lo + 1n;
      what = sym.demangled ?? sym.name;
    } else if (sym && sym.size > 0n) {
      lo = sym.address;
      hi = sym.address + sym.size;
      what = sym.demangled ?? sym.name;
    }
    const page = await store.api.referencesTo(lo, hi, 0, 12);
    if (store.selection.inspection !== ins) return;
    const rows: HTMLElement[] = [];
    const list = h('div', { class: 'ref-list' });
    const add = (refs: Reference[]) => {
      for (const r of refs) list.appendChild(this.refRow(r, lo, hi));
    };
    add(page.refs);
    rows.push(h('div', { class: 'ref-sum', title: what }, page.total ? describeRefs(page.counts) : `Nothing refers to ${what.length > 40 ? 'it' : what} directly.`));
    if (page.total === 0) rows.push(h('div', { class: 'muted', style: 'font-size:12px' }, code ? 'It may be called indirectly: through a register, a vtable or a callback set up at run time.' : 'It may be reached through a computed address.'));
    rows.push(list);
    let shown = page.refs.length;
    if (page.total > shown) {
      const more = h('button', { class: 'btn small', type: 'button' }, `Show more (${formatCount(page.total - shown)} left)`);
      more.addEventListener('click', async () => {
        const next = await store.api.referencesTo(lo, hi, shown, 100);
        add(next.refs);
        shown += next.refs.length;
        if (shown >= page.total) more.remove();
        else more.textContent = `Show more (${formatCount(page.total - shown)} left)`;
      });
      rows.push(h('div', { class: 'btn-row' }, more));
    }
    if (code) {
      const graph = h('button', { class: 'btn small', type: 'button', title: 'Callers and callees, a few levels each way (0)' }, 'Call graph');
      graph.addEventListener('click', () => store.setView('calls'));
      rows.push(h('div', { class: 'btn-row', style: 'margin-top:6px' }, graph));
    }
    body.replaceChildren(...rows);
  }

  private refRow(r: Reference, lo: bigint, hi: bigint): HTMLElement {
    const inCode = store.file!.sections.some((s) => s.kind === 'code' && s.loaded && r.source >= s.address && r.source < s.address + s.size);
    const row = h(
      'div',
      { class: 'ref-row', title: `${REF_LABELS[r.kind][0]} at ${hex(r.source)}${hi - lo > 1n && r.target !== lo ? ` (to +${hex(r.target - lo)})` : ''}` },
      h('span', { class: `ref-kind k-${r.kind}` }, r.kind),
      h('span', { class: 'link mono' }, r.from ?? hex(r.source)),
    );
    row.addEventListener('click', () => void store.select({ address: r.source }, { view: inCode ? 'code' : 'hex' }));
    return row;
  }

  /** Opens the note form for the current selection (the N key). */
  startNote() {
    if (store.selection.inspection?.address === undefined) return;
    this.editing = true;
    this.render();
  }

  private notes(ins: Inspection): HTMLElement | null {
    if (ins.address === undefined) return null;
    const section = h('div', { class: 'insp-section' }, h('h3', null, 'Notes'));
    if (this.editing) {
      section.appendChild(this.noteForm(ins));
      return section;
    }
    const btn = (label: string, fn: () => void, title = '') => {
      const b = h('button', { class: 'btn small', type: 'button', title }, label);
      b.addEventListener('click', fn);
      return b;
    };
    const a = ins.annotation;
    if (a) {
      section.append(
        h(
          'div',
          { class: 'note' },
          a.name ? h('div', { class: 'note-name mono' }, a.name) : null,
          a.comment ? h('div', { class: 'note-comment' }, a.comment) : null,
          h('div', { class: 'note-meta' }, a.reviewed ? h('span', { class: 'chip ok' }, 'Reviewed') : null, h('span', { class: 'muted mono' }, a.size > 0n ? `${hex(a.address)}..${hex(a.address + a.size)}` : `at ${hex(a.address)}`)),
        ),
        h(
          'div',
          { class: 'btn-row' },
          btn('Edit', () => this.startNote(), 'Edit this note (N)'),
          btn(a.reviewed ? 'Unmark reviewed' : 'Mark reviewed', () => void store.annotate({ ...a, reviewed: !a.reviewed })),
          btn('Delete', () => void store.removeAnnotation(a)),
        ),
      );
      return section;
    }
    const sym = ins.symbol;
    section.append(
      h('div', { class: 'muted', style: 'margin-bottom:6px' }, sym ? 'Rename the function, comment this spot, or mark it as understood.' : 'Name or comment this address.'),
      h(
        'div',
        { class: 'btn-row' },
        btn('Add note…', () => this.startNote(), 'Name or comment (N)'),
        sym && sym.size > 0n ? btn('Mark function reviewed', () => void store.annotate({ address: sym.address, size: sym.size, name: '', comment: '', reviewed: true })) : null,
      ),
    );
    return section;
  }

  private noteForm(ins: Inspection): HTMLElement {
    const a = ins.annotation;
    const sym = ins.symbol;
    const targets: { label: string; address: bigint; size: bigint }[] = [];
    if (a) targets.push({ label: `This note (${a.size > 0n ? `${hex(a.address)}, ${formatSize(a.size)}` : hex(a.address)})`, address: a.address, size: a.size });
    else {
      if (sym && sym.size > 0n) targets.push({ label: `Function ${short(sym.demangled ?? sym.name)} (${formatSize(sym.size)})`, address: sym.address, size: sym.size });
      if (ins.instruction) targets.push({ label: `This instruction (${hex(ins.instruction.address)})`, address: ins.instruction.address, size: BigInt(ins.instruction.len) });
      else targets.push({ label: `This byte (${hex(ins.address!)})`, address: ins.address!, size: 1n });
    }
    const target = h('select', { class: 'field small', 'aria-label': 'Applies to' }, targets.map((t, i) => h('option', { value: String(i) }, t.label)));
    const name = h('input', { class: 'field small', placeholder: 'Name, e.g. parse_header', value: a?.name ?? '', 'aria-label': 'Name', spellcheck: 'false' });
    const comment = h('textarea', { class: 'field', rows: 3, placeholder: 'What does it do?', 'aria-label': 'Comment' });
    comment.value = a?.comment ?? '';
    const reviewed = h('input', { type: 'checkbox', checked: a?.reviewed ?? false });
    const save = () => {
      const t = targets[Number(target.value)] ?? targets[0];
      const next: Annotation = { address: t.address, size: t.size, name: name.value.trim(), comment: comment.value.trim(), reviewed: reviewed.checked };
      this.editing = false;
      if (!next.name && !next.comment && !next.reviewed) {
        if (a) void store.removeAnnotation(a);
        else this.render();
      } else void store.annotate(next);
    };
    const cancel = () => {
      this.editing = false;
      this.render();
    };
    const form = h(
      'form',
      { class: 'note-form' },
      h('label', null, h('span', null, 'Applies to'), target),
      h('label', null, h('span', null, 'Name'), name),
      h('label', null, h('span', null, 'Comment'), comment),
      h('label', { class: 'check' }, reviewed, 'Reviewed — I understand this code'),
      h('div', { class: 'btn-row' }, h('button', { class: 'btn small primary', type: 'submit' }, 'Save'), h('button', { class: 'btn small', type: 'button', onclick: cancel }, 'Cancel')),
    );
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      save();
    });
    form.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cancel();
      } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        save();
      }
    });
    setTimeout(() => name.focus(), 0);
    return form;
  }

  private source(ins: Inspection): HTMLElement | null {
    if (!ins.source && ins.frames.length === 0) {
      if (ins.address !== undefined && store.file?.dwarf) {
        return h('div', { class: 'insp-section' }, h('h3', null, 'Source'), h('div', { class: 'muted' }, 'No line table entry covers this address.'));
      }
      return null;
    }
    const section = h('div', { class: 'insp-section' }, h('h3', null, 'Source'));
    if (ins.source) {
      const src = ins.source;
      const link = h('span', { class: 'link mono', title: src.path }, `${basename(src.path)}:${src.line}${src.column ? ':' + src.column : ''}`);
      link.addEventListener('click', () => store.openSource(src.file, src.line));
      section.appendChild(h('div', null, link, src.line === 0 ? h('span', { class: 'muted' }, '  (compiler-generated, no line)') : null));
      section.appendChild(h('div', { class: 'muted', style: 'font-size:12px;overflow-wrap:anywhere' }, src.path));
      const text = store.sources.get(src.file);
      if (text !== undefined && src.line > 0) {
        const lines = text.split(/\r?\n/);
        const from = Math.max(1, src.line - 2);
        const to = Math.min(lines.length, src.line + 2);
        const snippet = h('div', { class: 'code-snippet' });
        for (let l = from; l <= to; l++) {
          snippet.appendChild(h('span', { class: l === src.line ? 'hl' : '' }, `${String(l).padStart(5)}  ${lines[l - 1] ?? ''}\n`));
        }
        section.appendChild(snippet);
      } else if (src.line > 0) {
        const sf = store.file?.sourceFiles[src.file];
        section.appendChild(h('div', { class: 'muted', style: 'font-size:12px;margin-top:4px' }, sf?.embeddedSource ? 'Source is embedded — open it in Sources.' : 'Load the source folder (toolbar) to see the code here.'));
      }
    }
    if (ins.frames.length > 0) {
      section.appendChild(h('h3', { style: 'margin-top:12px' }, ins.frames.length > 1 ? 'Call stack (innermost first)' : 'Function'));
      const list = h('ul', { class: 'frames' });
      for (const f of ins.frames) {
        const where = f.file ? `${basename(f.file)}:${f.line ?? 0}` : '';
        const whereEl = h('span', { class: f.fileIndex !== undefined ? 'link' : '' }, where);
        if (f.fileIndex !== undefined) {
          const fi = f.fileIndex;
          whereEl.addEventListener('click', () => store.openSource(fi, f.line ?? 0));
        }
        list.appendChild(
          h(
            'li',
            { class: f.inlined ? 'inlined' : '' },
            h('div', { class: 'fn' }, f.demangled ?? f.function ?? '??'),
            h('div', { class: 'where' }, f.inlined ? 'inlined, ' : '', where ? 'at ' : '', whereEl),
          ),
        );
      }
      section.appendChild(list);
    }
    return section;
  }

  private instruction(ins: Inspection): HTMLElement | null {
    const i = ins.instruction;
    if (!i) return null;
    const target = i.target !== undefined ? h('span', { class: 'link' }, i.targetSymbol ? `<${i.targetSymbol}>` : hex(i.target)) : null;
    if (target && i.target !== undefined) {
      const t = i.target;
      target.addEventListener('click', () => void store.select({ address: t }, { view: 'code' }));
    }
    return h(
      'div',
      { class: 'insp-section' },
      h('h3', null, 'Instruction'),
      h('div', { class: 'mono' }, h('strong', null, i.mnemonic), ' ', i.operands),
      target ? h('div', { class: 'mono', style: 'margin-top:2px' }, i.flow === 'call' ? 'calls ' : i.flow.includes('jump') ? 'jumps to ' : 'refers to ', target) : null,
      h('div', { class: 'muted mono', style: 'margin-top:4px' }, `${i.bytes}  (${i.len} bytes at ${hex(i.address)})`),
    );
  }

  private actions(ins: Inspection): HTMLElement {
    const btn = (label: string, fn: () => void, disabled = false) => {
      const b = h('button', { class: 'btn small', disabled }, label);
      b.addEventListener('click', fn);
      return b;
    };
    const target = ins.address !== undefined ? { address: ins.address } : { offset: ins.offset };
    const bar = h('div', { style: 'display:flex;gap:6px;flex-wrap:wrap' });
    bar.append(
      btn('Hex', () => void store.select(target, { view: 'hex' }), ins.offset === undefined),
      btn('Code', () => void store.select(target, { view: 'code' }), !ins.instruction && !ins.symbol),
      btn('Layout', () => ins.offset !== undefined && store.revealRegion(ins.offset), ins.offset === undefined),
    );
    if (store.file?.dwarf && ins.address !== undefined) {
      const addr = ins.address;
      bar.append(
        btn('DWARF', async () => {
          const at = await store.api.dieAt(addr);
          if (at) store.openDie(at[0], at[1]);
          else if (ins.unit !== undefined) store.openDie(ins.unit, 0n);
        }),
      );
    }
    return h('div', { class: 'insp-section' }, h('h3', null, 'Show in'), bar);
  }
}

function short(name: string): string {
  return name.length > 40 ? name.slice(0, 39) + '…' : name;
}
