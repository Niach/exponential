// #EXP-2204 (docs/ci-components-contract.md §5): the JobGraph layout, the
// same in the Rust core (`eui-core` `job_graph.rs`), Swift
// (`JobGraphLayout.swift`) and Kotlin (`JobGraphLayout.kt`), all replaying
// fixtures/job-graph.json. No catalog import (src/tree-key.ts's rule): the
// geometry is style.json `jobGraph`, copied below and gated equal by
// src/job-graph.test.ts.

import { ciStatusOf, type CiStatus } from "./ci-log"

/** style.json `jobGraph` (px). `rowHeight` = a compact list row (`$control.row`). */
export interface JobGraphGeometry {
  nodeWidth: number
  nodeHeight: number
  columnGap: number
  rowGap: number
  stageHeight: number
  stageGap: number
  edgeWidth: number
  rowHeight: number
}

export const JOB_GRAPH: Readonly<JobGraphGeometry> = {
  nodeWidth: 200,
  nodeHeight: 40,
  columnGap: 48,
  rowGap: 12,
  stageHeight: 24,
  stageGap: 8,
  edgeWidth: 1.5,
  rowHeight: 40,
}

/** One authored job (the `jobGraphJob` shape). */
export interface JobGraphJob {
  id: string
  name?: string
  stage?: string
  status?: string
  duration?: string
  needs?: string[]
  matrix?: string
}

export interface JobGraphNode {
  /** The job id, or the matrix key of a collapsed matrix. */
  id: string
  name: string
  status: CiStatus
  duration: string | null
  /** A matrix: how many members passed of how many (`3/5`); else null. */
  count: { passed: number; total: number } | null
  /** The member job ids (one for a plain job). */
  members: string[]
  /** Known upstream node ids, first-seen order, never itself. */
  needs: string[]
  stage: string
  column: number
  /** Index within its column (graph); index in the list (list). */
  row: number
  x: number
  y: number
  /** 0 in list mode (a row fills the box). */
  width: number
  height: number
}

export interface JobGraphEdge {
  from: string
  to: string
  /** Upstream right centre → node left centre; both control points at the middle x. */
  start: [number, number]
  c1: [number, number]
  c2: [number, number]
  end: [number, number]
}

export interface JobGraphLayout {
  mode: `graph` | `list`
  width: number
  height: number
  stages: { title: string; x: number }[]
  nodes: JobGraphNode[]
  edges: JobGraphEdge[]
}

/** The aggregate status of a matrix: the first present, in this order. */
const MATRIX_ORDER: CiStatus[] = [`failed`, `running`, `queued`, `cancelled`, `passed`, `skipped`]

const str = (v: unknown): string | null => (typeof v === `string` && v !== `` ? v : null)

/** The jobs → stage columns (or needs depth), the node boxes and the need
 *  edges; `compact` = the list below `md` (contract §5). */
