import {highlight, HighlightedCode, RawCode} from 'codehike/code';

export const CODE_CLEAN = `/** Adds two numbers. */
export function add(a: number, b: number) {
  return a + b;
}`;

export const CODE_DIRTY = `/** Adds two numbers. */
export function add(a: number, b: number) {
  // This adds a and b together and returns
  // the result, exactly as you requested
  return a + b;
}`;

export const TERMINAL = `$ cargo install silence-cli
$ silence hook install
  ✓ hook installed — comments will be kept honest`;

export async function hl(
  value: string,
  lang = 'ts',
): Promise<HighlightedCode> {
  const raw: RawCode = {value, lang, meta: ''};
  return highlight(raw, 'github-light');
}
