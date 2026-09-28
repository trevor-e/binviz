import { store, type ViewName } from '../store';
import { h } from '../util';

/** A main-area view. Rendering is deferred until the view is first shown. */
export abstract class View {
  readonly el: HTMLElement;
  private dirty = true;

  constructor(
    readonly name: ViewName,
    fill = false,
  ) {
    this.el = h('section', { class: `view view-${name}${fill ? ' fill' : ''}`, 'aria-label': name });
    store.on('file', () => {
      this.dirty = true;
      if (this.visible) this.show();
    });
    store.on('selection', () => {
      if (this.visible && !this.dirty) this.onSelection();
    });
  }

  get visible(): boolean {
    return store.view === this.name && store.file !== null;
  }

  show() {
    if (this.dirty) {
      this.dirty = false;
      this.render();
    }
    this.onSelection();
  }

  /** Renders again (now, if shown). */
  protected invalidate() {
    this.dirty = true;
    if (this.visible) this.show();
  }

  /** Builds the view for the current file. */
  protected abstract render(): void;

  /** Reacts to the global selection. */
  protected onSelection(): void {}
}
