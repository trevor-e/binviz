// Compiler observations complement the original binary's call graph. Import
// through the shared Rust reader: no second schema validator or ABI inference.
import { store } from '../store';
import { h } from '../util';
import { View } from './base';

interface Signature { returnType: string; parameterTypes: string[]; variadic?: boolean }
interface Finding {
  caller: string; callee: string; source: string;
  kind: string;
  definitionAbi: Signature; declaredCallerAbi: Signature;
  actualCallCounts: (number | bigint)[]; allResultsDiscarded: boolean; oldStyle?: boolean;
  sourceSha256?: string; originalSha256?: string;
  [key: string]: unknown;
}
interface Artifact { id: string; role: string; location: string; sha256: string }
interface Check { id: string; state: string; contentState?: string; reasons: string[] }
interface Span { artifact: string; start: number | bigint; end: number | bigint; editable: boolean }
interface Witness { kind: string; reason: string; path: { pc: number; word?: number | null; delaySlot: boolean }[] }
interface Report {
  contracts: Finding[]; scope: string; cacheMisses: string[];
  evidence?: { artifacts: Artifact[]; stages: unknown[] };
  [key: string]: unknown;
}
interface WorkCall {
  call: { id: string; callee: string; arguments: unknown[]; resultConsumed: boolean; span: Span };
  binding?: { caller: string; provider: string; definition: string; providerKind: string; module: string; callerExport: string; providerExport: string; linkedCallOffset: string };
  state: string; reasons: string[]; findings: Finding[];
  policies: { policy: string; state: string; reasons: string[]; coveredFindings: string[]; equivalentTo?: string }[];
  audits: { register: number; provider?: { outcome: string; reads: Witness[]; endpoints: Witness[]; frontiers: Witness[] }; continuation?: { outcome: string; reads: Witness[]; endpoints: Witness[]; frontiers: Witness[] }; reasons: string[] }[];
  originalInstructions: { unit: string; pc: string; word: string; delaySlot: boolean }[];
}
interface WorkCaller { id: string; name: string; unit: string; sourceSpan: Span; calls: WorkCall[]; builds: unknown[]; rejected: boolean; addressReferences: unknown[]; extractionGaps: unknown[]; intrinsics: unknown[] }
interface StorageWork { objects: { description: { id: string }; identityState: string; accesses: { access: { object: string }; state: string; objectBytes: string; reservedBytes: string }[] }[]; native: unknown[]; linked: unknown[]; closureComplete: boolean }
interface WorkReport {
  nextActions: { state: string; summary: string; actions: { kind: string; subject: string; caller?: string; unit?: string; reason: string; evidence: string[]; cli: string[]; mcp: { tool: string; arguments: unknown } }[] };
  callers: WorkCaller[]; inventory: { units: { unit: { id: string; context: string; loadAddress: string; memberSize: string }; state: string; reasons: string[]; functions: { function: { id: string; identity: { unit: string } } }[]; gaps: unknown[] }[]; collisions: unknown[]; identityChecks: Check[] };
  contractReport: Report; blockers: { dependency: string; callers: string[]; rejectedCallers: number; observations: number }[];
  modules: Record<string, unknown>; layoutChecks: unknown[]; storage: StorageWork; promotionPlans: unknown[]; calleeCertificates: unknown[]; proofClosures: unknown[]; matchingPublications: unknown[]; readabilityBatches: unknown[]; adoption: unknown[]; callCoverage: unknown; rawObservations: number; rejectedCallers: number; unresolvedCallers: number;
}
interface CampaignReport { identityChecks: Check[]; observationsBound: boolean; requested: number; unexamined: number; executedPairs: number; failed: number; refused: number; coverage: string[]; comparisons: { id: string; result: { state: string; differences?: unknown[]; reason?: string; exclusions?: unknown[]; unspecifiedLocalBytes?: unknown } }[]; firstFailure?: unknown; statistics: unknown; runners: unknown; input: { evidence: Report['evidence']; [key: string]: unknown } }

function signature(s: Signature, oldStyle = false): string {
  const args = [...s.parameterTypes, ...(s.variadic ? ['...'] : [])];
  return `${s.returnType} (${oldStyle ? '' : args.join(', ') || 'void'})`;
}

// The shared WASM serializer preserves 64-bit metadata as BigInt.
function evidence(value: unknown): string {
  return JSON.stringify(value, (_key, item: unknown) => typeof item === 'bigint' ? item.toString() : item, 2);
}

