// EXP-707: the ONE Postgres unique_violation (23505) probe — previously
// copy-pasted into statuses/actions/labels with a divergent constraint-name
// variant in boards. Errors surface either as pg's DatabaseError directly or
// wrapped in an error cause by drizzle, so both walk the cause chain.

// Returns the violated constraint's name (`` when pg omitted it) or null when
// the error is no unique violation — boards needs the name (it has two
// uniques: team_id+slug and team_id+prefix).
export function uniqueViolationConstraint(err: unknown): string | null {
  if (!err || typeof err !== `object`) return null
  const candidate = err as {
    code?: unknown
    constraint?: unknown
    cause?: unknown
  }
  if (candidate.code === `23505`) {
    return typeof candidate.constraint === `string` ? candidate.constraint : ``
  }
  return uniqueViolationConstraint(candidate.cause)
}

export function isUniqueViolation(err: unknown): boolean {
  return uniqueViolationConstraint(err) !== null
}
