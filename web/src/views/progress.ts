import { store } from '../store';
import { downloadBlob, downloadText, toast } from '../ui';
import { basename, h } from '../util';
import { View } from './base';

/** The native renderer is shared with CLI and MCP image exports. */
export class ProgressView extends View {
  private includeLibrary = false;
  private width = 1600;
  private height = 900;
  private revision = 0;
  private svg = '';

  constructor() {
    super('progress');
    store.on('annotations', () => this.invalidate());
  }

  protected render() {
    const revision = ++this.revision;
    const file = store.file!;
    this.svg = '';
    const image = h('div', { class: 'progress-treemap' });
    const status = h('p', { class: 'sub', role: 'status' }, 'Generating progress treemap…');
    const png = h('button', { class: 'btn', type: 'button', disabled: true }, 'Export PNG');
    const svg = h('button', { class: 'btn', type: 'button', disabled: true }, 'Export SVG');
    const notesInput = h('input', { type: 'file', accept: '.json,application/json', style: 'display:none' });
    const importNotes = h('button', { class: 'btn', type: 'button' }, 'Import progress notes…');
    importNotes.addEventListener('click', () => notesInput.click());
    notesInput.addEventListener('change', async () => {
      const notes = notesInput.files?.[0];
      notesInput.value = '';
      if (!notes) return;
      try {
        const text = await notes.text();
        if (store.file !== file) return;
        const count = await store.importAnnotations(text, notes.name);
        toast(count > 0 ? `Imported ${count} annotations from ${notes.name}` : `No annotations found in ${notes.name}`, count > 0 ? 'info' : 'error');
      } catch (error) {
        toast(`Could not import notes: ${error}`, 'error');
      }
    });
    const library = h('input', { type: 'checkbox', checked: this.includeLibrary });
    library.addEventListener('change', () => { this.includeLibrary = library.checked; this.invalidate(); });
    const size = h('select', { 'aria-label': 'Image size' },
      h('option', { value: '1600', selected: this.width === 1600 }, '1600 × 900'),
      h('option', { value: '2560', selected: this.width === 2560 }, '2560 × 1440'),
    );
    size.addEventListener('change', () => { this.width = Number(size.value); this.height = this.width === 1600 ? 900 : 1440; this.invalidate(); });
    svg.addEventListener('click', () => downloadText(`${basename(file.name)}-progress.svg`, this.svg, 'image/svg+xml'));
    png.addEventListener('click', () => void this.exportPng(file.name));
    this.el.replaceChildren(h('div', { class: 'page' },
      h('h2', null, 'Decompilation progress'),
      h('p', { class: 'sub' }, 'Each tile is a function, sized by code bytes and colored by its recorded status. Hover for details; select a tile to inspect its code.'),
      h('p', { class: 'secondary' }, 'Matched means the recorded C compiles to the same bytes. Nonmatching C is shown separately. Library code is excluded from game progress totals. Functions are grouped by recorded source file, or by section when no source is recorded.'),
      h('div', { class: 'btn-row' }, png, svg, importNotes, notesInput, h('label', null, library, ' Show library code'), h('label', null, 'Image size ', size)),
      status, image,
    ));
    void store.api.progressSvg(this.width, this.height, this.includeLibrary).then((text) => {
      if (revision !== this.revision || store.file !== file) return;
      this.svg = text;
      // Native output XML-escapes names and contains no external resources.
      const document = new DOMParser().parseFromString(text, 'image/svg+xml');
      const root = document.documentElement;
      root.setAttribute('role', 'group');
      for (const tile of root.querySelectorAll<SVGGElement>('g[data-address]')) {
        tile.setAttribute('tabindex', '0');
        tile.setAttribute('role', 'link');
        tile.setAttribute('aria-label', tile.querySelector('title')?.textContent ?? 'Inspect function');
        const open = () => void store.select({ address: BigInt(tile.getAttribute('data-address')!) }, { view: 'code' });
        tile.addEventListener('click', open);
        tile.addEventListener('keydown', (event) => {
          if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); open(); }
        });
      }
      image.replaceChildren(root);
      status.textContent = root.querySelector('desc')?.textContent ?? '';
      png.disabled = svg.disabled = false;
    }).catch((error) => {
      if (revision === this.revision) status.textContent = `Could not generate progress: ${error}`;
    });
  }

  private async exportPng(name: string) {
    const text = this.svg;
    if (!text) return;
    const url = URL.createObjectURL(new Blob([text], { type: 'image/svg+xml' }));
    try {
      const image = new Image();
      image.src = url;
      await image.decode();
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      const context = canvas.getContext('2d');
      if (!context) throw new Error('PNG rendering is unavailable');
      context.drawImage(image, 0, 0);
      const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob((value) => value ? resolve(value) : reject(new Error('PNG encoding failed')), 'image/png'));
      downloadBlob(`${basename(name)}-progress.png`, blob);
    } catch (error) {
      toast(`Could not export PNG: ${error}`, 'error');
    } finally {
      URL.revokeObjectURL(url);
    }
  }
}
