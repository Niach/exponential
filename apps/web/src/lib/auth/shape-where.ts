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

/**
 * IN-clause over non-id text values (e.g. an enum column). Values MUST be
 * enum literals from the domain contract — never user input (same warning as
 * sqlStringLiteral). Sorted for stable shape identity.
 */
export function buildTextInClause(column: string, values: string[]): string {
  if (values.length === 0) {
    // Impossible-match fallback mirroring buildWhereClause.
    return `"${column}" = ${sqlStringLiteral(`__none__`)}`
  }
  const escaped = [...values].sort().map(sqlStringLiteral).join(`,`)
  return `"${column}" IN (${escaped})`
}

/**
 * The shared member where clause of the board-scoped child shapes (issues,
 * comments, attachments, issue_labels, issue_subscribers, issue_events,
 * coding_sessions): team-scoped via the denormalized `team_id`, trash-aware
 * via the trigger-maintained `board_deleted_at` mirror (REV2-5). Both parts
 * are stable across board create/trash/restore, so the shape identity only
 * changes on actual team-membership changes (whose wholesale resync is
 * legitimate). Board-less rows (batch coding_sessions) keep a NULL
 * board_deleted_at and therefore always match the trash arm. Pass an empty
 * teamIds list for anonymous callers — the impossible-match sentinel keeps
 * the clause byte-stable and matches nothing.
 */
export function buildTeamScopedChildWhere(teamIds: string[]): string {
  return andClauses(
    buildWhereClause(`team_id`, teamIds),
    `"board_deleted_at" IS NULL`
  )
}

/** AND-combines clauses, parenthesizing each. */
export function andClauses(...clauses: string[]): string {
  if (clauses.length === 1) return clauses[0]
  return clauses.map((clause) => `(${clause})`).join(` AND `)
}

/** OR-combines clauses, parenthesizing each. */
export function orClauses(...clauses: string[]): string {
  if (clauses.length === 1) return clauses[0]
  return clauses.map((clause) => `(${clause})`).join(` OR `)
}
