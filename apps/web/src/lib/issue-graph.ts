// EXP-980: the `blocks` graph behind the list badge, the mini-graph overlay
// and the blocked-start dialog.
//
// ONE pure rule, mirrored ×4 (iOS `ExpCore/Sources/Domain/IssueGraph.swift`,
// Android `domain/IssueGraph.kt`, desktop `domain::issue_graph`) and locked by
// the contract fixture `domain-contract/fixtures/issue-graph.json` — same
// cases, same test names.
//
// A canonical `blocks` row is `issue_id` BLOCKS `related_issue_id` (EXP-736).
// An edge counts only while BOTH ends are synced and OPEN (anchor status not
// done / cancelled / duplicate): a finished blocker is in nobody's way, and a
// finished issue is blocked by nothing. A SUBJECT is always kept, open or not.

const CLOSED_ANCHORS = new Set<string>([`done`, `cancelled`, `duplicate`])

/** The most nodes one graph draws; the rest is cut and `truncated` says so. */
export const ISSUE_GRAPH_MAX_NODES = 60

export interface GraphRelation {
  type: string
  issueId: string
  relatedIssueId: string
}

export interface GraphIssue {
  id: string
  identifier: string
  /** The dual-written ANCHOR enum (`issues.status`). */
  status: string
}

export interface BlockCounts {
  /** Open issues that block this one. */
  blockedBy: number
  /** Open issues this one blocks. */
  blocking: number
}

export interface IssueGraphNode {
  id: string
  /** The column: 0 = blocked by nothing in the graph; every edge points to a
   *  higher wave (cycle edges aside). */
  wave: number
  /** The row inside the wave, by identifier. */
  lane: number
  subject: boolean
}

export interface IssueGraphEdge {
  /** The blocker. */
  from: string
  /** The blocked issue. */
  to: string
  /** Part of a blocking cycle: drawn red, and nothing on it can start. */
  cycle: boolean
}

export interface IssueGraph {
  nodes: IssueGraphNode[]
  edges: IssueGraphEdge[]
  hasCycle: boolean
  truncated: boolean
}

/** Under a graph the node cap cut. Byte-identical ×4. */
export const ISSUE_GRAPH_TRUNCATED_NOTE = `Showing the nearest ${ISSUE_GRAPH_MAX_NODES} issues.`
/** Under a graph that holds a cycle. Byte-identical ×4. */
export const ISSUE_GRAPH_CYCLE_NOTE = `Red issues block each other in a cycle.`

/** The badge's accessible label: `Blocked by 2`, `Blocking 1` or
 *  `Blocked by 2, blocking 1`. Byte-identical ×4. */
export function blocksBadgeLabel(counts: BlockCounts): string {
  const parts: string[] = []
  if (counts.blockedBy > 0) parts.push(`Blocked by ${counts.blockedBy}`)
  if (counts.blocking > 0) {
    parts.push(`${parts.length > 0 ? `blocking` : `Blocking`} ${counts.blocking}`)
  }
  return parts.join(`, `)
}

const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

function openEdges(
  relations: readonly GraphRelation[],
  issues: readonly GraphIssue[],
  keep: ReadonlySet<string>
): Array<[string, string]> {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const usable = (id: string) => {
    const issue = byId.get(id)
    return issue !== undefined && (keep.has(id) || !CLOSED_ANCHORS.has(issue.status))
  }
  const seen = new Set<string>()
  const edges: Array<[string, string]> = []
  for (const relation of relations) {
    if (relation.type !== `blocks`) continue
    if (relation.issueId === relation.relatedIssueId) continue
    if (!usable(relation.issueId) || !usable(relation.relatedIssueId)) continue
    const key = `${relation.issueId}\n${relation.relatedIssueId}`
    if (seen.has(key)) continue
    seen.add(key)
    edges.push([relation.issueId, relation.relatedIssueId])
  }
  return edges
}

/** EXP-998: every open `blocks` edge, blocker → blocked, deduplicated — what
 *  the list's rail draws (`lib/issue-rail.ts`). */
export function openBlockEdges(
  relations: readonly GraphRelation[],
  issues: readonly GraphIssue[]
): Array<{ from: string; to: string }> {
  return openEdges(relations, issues, new Set()).map(([from, to]) => ({ from, to }))
}

/** The badge numbers of every issue that has any; an issue with neither count
 *  is absent. */