// Export numeric JSON tokens for BigInt, preserving both serde count fields and
// exact 64-bit metadata. Display formatting above may quote them for readability.
function exportJson(value: unknown): string {
  if (typeof value === 'bigint') return value.toString();
  if (Array.isArray(value)) return `[${value.map(item => item === undefined ? 'null' : exportJson(item)).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.entries(value).filter(([, item]) => item !== undefined).map(([key, item]) => `${JSON.stringify(key)}:${exportJson(item)}`).join(',')}}`;
  }
  return JSON.stringify(value) ?? 'null';
}
function downloadText(name: string, content: string, type = 'application/json') {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = h('a', { href: url, download: name, style: 'display:none' });
  document.body.append(anchor); anchor.click(); anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export class ContractsView extends View {
  private report?: Report;
  private workspace?: WorkReport;
  private workspaceText = '';
  private campaign?: CampaignReport;
  private campaignText = '';
  private filename = '';
  private caller = '';
  private callee = '';
  private unit = '';
  private kind = '';
  private identity = '';
  private checks: Check[] = [];
  private files: Record<string, Uint8Array> = {};
  private sequence = 0;
  private status!: HTMLElement;
  private filters!: HTMLElement;
  private results!: HTMLElement;

  constructor() { super('contracts'); }

  private identityFor(row: Finding): string {
    if (!row.sourceSpan) return 'unverified';
    const ids = [String(row.unit) + ':extract'];
    if (typeof row.providerId === 'string') ids.push(row.providerId.split(':decl:')[0] + ':extract');
    const states = ids.map(id => this.checks.find(c => c.id === id)?.state ?? 'unverified');
    return states.find(s => s === 'missing') ?? states.find(s => s === 'stale') ?? states.find(s => s === 'unsupported') ?? (states.every(s => s === 'verified') ? 'verified' : 'unverified');
  }

  protected render() {
    const input = h('input', { type: 'file', accept: '.json,application/json', style: 'display:none' });
    const load = h('button', { type: 'button', class: 'btn' }, 'Import compiler report…');
    load.addEventListener('click', () => input.click());
    const verify = h('button', { type: 'button', class: 'btn' }, 'Verify local artifact…');
    verify.addEventListener('click', () => this.verifyPicker());
    const folder = h('input', { type: 'file', webkitdirectory: '', multiple: true, style: 'display:none' });
    const verifyFolder = h('button', { type: 'button', class: 'btn' }, 'Verify artifact folder…');
    verifyFolder.addEventListener('click', () => folder.click());
    folder.addEventListener('change', () => { void this.verifyFolder(Array.from(folder.files ?? [])); folder.value = ''; });
    const exportButton = h('button', { type: 'button', class: 'btn' }, 'Export selected evidence');
    const buildInput = h('input', { type: 'file', accept: '.json', style: 'display:none' });
    const buildButton = h('button', { type: 'button', class: 'btn' }, 'Import build outcomes…');
    buildButton.addEventListener('click', () => buildInput.click());
    buildInput.addEventListener('change', async () => {
      const file = buildInput.files?.[0]; buildInput.value = '';
      if (!file || !this.workspaceText) { this.status.textContent = 'Import a workspace before adding build outcomes.'; return; }
      const sequence = this.sequence;
      try {
        if (file.size > 16 * 1024 * 1024) throw new Error('Build report exceeds 16 MiB.');
        const text = await store.api.call<string>('workspaceMergeBuilds', this.workspaceText, await file.text());
        const report = await store.api.call<WorkReport>('workspaceAnalyze', text, this.files);
        if (sequence !== this.sequence) return;
        this.workspaceText = text; this.workspace = report; this.checks = report.inventory.identityChecks;
        this.report!.evidence = (JSON.parse(text) as { evidence: Report['evidence'] }).evidence;
        this.draw();
      } catch (e) { this.status.textContent = `Build import refused: ${String(e)}`; }
    });
    const planButton = h('button', { type: 'button', class: 'btn' }, 'Plan call adapters');
    planButton.addEventListener('click', async () => {
      if (!this.workspaceText) { this.status.textContent = 'Import and verify a workspace manifest to plan adapters.'; return; }
      try {
        const plan = await store.api.call<{ candidates: { artifact: string; source: string; beforeSha256: string; afterSha256: string; edits: unknown[] }[]; refused: unknown[] }>('adapterPlan', this.workspaceText, this.files, []);
        const output = h('div', { class: 'card' }, h('h3', null, 'Reviewable adapter plan'), h('p', null, `${plan.candidates.length} prepared candidates · ${plan.refused.length} refusals`));
        const download = (label: string, name: string, content: string) => {
          const button = h('button', { class: 'btn', type: 'button' }, label);
          button.addEventListener('click', () => downloadText(name, content));
          return button;
        };
        output.append(download('Export plan JSON', 'binviz-edit-plan.json', exportJson(plan)));
        for (const c of plan.candidates) output.append(h('details', null, h('summary', null, c.artifact), h('p', { class: 'mono' }, `${c.beforeSha256} → ${c.afterSha256}`), h('pre', { class: 'contract-evidence' }, evidence(c.edits)), h('pre', { class: 'contract-evidence' }, c.source), download('Download prepared candidate', c.artifact.replaceAll(/[^a-zA-Z0-9_.-]/g, '_') + '.c', c.source)));
        output.append(h('pre', { class: 'contract-evidence' }, evidence(plan.refused)));
        this.results.prepend(output);
      } catch (e) { this.status.textContent = `Plan refused: ${String(e)}`; }
    });
    exportButton.addEventListener('click', () => {
      if (!this.report) return;
      const rows = this.selected();
      const data = this.campaign ? exportJson(this.campaign) : this.workspace ? exportJson({ ...this.workspace, callers: this.workspace.callers.filter(c => (!this.caller || c.id === this.caller) && (!this.unit || c.unit === this.unit)), inputManifest: this.workspaceText }) : exportJson({ ...this.report, contracts: rows, contractCount: rows.length, distinctCallees: new Set(rows.map(r => r.callee)).size, identityChecks: this.checks });
      downloadText('binviz-call-evidence.json', data);
    });
    input.addEventListener('change', () => {
      const file = input.files?.[0];
      input.value = '';
      if (file) void this.load(file);
    });
    this.status = h('p', { role: 'status', class: 'sub' });
    this.filters = h('div', { class: 'toolbar' });
    this.results = h('div');
    this.el.replaceChildren(h('div', { class: 'page contracts-page' },
      h('div', { class: 'card' }, h('h2', null, 'Call contracts'),
        h('p', { class: 'sub' }, 'Compare reconstructed C calls with actual compiler definitions. Load an all-caller contract report generated by your decompilation build.'),
        load, input, verify, verifyFolder, folder, exportButton, planButton, buildButton, buildInput, this.status,
        h('p', { class: 'muted' }, 'Imported observations are independent of the open binary. Artifact verification checks bytes and stage lineage; native behavior and policy eligibility remain separate.')),
      this.filters, this.results, this.registerAudits()));
    this.draw();
  }

  private async load(file: File) {
    const sequence = ++this.sequence;
    this.status.textContent = `Reading ${file.name}…`;
    try {
      if (file.size > 16 * 1024 * 1024) throw new Error('Compiler report exceeds 16 MiB.');
      let text = await file.text();
      const imported = JSON.parse(text) as { format?: string; inputManifest?: string; input?: unknown };
      if (imported.format === 'binviz-workspace-report' && typeof imported.inputManifest === 'string') text = imported.inputManifest;
      const format = (JSON.parse(text) as { format?: string }).format;
      const isCampaign = format === 'binviz-campaign' || format === 'binviz-campaign-report';
      const campaign = isCampaign ? await store.api.call<CampaignReport>('campaignReport', text, null) : undefined;
      const isWorkspace = (JSON.parse(text) as { format?: string }).format === 'binviz-workspace';
      const workspace = isWorkspace ? await store.api.call<WorkReport>('workspaceAnalyze', text, null) : undefined;
      const report = campaign ? { contracts: [], scope: 'Configured runner observations', cacheMisses: [], evidence: campaign.input.evidence } : workspace?.contractReport ?? await store.api.call<Report>('callContractsParse', text);
      if (sequence !== this.sequence) return;
      this.report = report;
      this.workspace = workspace;
      this.workspaceText = isWorkspace ? text : '';
      this.campaign = campaign; this.campaignText = isCampaign ? text : '';
      if (isWorkspace) this.report.evidence = (JSON.parse(text) as { evidence: Report['evidence'] }).evidence;
      this.filename = file.name;
      this.caller = this.callee = this.unit = this.kind = this.identity = '';
      this.checks = campaign?.identityChecks ?? workspace?.inventory.identityChecks ?? [];
      this.files = {};
      this.draw();
    } catch (e) {
      if (sequence !== this.sequence) return;
      this.status.textContent = `Could not import ${file.name}: ${e instanceof Error ? e.message : String(e)}${this.report ? ' Previous report retained.' : ''}`;
    }
  }

  private registerAudits(): HTMLElement {
    const input = h('textarea', { class: 'field mono', rows: '7', 'aria-label': 'Register audit batch JSON', placeholder: '{"schemaVersion":1,"requests":[{"id":"callee-a2","address":"0x80010000","bytes":8,"entry":"0x80010000","register":"a2"}]}' });
    const run = h('button', { class: 'btn', type: 'button', disabled: !store.file }, 'Run native register audits');
    const output = h('div', { role: 'status' });
    const batchFile = h('input', { type: 'file', accept: '.json', 'aria-label': 'Register audit requests file' });
    batchFile.addEventListener('change', async () => {
      const file = batchFile.files?.[0];
      if (!file) return;
      if (file.size > 4 * 1024 * 1024) { output.textContent = 'Request file exceeds 4 MiB.'; return; }
      input.value = await file.text();
    });
    run.addEventListener('click', async () => {
      run.disabled = true;
      output.textContent = 'Auditing exact loaded extents…';
      const binary = store.file;
      try {
        const result = await store.api.call<{ requests: { id: string; report: { outcome: string; register: number; reads: Witness[]; endpoints: Witness[]; frontiers: Witness[] } }[] }>('registerUseBatch', input.value);
        if (store.file !== binary) return;
        output.replaceChildren(...result.requests.map(r => h('details', { class: 'card' },
          h('summary', null, `${r.id}: register ${r.report.register} · ${r.report.outcome}`),
          ...[...r.report.reads, ...r.report.endpoints, ...r.report.frontiers].map(w => h('details', null,
            h('summary', null, `${w.kind}: ${w.reason}`),
            ...w.path.map(s => {
              const button = h('button', { type: 'button', class: 'btn mono', title: 'Inspect original instruction' }, `0x${s.pc.toString(16)}  ${s.word === undefined || s.word === null ? 'outside extent' : '0x' + s.word.toString(16).padStart(8, '0')}${s.delaySlot ? ' [delay slot]' : ''}`);
              button.addEventListener('click', () => void store.goTo({ view: 'code', target: { address: BigInt(s.pc) } }, 'push'));
              return h('div', null, button);
            }))),
          h('details', null, h('summary', null, 'Full audit, assumptions and witnesses'), h('pre', { class: 'contract-evidence' }, evidence(r.report))))));
      } catch (e) { output.textContent = `Audit refused: ${e instanceof Error ? e.message : String(e)}`; }
      finally { run.disabled = !store.file; }
    });
    return h('details', { class: 'card' }, h('summary', null, 'Native register audit paths'),
      h('p', { class: 'sub' }, 'Load a PS1 binary, then supply exact extents and per-request policies. Unknown paths remain unresolved; discarded returns remain distinct from instruction kills.'), input, batchFile, run, output);
  }

  private selected(): Finding[] {
    return this.report?.contracts.filter(r => (!this.caller || r.caller === this.caller) && (!this.callee || r.callee === this.callee) && (!this.unit || r.unit === this.unit) && (!this.kind || r.kind === this.kind) && (!this.identity || this.identityFor(r) === this.identity)) ?? [];
  }

  private verifyPicker() {
    const manifest = this.report?.evidence;
    if (!manifest) { this.status.textContent = 'This legacy report has no artifact manifest. Its observations remain unverified.'; return; }
    const select = h('select', { class: 'field', 'aria-label': 'Artifact to verify' }, manifest.artifacts.map(a => h('option', { value: a.id }, `${a.role}: ${a.location}`)));
    const input = h('input', { type: 'file', 'aria-label': 'Local artifact bytes' });
    const dialog = h('dialog', { class: 'card' }, h('h3', null, 'Verify actual artifact bytes'), h('p', null, 'Choose the manifest artifact and its current local file. Repeat for dependencies to verify a complete stage.'), select, input);
    const close = h('button', { type: 'button', class: 'btn' }, 'Close');
    close.addEventListener('click', () => dialog.close());
    dialog.append(close);
    dialog.addEventListener('close', () => dialog.remove());
    input.addEventListener('change', async () => {
      const file = input.files?.[0];
      if (!file) return;
      const id = select.value;
      const sequence = this.sequence;
      try {
        if (file.size > 128 * 1024 * 1024) throw new Error('Artifact exceeds 128 MiB. Use the desktop evidence command for larger files.');
        const bytes = new Uint8Array(await file.arrayBuffer());
        if (sequence !== this.sequence) { dialog.close(); return; }
        const files = { ...this.files, [id]: bytes };
        const workspace = this.workspaceText ? await store.api.call<WorkReport>('workspaceAnalyze', this.workspaceText, files) : undefined;
        const campaign = this.campaignText ? await store.api.call<CampaignReport>('campaignReport', this.campaignText, files) : undefined;
        const checks = campaign?.identityChecks ?? workspace?.inventory.identityChecks ?? await store.api.call<Check[]>('evidenceVerify', evidence(manifest), files);
        if (sequence !== this.sequence) return;
        this.files = files;
        this.checks = checks;
        this.workspace = workspace;
        this.campaign = campaign;
        this.draw();
        this.status.textContent += ` · ${id}: ${checks.find(c => c.id === id)?.state}`;
        input.value = '';
      } catch (e) { this.status.textContent = `Verification failed: ${e instanceof Error ? e.message : String(e)}`; }
    });
    document.body.append(dialog);
    dialog.showModal();
  }

  private async verifyFolder(selected: File[]) {
    const manifest = this.report?.evidence;
    if (!manifest) { this.status.textContent = 'Import a manifest with artifact identities first.'; return; }
    const sequence = this.sequence;
    const files = { ...this.files };
    const ambiguous: string[] = [];
    let count = 0;
    try {
      for (const a of manifest.artifacts) {
        const location = a.location.replaceAll('\\', '/').replace(/^(\.\.\/|\.\/)+/, '');
        let candidates = selected.filter(f => f.webkitRelativePath.endsWith('/' + location));
        if (!candidates.length) candidates = selected.filter(f => f.name === location.split('/').at(-1));
        if (candidates.length > 1) { ambiguous.push(a.id); continue; }
        const file = candidates[0];
        if (!file || file.size > 128 * 1024 * 1024) continue;
        this.status.textContent = `Reading artifact ${a.id}…`;
        files[a.id] = new Uint8Array(await file.arrayBuffer());
        count++;
        if (sequence !== this.sequence) return;
      }
      const workspace = this.workspaceText ? await store.api.call<WorkReport>('workspaceAnalyze', this.workspaceText, files) : undefined;
      const campaign = this.campaignText ? await store.api.call<CampaignReport>('campaignReport', this.campaignText, files) : undefined;
      const checks = campaign?.identityChecks ?? workspace?.inventory.identityChecks ?? await store.api.call<Check[]>('evidenceVerify', evidence(manifest), files);
      if (sequence !== this.sequence) return;
      this.files = files; this.workspace = workspace; this.campaign = campaign; this.checks = checks; this.draw();
      this.status.textContent += ` · supplied ${count} artifacts${ambiguous.length ? '; ambiguous paths need individual selection: ' + ambiguous.join(', ') : ''}`;
    } catch (e) { this.status.textContent = `Verification refused: ${String(e)}`; }
  }

  private source(row: Finding): HTMLElement {
    const span = row.sourceSpan as Span | undefined;
    if (!span) return h('p', { class: 'muted' }, 'No compiler source span supplied.');
    const bytes = this.files[span.artifact];
    const check = this.checks.find(c => c.id === span.artifact);
    const description = `${span.artifact}: bytes ${span.start}–${span.end}${span.editable ? '' : ' (non-editable span)'}${check && check.state !== 'verified' ? ` · lineage ${check.state}` : ''}`;
    if (!bytes || (check?.contentState ?? check?.state) !== 'verified') return h('p', { class: 'mono' }, description, ' · Verify the local artifact to inspect source.');
    const start = Number(span.start), end = Number(span.end);
    if (end > bytes.length) return h('p', { class: 'muted' }, 'Source span exceeds supplied bytes.');
    const decode = (a: number, b: number) => new TextDecoder().decode(bytes.subarray(a, b));
    return h('details', null, h('summary', null, description), h('pre', { class: 'contract-evidence' }, decode(Math.max(0, start - 120), start), h('mark', null, decode(start, end)), decode(end, Math.min(bytes.length, end + 120))));
  }

  private picker(label: string, values: string[], selected: string, change: (value: string) => void) {
    const select = h('select', { class: 'field', 'aria-label': label },
      h('option', { value: '' }, label === 'Identity' ? 'All identity states' : `All ${label.toLowerCase()}s`),
      [...new Set(values)].sort().map(value => h('option', { value }, value)));
    select.value = selected;
    select.addEventListener('change', () => { change(select.value); this.draw(); });
    return h('label', null, label, ' ', select);
  }

  private draw() {
    if (this.campaign) { this.drawCampaign(); return; }
    if (this.workspace) { this.drawWorkspace(); return; }
    const report = this.report;
    if (!report) {
      this.status.textContent = 'No compiler report imported.';
      this.filters.replaceChildren();
      this.results.replaceChildren();
      return;
    }
    const rows = this.selected();
    this.status.textContent = `${this.filename} · ${rows.length} of ${report.contracts.length} findings · ${report.cacheMisses.length} uncached callers`;
    this.filters.replaceChildren(
      this.picker('Caller', report.contracts.map(r => r.caller), this.caller, v => this.caller = v),
      this.picker('Callee', report.contracts.map(r => r.callee), this.callee, v => this.callee = v),
      this.picker('Unit', report.contracts.map(r => String(r.unit ?? '')).filter(Boolean), this.unit, v => this.unit = v),
      this.picker('Kind', report.contracts.map(r => r.kind), this.kind, v => this.kind = v),
      this.picker('Identity', ['unverified', 'verified', 'stale', 'missing', 'unsupported'], this.identity, v => this.identity = v));
    this.results.replaceChildren(
      h('p', { class: 'sub' }, `Reported scope: ${report.scope}`),
      ...rows.map(r => h('details', { class: 'card contract-finding' },
        h('summary', null, h('span', { class: 'mono' }, `${r.caller} → ${r.callee}`),
          ' · ', r.kind, ' · ', this.identityFor(r)),
        h('p', { class: 'sub' }, String(r.reason ?? (r.kind === 'void-result' ? 'Consumes void result' : `Supplied ${r.actualCallCounts.join('/')} args; definition takes ${r.definitionAbi.parameterTypes.length}`))),
        h('dl', { class: 'facts' },
          h('dt', null, 'Source'), h('dd', { class: 'mono' }, r.source),
          h('dt', null, 'Caller declaration'), h('dd', { class: 'mono' }, signature(r.declaredCallerAbi, r.oldStyle), r.oldStyle ? ' [unspecified parameter list]' : ''),
          h('dt', null, 'Actual definition'), h('dd', { class: 'mono' }, signature(r.definitionAbi)),
          h('dt', null, 'Observed result use'), h('dd', null, r.allResultsDiscarded ? 'Every observed result discarded' : 'At least one result consumed')),
        this.source(r),
        h('details', null, h('summary', null, 'Reported evidence and metadata'), h('pre', { class: 'contract-evidence' }, evidence(r))))),
      ...(rows.length === 0 ? [h('p', { class: 'sub' }, 'No findings match these exact names.')] : []),
      ...(report.cacheMisses.length ? [h('div', { class: 'card' }, h('h3', null, 'Uncached callers'), h('p', { class: 'mono' }, report.cacheMisses.join(', ')))] : []),
      ...(this.checks.length ? [h('details', { class: 'card' }, h('summary', null, 'Artifact identities and stage lineage'), ...this.checks.map(c => h('p', { class: 'mono' }, `${c.id}: ${c.state}${c.reasons.length ? ' · ' + c.reasons.join('; ') : ''}`)))] : []),
      h('details', { class: 'card' }, h('summary', null, 'Report metadata'), h('pre', { class: 'contract-evidence' },
        evidence(Object.fromEntries(Object.entries(report).filter(([k]) => k !== 'contracts'))))));
  }

  private drawCampaign() {
    const report = this.campaign!;
    this.status.textContent = `${this.filename} · ${report.executedPairs} executed pairs of ${report.requested} requested · ${report.failed} failures · ${report.refused} refusals · ${report.unexamined} unexamined · observations ${report.observationsBound ? 'bound to supplied bytes' : 'unverified'} · lineage ${report.identityChecks.length && report.identityChecks.every(c => c.state === 'verified') ? 'current' : 'unverified'}`;
    this.filters.replaceChildren();
    const full = (label: string, value: unknown) => h('details', { class: 'card' }, h('summary', null, label), h('pre', { class: 'contract-evidence' }, evidence(value)));
    this.results.replaceChildren(h('h3', null, 'Differential campaign'),
      full('First failure and reproduction fixture', report.firstFailure), full('Runner versions and real/controlled provider profiles', report.runners),
      ...report.comparisons.map(c => h('details', { class: 'card' }, h('summary', { class: 'mono' }, `${c.id}: ${c.result.state}`), h('p', null, c.result.reason ?? ''), h('pre', { class: 'contract-evidence' }, evidence(c.result)),
        full('Participant observations and fixture', (report.input.cases as { id: string }[]).find(v => v.id === c.id)))),
      full('Observed coverage', report.coverage), full('Measured execution statistics', report.statistics), full('Actual artifacts and execution lineage', report.identityChecks), full('Configuration and explicit exclusions', report.input.configuration));
  }

  private drawWorkspace() {
    const report = this.workspace!;
    this.status.textContent = `${this.filename} · ${report.rawObservations} raw observations · ${report.rejectedCallers} currently rejected callers · ${report.unresolvedCallers} unresolved packages`;
    this.filters.replaceChildren(
      this.picker('Caller', report.callers.map(c => c.id), this.caller, v => this.caller = v),
      this.picker('Unit', report.callers.map(c => c.unit), this.unit, v => this.unit = v));
    const full = (label: string, value: unknown) => h('details', null, h('summary', null, label), h('pre', { class: 'contract-evidence' }, evidence(value)));
    const unitFor = (id?: string) => report.inventory.units.flatMap(u => u.functions).find(f => f.function.id === id)?.function.identity.unit ?? '';
    const nativeButton = (unit: string, pc: string, label: string) => {
      const button = h('button', { class: 'btn mono', type: 'button' }, label);
      button.addEventListener('click', async () => {
        try {
          if (!store.file || !await store.api.call<boolean>('workspaceUnitMatches', this.workspaceText, unit)) {
            this.status.textContent = `Load the exact ${unit} member or asset at its configured address to inspect this instruction. Equal overlay addresses cannot establish identity.`; return;
          }
          await store.goTo({ view: 'code', target: { address: BigInt(pc) } }, 'push');
        } catch (e) { this.status.textContent = String(e); }
      });
      return button;
    };
    const drawAudit = (a: WorkCall['audits'][number], site: WorkCall) => {
      const panels = ([['Provider', a.provider, site.binding?.provider], ['Caller continuation', a.continuation, site.binding?.caller]] as const).map(([label, audit, owner]) => {
        const witnesses = [...(audit?.reads ?? []), ...(audit?.endpoints ?? []), ...(audit?.frontiers ?? [])];
        return h('details', null, h('summary', null, label), ...witnesses.map(w =>
          h('details', null, h('summary', null, `${w.kind}: ${w.reason}`), ...w.path.map(step =>
            h('p', null, nativeButton(unitFor(owner), `0x${step.pc.toString(16)}`, `0x${step.pc.toString(16)} ${step.word?.toString(16) ?? 'outside extent'}${step.delaySlot ? ' [delay slot]' : ''}`))))));
      });
      return h('details', null, h('summary', null, `Register ${a.register}: provider ${a.provider?.outcome ?? 'not tested'}; post-call ${a.continuation?.outcome ?? 'unresolved'}`),
        h('p', null, a.reasons.join('; ')), ...panels, full('Complete audit and assumptions', a));
    };
    this.results.replaceChildren(
      h('section', { class: 'card' }, h('h3', null, 'Next actions'),
        h('p', null, report.nextActions.summary),
        ...report.nextActions.actions.filter(a => (!this.caller || !a.caller || a.caller === this.caller) && (!this.unit || !a.unit || a.unit === this.unit)).slice(0, 10).map(a =>
          h('details', null, h('summary', null, `${a.kind}: ${a.subject}`), h('p', null, a.reason),
            full('Evidence locations', a.evidence), full('CLI arguments', a.cli), full('MCP follow-up', a.mcp))),
        full('Complete next-action report (first 10 shown above)', report.nextActions)),
      h('details', { class: 'card' }, h('summary', null, `Physical ownership · ${report.inventory.units.length} units · ${report.inventory.collisions.length} collisions`),
        ...report.inventory.units.map(u => h('details', null, h('summary', { class: 'mono' }, `${u.unit.id} · ${u.state} · ${u.unit.context} · ${u.unit.loadAddress} + ${u.unit.memberSize}`),
          h('p', null, u.reasons.join('; ')), full('Analysis extents, matching extents and aliases', u.functions), full('Gaps and data exclusions', u.gaps))), full('Ownership collisions and decisions', report.inventory.collisions)),
      h('details', { class: 'card' }, h('summary', null, 'Ranked outstanding dependencies'), ...report.blockers.map(b => h('p', { class: 'mono' }, `${b.dependency}: ${b.callers.length} distinct callers, ${b.rejectedCallers} currently rejected, ${b.observations} observations`))),
      ...report.callers.filter(c => (!this.caller || c.id === this.caller) && (!this.unit || c.unit === this.unit)).map(c => h('details', { class: 'card caller-package', open: !!this.caller },
        h('summary', null, `${c.name} · ${c.id} · ${c.rejected ? 'build rejected' : 'inspect build outcome'} · ${c.calls.length} calls`),
        this.source({ sourceSpan: c.sourceSpan } as unknown as Finding), full('Current preparation, compilation and linking', c.builds),
        ...c.calls.map(site => h('details', { class: 'contract-finding', open: true },
          h('summary', { class: 'mono' }, `${site.call.id} → ${site.call.callee} · ${site.state}`),
          h('p', null, `${site.call.arguments.length} supplied arguments · result ${site.call.resultConsumed ? 'consumed' : 'discarded'}`),
          this.source({ sourceSpan: site.call.span } as unknown as Finding),
          h('p', null, site.reasons.join('; ')),
          ...(site.binding ? [h('p', { class: 'mono' }, `Selected ${site.binding.providerKind}: ${site.binding.provider} · definition ${site.binding.definition} · ${site.binding.module}: ${site.binding.callerExport} → ${site.binding.providerExport} at ${site.binding.linkedCallOffset}`)] : [h('p', null, 'Provider correspondence unresolved.')]),
          ...site.originalInstructions.map(i => h('p', null, nativeButton(i.unit, i.pc, `${i.pc} ${i.word}${i.delaySlot ? ' [delay slot]' : ''}`))),
          ...site.findings.map(f => h('p', { class: 'mono' }, `${f.kind}: declared ${signature(f.declaredCallerAbi, f.oldStyle)}; definition ${signature(f.definitionAbi)}`)),
          ...site.policies.map(p => h('details', null, h('summary', null, `${p.policy}: ${p.state}${p.equivalentTo ? ' · equivalent to ' + p.equivalentTo : ''}`), h('p', null, p.reasons.join('; ')), full('Covered observations', p.coveredFindings))),
          ...site.audits.map(a => drawAudit(a, site)),
          full('Full call package', site))),
        full('Callback and address references', c.addressReferences), full('Compiler intrinsics and exception effects', c.intrinsics), full('Extraction gaps and frontiers', c.extractionGaps))),
      full('Actual linked modules and signatures', report.modules), full('Memory and data allocation certificates', report.layoutChecks),
      ...(report.storage.objects.length ? [h('div', { class: 'card' }, h('h3', null, 'Object and access extents'),
        ...report.storage.objects.flatMap(d => d.accesses.map(a => h('p', { class: 'mono' }, `${d.description.id}/${a.access.object}: ${a.state} · object ${a.objectBytes} bytes · reservation ${a.reservedBytes} bytes · evidence ${d.identityState}`))),
        h('p', { class: 'sub' }, 'Reviewed access descriptions retain their stated scope. Unknown closure paths remain incomplete.'))] : []),
      full('Storage, initialized bytes and provider access', report.storage), full('Before/candidate evidence promotion plans', report.promotionPlans),
      full('Native callee certificates and endpoint lineage', report.calleeCertificates),
      full('Provider, initialization, callback and stack closures', report.proofClosures),
      full('Scoped matching publication plans', report.matchingPublications),
      full('Readability batch acceptance', report.readabilityBatches),
      full('SDK, snapshot, dependency and source mapping evidence', report.adoption),
      full('Source calls and unique physical native calls', report.callCoverage),
      full('Current artifact identities and stage lineage', this.checks));
  }
}
