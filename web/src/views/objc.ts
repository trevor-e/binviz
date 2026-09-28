// Objective-C selectors: where one is implemented and which functions send it,
// for the Classes tab and the inspector.
import { store } from '../store';
import type { SelectorUses } from '../types';
import { formatCount, h, hex } from '../util';

/** The selector of an Objective-C method's name: `-[Greeter greetWith:times:]` sends `greetWith:times:`. */
export function objcSelectorOf(name: string): string | undefined {
  return /^[-+]\[[^\s\]]+ ([^\s\]]+)\]$/.exec(name)?.[1];
}

/** Rows saying who implements and who sends `selector`. */
export function selectorUses(selector: string, uses: SelectorUses | null): HTMLElement[] {
  const rows: HTMLElement[] = [];
  const link = (label: string, address: bigint, title: string) => {
    const a = h('span', { class: 'link mono', title }, label);
    a.addEventListener('click', () => void store.select({ address }, { view: 'code' }));
    return a;
  };
  if (!uses) return [h('div', { class: 'muted' }, `Nothing in this image refers to ${selector}.`)];
  // Other classes answering the same message (one implementation is the method itself).
  if (uses.implementations.length > 1) {
    rows.push(h('div', { class: 'ref-sum' }, `Implemented by ${formatCount(uses.implementations.length)} methods`));
    for (const m of uses.implementations.slice(0, 50)) rows.push(h('div', { class: 'ref-row' }, link(m.name, m.address, hex(m.address))));
  }
  if (uses.senders.length === 0) {
    rows.push(h('div', { class: 'ref-sum' }, `No code here sends ${selector}`));
    rows.push(h('div', { class: 'muted', style: 'font-size:12px' }, 'It may be sent from another image, or with a selector made at run time.'));
    return rows;
  }
  const sites = uses.senders.reduce((n, s) => n + s.calls, 0);
  rows.push(h('div', { class: 'ref-sum', title: selector }, `Sent by ${formatCount(uses.senders.length)} ${uses.senders.length === 1 ? 'function' : 'functions'} (${formatCount(sites)} ${sites === 1 ? 'site' : 'sites'})`));
  for (const s of uses.senders.slice(0, 100)) {
    rows.push(h('div', { class: 'ref-row', title: `First at ${hex(s.site)}` }, h('span', { class: 'ref-kind k-call' }, `${s.calls}×`), link(s.name, s.site, `First at ${hex(s.site)}`)));
  }
  if (uses.senders.length > 100) rows.push(h('div', { class: 'muted' }, `… ${formatCount(uses.senders.length - 100)} more`));
  return rows;
}