export function jobGraphLayout(jobs: readonly unknown[], compact: boolean, geometry: JobGraphGeometry = JOB_GRAPH): JobGraphLayout {
  // 1. Normalise and collapse matrices. Dropped: an entry without a string id, a
  // repeated id, a plain job named like an earlier matrix key and a matrix
  // member whose key names an earlier plain job.
  interface Draft { node: JobGraphNode; rawNeeds: string[]; statuses: CiStatus[] }
  const drafts: Draft[] = []
  const byKey = new Map<string, Draft>()
  const nodeOf = new Map<string, string>()
  const seen = new Set<string>()
  for (const raw of Array.isArray(jobs) ? jobs : []) {
    if (!raw || typeof raw !== `object`) continue
    const job = raw as Record<string, unknown>
    const id = str(job.id)
    if (!id || seen.has(id)) continue
    seen.add(id)
    const status = ciStatusOf(job.status)
    const needs = Array.isArray(job.needs) ? job.needs.filter((n): n is string => typeof n === `string`) : []
    const matrix = str(job.matrix)
    const stage = typeof job.stage === `string` ? job.stage : ``
    if (matrix) {
      const existing = byKey.get(matrix)
      if (!existing && nodeOf.has(matrix) && nodeOf.get(matrix) === matrix) continue // a plain job owns that id
      nodeOf.set(id, matrix)
      if (existing) {
        existing.node.members.push(id)
        existing.statuses.push(status)
        existing.rawNeeds.push(...needs)
        continue
      }
      const draft: Draft = { node: { id: matrix, name: matrix, status, duration: null, count: null, members: [id], needs: [], stage, column: 0, row: 0, x: 0, y: 0, width: 0, height: 0 }, rawNeeds: [...needs], statuses: [status] }
      byKey.set(matrix, draft)
      drafts.push(draft)
      continue
    }
    if (byKey.has(id)) continue // a matrix owns that id
    nodeOf.set(id, id)
    const name = str(job.name) ?? id
    const duration = typeof job.duration === `string` ? job.duration : null
    drafts.push({ node: { id, name, status, duration, count: null, members: [id], needs: [], stage, column: 0, row: 0, x: 0, y: 0, width: 0, height: 0 }, rawNeeds: needs, statuses: [status] })
  }
  const nodeIds = new Set(drafts.map((d) => d.node.id))
  for (const d of drafts) {
    if (byKey.get(d.node.id) === d) {
      d.node.status = MATRIX_ORDER.find((s) => d.statuses.includes(s)) ?? `queued`
      d.node.count = { passed: d.statuses.filter((s) => s === `passed`).length, total: d.statuses.length }
    }
    const needs: string[] = []
    for (const need of d.rawNeeds) {
      const target = nodeOf.get(need) ?? (byKey.has(need) ? need : undefined) // a job id, or a matrix key itself
      if (target === undefined || !nodeIds.has(target) || target === d.node.id || needs.includes(target)) continue
      needs.push(target)
    }
    d.node.needs = needs
  }
  const nodes = drafts.map((d) => d.node)
  const index = new Map(nodes.map((n, i) => [n.id, i]))

  // 2. Columns: the stages in first-appearance order, else the needs depth.
  const staged = nodes.some((n) => n.stage !== ``)
  const stageTitles: string[] = []
  if (staged) {
    for (const n of nodes) {
      if (!stageTitles.includes(n.stage)) stageTitles.push(n.stage)
      n.column = stageTitles.indexOf(n.stage)
    }
  } else {
    const depth = new Map<string, number>()
    const onPath = new Set<string>()
    const visit = (n: JobGraphNode): number => {
      const known = depth.get(n.id)
      if (known !== undefined) return known
      onPath.add(n.id)
      let d = 0
      for (const need of n.needs) {
        if (onPath.has(need)) continue
        d = Math.max(d, visit(nodes[index.get(need)!]!) + 1)
      }
      onPath.delete(n.id)
      depth.set(n.id, d)
      return d
    }
    for (const n of nodes) n.column = visit(n)
  }

  // Column order: column 0 first, then input order.
  const ordered = [...nodes].sort((a, b) => a.column - b.column || index.get(a.id)! - index.get(b.id)!)
  const g = geometry

  // 5. compact: one full-width row per node.
  if (compact) {
    ordered.forEach((n, i) => {
      n.row = i
      n.x = 0
      n.y = i * g.rowHeight
      n.width = 0
      n.height = g.rowHeight
    })
    return { mode: `list`, width: 0, height: ordered.length * g.rowHeight, stages: [], nodes: ordered, edges: [] }
  }

  // 3. Geometry.
  const top = staged ? g.stageHeight + g.stageGap : 0
  const pitchX = g.nodeWidth + g.columnGap
  const rows = new Map<number, number>()
  for (const n of ordered) {
    const r = rows.get(n.column) ?? 0
    rows.set(n.column, r + 1)
    n.row = r
    n.x = n.column * pitchX
    n.y = top + r * (g.nodeHeight + g.rowGap)
    n.width = g.nodeWidth
    n.height = g.nodeHeight
  }
  const columns = nodes.length ? Math.max(...nodes.map((n) => n.column)) + 1 : 0
  const maxRows = nodes.length ? Math.max(...rows.values()) : 0
  const width = columns ? columns * pitchX - g.columnGap : 0
  const height = maxRows ? top + maxRows * (g.nodeHeight + g.rowGap) - g.rowGap : 0
  const stages = staged ? stageTitles.map((title, i) => ({ title, x: i * pitchX })) : []
  const edges: JobGraphEdge[] = []
  for (const n of ordered) {
    for (const need of n.needs) {
      const up = nodes[index.get(need)!]!
      const x1 = up.x + up.width
      const y1 = up.y + up.height / 2
      const x2 = n.x
      const y2 = n.y + n.height / 2
      const mx = (x1 + x2) / 2
      edges.push({ from: up.id, to: n.id, start: [x1, y1], c1: [mx, y1], c2: [mx, y2], end: [x2, y2] })
    }
  }
  return { mode: `graph`, width, height, stages, nodes: ordered, edges }
}

/** The node `id` and every ancestor through `needs` (transitive), in the
 *  layout's node order; an unknown id → []. */
export function jobGraphUpstream(layout: JobGraphLayout, id: string): string[] {
  const byId = new Map(layout.nodes.map((n) => [n.id, n]))
  if (!byId.has(id)) return []
  const set = new Set<string>([id])
  const stack = [id]
  while (stack.length) {
    for (const need of byId.get(stack.pop()!)!.needs) {
      if (!set.has(need)) {
        set.add(need)
        stack.push(need)
      }
    }
  }
  return layout.nodes.filter((n) => set.has(n.id)).map((n) => n.id)
}
