/**
 * Stable orderings for lists rendered off Electric (EXP-668).
 *
 * Electric hands a collection's rows over in no guaranteed order, and both
 * `Array.prototype.sort` and the query builder's `orderBy` are STABLE — so rows
 * with an equal sort key keep whatever order they arrived in, which differs
 * between syncs. Every list keyed on `createdAt` alone therefore reorders itself
 * for no reason: the review queue, the board list, the launcher's issue picker
 * and the members list were each observed swapping rows between two runs over
 * identical data, which is how it was found (five screenshot views churning
 * every refresh).
 *
 * It is not only a capture problem. Two people looking at the same review queue
 * can see it in different orders, and one person can see it reorder under them
 * on a resync.
 *
 * The fix everywhere is the same: break the tie on `id`. It is the one column
 * every synced table has, it is unique, and it never changes — so equal
 * timestamps resolve to ONE order that every client agrees on. The direction of
 * the tie-break is deliberately ASCENDING in both comparators: it is an
 * arbitrary-but-fixed disambiguator, not a second meaningful sort, and keeping
 * it stable in both directions means a list and its reverse stay mirror images.
 */

/** The shape both comparators need: a synced row with a creation timestamp. */
export interface StablyOrdered {
  id: string
  createdAt: Date | string | number
}

const at = (row: StablyOrdered): number => new Date(row.createdAt).getTime()

/** Oldest first, ties broken by id. */
export function byCreatedAt(a: StablyOrdered, b: StablyOrdered): number {
  return at(a) - at(b) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
}

/** Newest first, ties broken by id (ascending — see the module note). */
export function byCreatedAtDesc(a: StablyOrdered, b: StablyOrdered): number {
  return at(b) - at(a) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
}
