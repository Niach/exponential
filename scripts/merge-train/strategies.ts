// EXP-1708 (fixtures/merge-train.json `pipeline`): the pure file strategies
// of the train's generic resolver. Base = the common ancestor, ours = the
// train tip (earlier units), theirs = the later unit.

type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

const same = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b)
const isObject = (v: unknown): v is Record<string, Json> => typeof v === `object` && v !== null && !Array.isArray(v)

function unionArray(base: Json[], ours: Json[], theirs: Json[]): Json[] {
  const key = (v: Json) => JSON.stringify(v)
  const baseKeys = new Set(base.map(key))
  const theirKeys = new Set(theirs.map(key))
  const ourKeys = new Set(ours.map(key))
  // Either side's deletion of a base item holds; each side's additions stay,
  // ours first, then theirs in order.
  const kept = ours.filter((v) => !baseKeys.has(key(v)) || theirKeys.has(key(v)))
  return [...kept, ...theirs.filter((v) => !ourKeys.has(key(v)) && !baseKeys.has(key(v)))]
}

/** A three-way JSON union: a key only one side changed takes that side; a
 *  key both changed recurses into objects, unions arrays, else the later
 *  unit (theirs) wins. `undefined` = absent. */
export function mergeJson(base: Json | undefined, ours: Json | undefined, theirs: Json | undefined): Json | undefined {
  if (same(theirs, base)) return ours
  if (same(ours, base)) return theirs
  if (isObject(ours) && isObject(theirs)) {
    const b = isObject(base) ? base : {}
    const out: Record<string, Json> = {}
    for (const k of new Set([...Object.keys(ours), ...Object.keys(theirs)])) {
      const v = mergeJson(b[k], ours[k], theirs[k])
      if (v !== undefined) out[k] = v
    }
    return out
  }
  if (Array.isArray(ours) && Array.isArray(theirs)) return unionArray(Array.isArray(base) ? base : [], ours, theirs)
  return theirs
}

/** The text form: the train tip's indentation and final newline. */
export function jsonUnion(base: string | null, ours: string, theirs: string): string {
  const indent = /\n([ \t]+)\S/.exec(ours)?.[1] ?? `  `
  const merged = mergeJson(base === null ? undefined : JSON.parse(base), JSON.parse(ours), JSON.parse(theirs))
  return JSON.stringify(merged, null, indent) + (ours.endsWith(`\n`) ? `\n` : ``)
}

/** The leading number of a numbered file (`0042_brave_owl.sql` → 42). */
export function fileNumber(path: string): number | null {
  const m = /^(\d+)[_-]/.exec(path.split(`/`).pop() ?? ``)
  return m ? Number(m[1]) : null
}

/** Do the regenerated files carry exactly the dropped ones' contents (names
 *  aside)? Trailing whitespace is not a difference. */
export function sameContents(dropped: readonly string[], regenerated: readonly string[]): boolean {
  const norm = (list: readonly string[]) => list.map((s) => s.trimEnd()).sort()
  return same(norm(dropped), norm(regenerated))
}
