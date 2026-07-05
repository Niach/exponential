/**
 * Quotes a value as a SQL string literal for an Electric shape `where` clause.
 *
 * WARNING: only pass database-derived values (ids read back from our own
 * tables) — NEVER raw user input. Electric's where param has no server-side
 * parameter binding, so this naive escaping is the only barrier; it handles
 * quote doubling but is not a general-purpose SQL sanitizer.
 */
export function sqlStringLiteral(value: string): string {
  return `'${value.replace(/'/g, `''`)}'`
}

export function buildWhereClause(column: string, ids: string[]): string {
  if (ids.length === 0) {
    return `"${column}" = ${sqlStringLiteral(`00000000-0000-0000-0000-000000000000`)}`
  }

  // Sort so the same id SET always yields byte-identical SQL. Membership
  // queries return rows in heap order, which can flip between requests — and
  // the where clause is part of Electric's shape identity, so an order flip
  // rotates the shape handle and 409-loops every syncing client.
  const escapedIds = [...ids].sort().map(sqlStringLiteral).join(`,`)
  return `"${column}" IN (${escapedIds})`
}