export function blockCounts(
  relations: readonly GraphRelation[],
  issues: readonly GraphIssue[]
): Map<string, BlockCounts> {
  const counts = new Map<string, BlockCounts>()
  const at = (id: string) => {
    let entry = counts.get(id)
    if (!entry) {
      entry = { blockedBy: 0, blocking: 0 }
      counts.set(id, entry)
    }
    return entry
  }
  for (const [from, to] of openEdges(relations, issues, new Set())) {
    at(from).blocking += 1
    at(to).blockedBy += 1
  }
  return counts
}

/**
 * The open issues that block any of `ids` from OUTSIDE the set, by identifier:
 * what a batch start has to ask about (a blocker picked into the same batch is
 * not in its way).
 */
export function openBlockersOfSet<T extends GraphIssue>(
  ids: readonly string[],
  relations: readonly GraphRelation[],
  issues: readonly T[]
): T[] {
  const picked = new Set(ids)
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const out = new Map<string, T>()
  for (const [from, to] of openEdges(relations, issues, new Set())) {
    if (!picked.has(to) || picked.has(from)) continue
    const blocker = byId.get(from)
    if (blocker) out.set(from, blocker)
  }
  return [...out.values()].sort((a, b) => byText(a.identifier, b.identifier))
}

/**
 * The graph around `subjectIds`: the subjects, everything that transitively
 * blocks them and everything they transitively block, with every open edge
 * among those nodes.
 *
 * Layout: an edge is a CYCLE edge when its blocker is reachable from its
 * blocked end; `wave` is the longest path over the remaining (acyclic) edges,
 * `lane` the identifier order inside a wave. Nodes come back by (wave, lane),
 * edges by (from, to) identifier.
 */
export function blockGraph(
  subjectIds: readonly string[],
  relations: readonly GraphRelation[],
  issues: readonly GraphIssue[]
): IssueGraph {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const identifierOf = (id: string) => byId.get(id)?.identifier ?? id
  const byIdentifier = (a: string, b: string) =>
    byText(identifierOf(a), identifierOf(b)) || byText(a, b)

  const subjects = [...new Set(subjectIds)].filter((id) => byId.has(id)).sort(byIdentifier)
  const subjectSet = new Set(subjects)
  const all = openEdges(relations, issues, subjectSet)
  const blockersOf = new Map<string, string[]>()
  const blockedBy = new Map<string, string[]>()
  for (const [from, to] of all) {
    blockersOf.set(to, [...(blockersOf.get(to) ?? []), from])
    blockedBy.set(from, [...(blockedBy.get(from) ?? []), to])
  }

  // The closure, blockers first and then the blocked side, level by level and
  // in identifier order, so the node cap cuts the same nodes everywhere.
  const picked = new Set<string>()
  let truncated = false
  const admit = (id: string) => {
    if (picked.has(id)) return false
    if (picked.size >= ISSUE_GRAPH_MAX_NODES) {
      truncated = true
      return false
    }
    picked.add(id)
    return true
  }
  for (const id of subjects) admit(id)
  for (const next of [blockersOf, blockedBy]) {
    let frontier = subjects
    while (frontier.length > 0) {
      const found = new Set<string>()
      for (const id of frontier) {
        for (const other of next.get(id) ?? []) {
          if (!picked.has(other)) found.add(other)
        }
      }
      frontier = [...found].sort(byIdentifier).filter(admit)
    }
  }

  const inside = all.filter(([from, to]) => picked.has(from) && picked.has(to))
  const out = new Map<string, string[]>()
  for (const [from, to] of inside) out.set(from, [...(out.get(from) ?? []), to])
  const reaches = (start: string, goal: string) => {
    const seen = new Set<string>([start])
    const stack = [start]
    while (stack.length > 0) {
      const id = stack.pop() as string
      if (id === goal) return true
      for (const other of out.get(id) ?? []) {
        if (!seen.has(other)) {
          seen.add(other)
          stack.push(other)
        }
      }
    }
    return false
  }

  const edges: IssueGraphEdge[] = inside
    .map(([from, to]) => ({ from, to, cycle: reaches(to, from) }))
    .sort((a, b) => byIdentifier(a.from, b.from) || byIdentifier(a.to, b.to))

  // Longest path over the acyclic edges (Kahn).
  const wave = new Map<string, number>([...picked].map((id) => [id, 0]))
  const pending = new Map<string, number>([...picked].map((id) => [id, 0]))
  const forward = new Map<string, string[]>()
  for (const edge of edges) {
    if (edge.cycle) continue
    pending.set(edge.to, (pending.get(edge.to) ?? 0) + 1)
    forward.set(edge.from, [...(forward.get(edge.from) ?? []), edge.to])
  }
  const ready = [...picked].filter((id) => pending.get(id) === 0)
  while (ready.length > 0) {
    const id = ready.pop() as string
    for (const other of forward.get(id) ?? []) {
      wave.set(other, Math.max(wave.get(other) ?? 0, (wave.get(id) ?? 0) + 1))
      const left = (pending.get(other) ?? 0) - 1
      pending.set(other, left)
      if (left === 0) ready.push(other)
    }
  }

  const ordered = [...picked].sort(
    (a, b) => (wave.get(a) ?? 0) - (wave.get(b) ?? 0) || byIdentifier(a, b)
  )
  const laneAt = new Map<number, number>()
  const nodes: IssueGraphNode[] = ordered.map((id) => {
    const w = wave.get(id) ?? 0
    const lane = laneAt.get(w) ?? 0
    laneAt.set(w, lane + 1)
    return { id, wave: w, lane, subject: subjectSet.has(id) }
  })

  return { nodes, edges, hasCycle: edges.some((edge) => edge.cycle), truncated }
}

