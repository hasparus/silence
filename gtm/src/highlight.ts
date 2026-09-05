import {highlight, HighlightedCode, RawCode} from 'codehike/code';

export const CODE_BUGGY = `/** Adds two numbers. */
export function add(a: number, b: number) {
  return a - b;
}`;

export const CODE_DIRTY = `/** Adds two numbers. */
export function add(a: number, b: number) {
  // Adds numbers, does not subtract. Returns.
  return a + b;
}`;

export const CODE_CLEAN = `/** Adds two numbers. */
export function add(a: number, b: number) {
  return a + b;
}`;

export async function hl(
  value: string,
  lang = 'ts',
): Promise<HighlightedCode> {
  const raw: RawCode = {value, lang, meta: ''};
  return highlight(raw, 'github-light');
}
