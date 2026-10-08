// Round 1 (docs/round-1-contract.md §4): the built-in UI strings. The ids
// and English defaults live in catalog/strings.json; a host overrides any id
// per surface; `$string.<id>` in a prop value resolves through this table at
// bind time. Pure, mirrored by the Rust core and the native renderers.

import stringsJson from "../catalog/strings.json" with { type: "json" }

export const DEFAULT_STRINGS: Readonly<Record<string, string>> = stringsJson.strings
export const STRING_IDS: readonly string[] = Object.keys(DEFAULT_STRINGS)

const REF = /^\$string\.([a-zA-Z][a-zA-Z0-9]*)$/
const PLACEHOLDER = /\{([a-zA-Z][a-zA-Z0-9]*)\}/g

/** `$string.choose` → `choose`; null for any other value. */
export function parseStringRef(value: unknown): string | null {
  if (typeof value !== `string`) return null
  const m = REF.exec(value)
  return m ? m[1] : null
}

/** True for a well-formed reference to a KNOWN id. */
export function isStringRef(value: unknown): boolean {
  const id = parseStringRef(value)
  return id !== null && id in DEFAULT_STRINGS
}

/** The host's overrides merged over the defaults. */
export function stringTable(overrides: Readonly<Record<string, string>> = {}): Record<string, string> {
  return { ...DEFAULT_STRINGS, ...overrides }
}

/** `{name}` placeholders filled from `params`; unknown ones stay. */
export function formatString(template: string, params: Record<string, unknown> = {}): string {
  return template.replace(PLACEHOLDER, (whole, name: string) => (name in params && params[name] !== undefined && params[name] !== null ? String(params[name]) : whole))
}

/** A prop value with any `$string.<id>` reference replaced by the table's
 *  text (an unknown id resolves to the id itself); other values pass. */
export function resolveString(value: unknown, table: Readonly<Record<string, string>> = DEFAULT_STRINGS): unknown {
  const id = parseStringRef(value)
  if (id === null) return value
  return table[id] ?? id
}
