/* tslint:disable */
/* eslint-disable */

/**
 * `excerptAroundMatch`: lengths in UTF-16 units.
 */
export function excerptAroundMatch(
  content: string,
  terms: string[],
  max: number,
  lead: number,
): string;

/**
 * `extractSearchTerms`: "quoted phrases" whole, bare words split on
 * whitespace, single characters dropped unless quoted; case-insensitively
 * unique, at most eight.
 */
export function extractSearchTerms(text: string): string[];

/**
 * `highlightSegments`, as JSON (`[{"text": "...", "match": true}, ...]`):
 * one string across the boundary instead of an object per segment.
 */
export function highlightSegmentsJson(content: string, terms: string[]): string;

export type InitInput =
  | RequestInfo
  | URL
  | Response
  | BufferSource
  | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly excerptAroundMatch: (
    a: number,
    b: number,
    c: number,
    d: number,
    e: number,
    f: number,
  ) => [number, number];
  readonly extractSearchTerms: (a: number, b: number) => [number, number];
  readonly highlightSegmentsJson: (
    a: number,
    b: number,
    c: number,
    d: number,
  ) => [number, number];
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (
    a: number,
    b: number,
    c: number,
    d: number,
  ) => number;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_alloc: () => number;
  readonly __externref_drop_slice: (a: number, b: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(
  module: { module: SyncInitInput } | SyncInitInput,
): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init(
  module_or_path?:
    | { module_or_path: InitInput | Promise<InitInput> }
    | InitInput
    | Promise<InitInput>,
): Promise<InitOutput>;
