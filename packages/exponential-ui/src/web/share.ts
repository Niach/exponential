// VAPP-141 (React) + VAPP-152 (Solid, the element): structural sharing
// between two reduced trees, so a streamed `updateComponents` reaches only
// the nodes it changed.

const isPlainObject = (v: object): boolean => {
  const proto = Object.getPrototypeOf(v)
  return proto === Object.prototype || proto === null
}
const idOf = (v: unknown): string | undefined => (typeof v === `object` && v !== null && typeof (v as { id?: unknown }).id === `string` ? (v as { id: string }).id : undefined)

/** `next` with every part deep-equal to `prev`'s replaced by `prev`'s
 *  object (arrays of nodes matched by `id`, other arrays by index): a
 *  re-reduce (`updateComponents`) hands every unchanged node its old
 *  object, so the renderers skip it. Plain JSON only; anything else is
 *  taken as it is. */
export function share<T>(prev: unknown, next: T): T {
  if (Object.is(prev, next) || typeof prev !== `object` || typeof next !== `object` || prev === null || next === null) return next
  if (Array.isArray(next)) {
    if (!Array.isArray(prev)) return next
    const byId = prev.length > 0 && idOf(prev[0]) !== undefined ? new Map(prev.map((p) => [idOf(p), p])) : null
    let same = prev.length === next.length
    const out = next.map((v, i) => {
      const id = byId ? idOf(v) : undefined
      const s = share(id !== undefined ? byId!.get(id) : prev[i], v)
      if (s !== prev[i]) same = false
      return s
    })
    return (same ? prev : out) as T
  }
  if (Array.isArray(prev) || !isPlainObject(prev) || !isPlainObject(next)) return next
  const p = prev as Record<string, unknown>
  const keys = Object.keys(next)
  let same = keys.length === Object.keys(p).length
  const out: Record<string, unknown> = {}
  for (const k of keys) {
    const had = Object.prototype.hasOwnProperty.call(p, k)
    const s = share(had ? p[k] : undefined, (next as Record<string, unknown>)[k])
    out[k] = s
    if (!had || s !== p[k]) same = false
  }
  return (same ? prev : out) as T
}
