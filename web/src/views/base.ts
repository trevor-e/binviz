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
    store.on('viewstate', (view) => {
      if (view === this.name && this.visible && !this.dirty) this.onState();
    });
  }

  get visible(): boolean {
    return store.view === this.name && (store.file !== null || this.name === 'contracts');
  }

  show() {
    if (this.dirty) {
      this.dirty = false;
      this.render();
    } else this.onState();
    this.onSelection();
  }

  /** Renders again (now, if shown). */
  protected invalidate() {
    this.dirty = true;
    if (this.visible) this.show();
  }

  /** Builds the view for the current file (its state, `store.viewState[name]`, included). */
  protected abstract render(): void;

  /** Reacts to the global selection. */
  protected onSelection(): void {}

  /** Shows the view's state as history left it (`store.viewState[name]`): a tab… */
  protected onState(): void {}
}

/** Part of a view (a tab): built when first shown, and again after a new file. */
export interface Panel {
  readonly el: HTMLElement;
  /** A new file: what was shown is gone. */
  reset(): void;
  /** Shown: builds what is missing. */
  show(): void;
  /** The selection changed while shown. */
  onSelection?(): void;
}
