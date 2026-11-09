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

const linesOf = (text: string): string[] => text.match(/[^\n]*\n|[^\n]+$/g) ?? []

/** base → side as ONE pure insertion: the split points it fits ([lo, hi],
 *  a block can slide over repeated lines) and its length; null = not one. */
function insertion(base: string[], side: string[]): { lo: number; hi: number; k: number } | null {
  const k = side.length - base.length
  if (k <= 0) return null
  let p = 0
  while (p < base.length && base[p] === side[p]) p++
  let s = 0
  while (s < base.length && base[base.length - 1 - s] === side[side.length - 1 - s]) s++
  const lo = base.length - s
  return lo <= p ? { lo, hi: p, k } : null
}

const DECL = /\bconst\s+([A-Za-z_][A-Za-z0-9_]*)/g
const declared = (text: string): string[] => [...text.matchAll(DECL)].map((m) => m[1]!)
const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, `\\$&`)

/** prepend-union, entry-aware (#LOGS-2): base → ours (the train tip) and
 *  base → theirs (the later unit) each ONE pure insertion at one common
 *  split point (the earliest) = base prefix + theirs' block + ours' block +
 *  base rest, so both blocks stay whole entries. A `const <NAME>_<N>`
 *  theirs' block declares that ours already declares is renumbered to the
 *  next free N when the block names it only there. Anything else = null (a
 *  conflict), never a line union, never a const declared twice. */
export function prependUnion(base: string, ours: string, theirs: string): string | null {
  if (theirs === base || theirs === ours) return ours
  if (ours === base) return theirs
  const b = linesOf(base)
  const o = linesOf(ours)
  const t = linesOf(theirs)
  const io = insertion(b, o)
  const it = insertion(b, t)
  if (!io || !it) return null
  const at = Math.max(io.lo, it.lo)
  if (at > Math.min(io.hi, it.hi)) return null
  const ourBlock = o.slice(at, at + io.k).join(``)
  let theirBlock = t.slice(at, at + it.k).join(``)
  const taken = new Set(declared(ours))
  for (const name of new Set(declared(theirBlock))) {
    if (!taken.has(name)) continue
    const m = /^(.*_)(\d+)$/.exec(name)
    if (!m) return null
    const word = (n: string) => new RegExp(`\\b${escape(n)}\\b`, `g`)
    if ((theirBlock.match(word(name)) ?? []).length !== 1) return null
    const numbers = [...`${ours}\n${theirBlock}`.matchAll(new RegExp(`\\b${escape(m[1]!)}(\\d+)\\b`, `g`))].map((x) => Number(x[1]))
    const next = `${m[1]}${Math.max(...numbers) + 1}`
    theirBlock = theirBlock.replace(word(name), next)
    taken.add(next)
  }
  const text = b.slice(0, at).join(``) + theirBlock + ourBlock + b.slice(at).join(``)
  const names = declared(text)
  return new Set(names).size === names.length ? text : null
}