// EXP-1057: THE mini-graph's geometry, identical ×4 and locked by
// `domain-contract/fixtures/issue-graph-geometry.json` (desktop
// `domain::issue_graph::geometry`, iOS `IssueGraph.Geometry`, Android
// `IssueGraph.Geometry`). The web draws it through `@exp/ui` `WaveGraph`,
// whose edge rule is the one below; the grid sits `inset` inside its scroll
// box so the rings are never clipped.
export const ISSUE_GRAPH_GEOMETRY = {
  nodeWidth: 176,
  nodeHeight: 28,
  waveGap: 40,
  laneGap: 8,
  inset: 4,
  maxViewWidth: 520,
  maxViewHeight: 320,
  edgeStroke: 1.25,
  ringWidth: 1,
  nodeRadius: 6,
  railGutter: 8,
  railNodeWidth: 24,
  railDot: 10,
  railDotRing: 2,
} as const

export interface GraphPoint {
  x: number
  y: number
}

/** A node box's top-left inside the grid. */
export function issueGraphOrigin(wave: number, lane: number): GraphPoint {
  const g = ISSUE_GRAPH_GEOMETRY
  return {
    x: g.inset + wave * (g.nodeWidth + g.waveGap),
    y: g.inset + lane * (g.nodeHeight + g.laneGap),
  }
}

/** The grid's natural size for `waves` × `lanes` (insets included) and the
 *  viewport it shows before scrolling. */
export function issueGraphSize(
  waves: number,
  lanes: number
): { width: number; height: number; viewWidth: number; viewHeight: number } {
  const g = ISSUE_GRAPH_GEOMETRY
  if (waves <= 0 || lanes <= 0) {
    return { width: 0, height: 0, viewWidth: 0, viewHeight: 0 }
  }
  const width = 2 * g.inset + waves * (g.nodeWidth + g.waveGap) - g.waveGap
  const height = 2 * g.inset + lanes * (g.nodeHeight + g.laneGap) - g.laneGap
  return {
    width,
    height,
    viewWidth: Math.min(width, g.maxViewWidth),
    viewHeight: Math.min(height, g.maxViewHeight),
  }
}

/** One edge as a cubic: blocker's right-middle → blocked box's left-middle. */
export function issueGraphEdge(
  from: { wave: number; lane: number },
  to: { wave: number; lane: number }
): { start: GraphPoint; control1: GraphPoint; control2: GraphPoint; end: GraphPoint } {
  const g = ISSUE_GRAPH_GEOMETRY
  const a = issueGraphOrigin(from.wave, from.lane)
  const b = issueGraphOrigin(to.wave, to.lane)
  const start = { x: a.x + g.nodeWidth, y: a.y + g.nodeHeight / 2 }
  const end = { x: b.x, y: b.y + g.nodeHeight / 2 }
  const bend =
    end.x > start.x
      ? (end.x - start.x) / 2
      : Math.max(g.waveGap / 2, Math.abs(end.x - start.x) / 2)
  return {
    start,
    control1: { x: start.x + bend, y: start.y },
    control2: { x: end.x - bend, y: end.y },
    end,
  }
}
