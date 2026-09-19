// EXP-980: sub-issues nest under their parent in every issue list. The ROOT
// issue decides the group and the sort position; its sub-issues follow it,
// whatever status they are in themselves.
//
// ONE pure rule over ids, mirrored ×4 (iOS `ExpCore/Sources/Domain/
// IssueNesting.swift`, Android `domain/IssueNesting.kt`, desktop
// `domain::issue_nesting`) and locked by the contract fixture
// `domain-contract/fixtures/issue-nesting.json` — same cases, same test names.
// The elbow connectors are the shared `treeGuides(depths)`.
//
// 1. A canonical `parent` row is `issue_id` = the PARENT, `related_issue_id` =
//    the CHILD (EXP-736). Only rows whose BOTH ends are in the list count: a
//    parent on another board, unsynced or filtered out leaves the child a root.
// 2. A child with several listed parents nests under the one with the lowest
//    identifier (plain string order).
// 3. An issue on a parent CYCLE keeps no parent (walking up from it returns to
//    it), so a cycle can never swallow rows; whatever hangs off it still nests.
// 4. Siblings keep the LIST's order: group order first, then the position
//    inside their own group — the caller's comparator already ran.

export interface NestingRelation {
  type: string
  issueId: string
  relatedIssueId: string
}

export interface NestedIssueRow {
  id: string
  /** 0 = a root row, 1 = its sub-issue, … */
  depth: number
}

/**
 * `groups` = the list's groups in order, each the ids in display order.
 * Returns the same number of groups; one a nesting emptied comes back empty
 * (the caller hides it).
 */
export function nestIssueRows(
  groups: readonly (readonly string[])[],
  relations: readonly NestingRelation[],
  identifierOf: (id: string) => string
): NestedIssueRow[][] {
  const position = new Map<string, number>()
  for (const ids of groups) {
    for (const id of ids) {
      if (!position.has(id)) position.set(id, position.size)
    }
  }

  const parentOf = new Map<string, string>()
  for (const relation of relations) {
    if (relation.type !== `parent`) continue
    const parent = relation.issueId
    const child = relation.relatedIssueId
    if (parent === child) continue
    if (!position.has(parent) || !position.has(child)) continue
    const current = parentOf.get(child)
    if (current === undefined || identifierOf(parent) < identifierOf(current)) {
      parentOf.set(child, parent)
    }
  }

  // Rule 3: drop the parent of every issue that is its own ancestor.
  // Judged against the untouched map first, so EVERY member of a cycle
  // becomes a root, whatever order the rows arrived in.
  const onCycle: string[] = []
  for (const start of parentOf.keys()) {
    const seen = new Set<string>([start])
    let cursor = parentOf.get(start)
    while (cursor !== undefined && !seen.has(cursor)) {
      seen.add(cursor)
      cursor = parentOf.get(cursor)
    }
    if (cursor === start) onCycle.push(start)
  }
  for (const id of onCycle) parentOf.delete(id)

  const childrenOf = new Map<string, string[]>()
  for (const [child, parent] of parentOf) {
    const siblings = childrenOf.get(parent)
    if (siblings) siblings.push(child)
    else childrenOf.set(parent, [child])
  }
  for (const siblings of childrenOf.values()) {
    siblings.sort((a, b) => (position.get(a) ?? 0) - (position.get(b) ?? 0))
  }

  const emitted = new Set<string>()
  const emit = (id: string, depth: number, out: NestedIssueRow[]) => {
    if (emitted.has(id)) return
    emitted.add(id)
    out.push({ id, depth })
    for (const child of childrenOf.get(id) ?? []) emit(child, depth + 1, out)
  }

  return groups.map((ids) => {
    const out: NestedIssueRow[] = []
    for (const id of ids) {
      if (!parentOf.has(id)) emit(id, 0, out)
    }
    return out
  })
}
