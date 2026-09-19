// EXP-981: the workflow graph, computed ONCE on the server. Clients draw a
// grid (`wave` = column, `lane` = row) and edges; none of them lays anything
// out, so the four renderings can never disagree.
//
// 1. Cycles: Tarjan's strongly connected components. A component of two or
//    more nodes (or a self edge) is a cycle: its nodes and the edges inside
//    it are flagged, and a workflow that has one cannot start.
// 2. Waves: longest-path layering over the edges that are NOT inside a cycle
//    (that remainder is acyclic by construction): wave 0 = blocked by
//    nothing, every edge points to a later wave. The wave IS the earliest
//    moment Kahn's scheduler could start the node.
// 3. Lanes: barycenter ordering, a fixed number of down/up sweeps from the
//    identifier order, so crossings drop and the result is deterministic.
//
// Pure: ids in, numbers out. `key` is what ties are broken by (the issue
// identifier), never the uuid, so the picture is stable for a reader.

export interface LayoutNode {
  id: string
  key: string
}

/** `from` must land before `to` (a `blocks` relation between the nodes). */
export type LayoutEdge = readonly [from: string, to: string]

export interface NodePlacement {
  wave: number
  lane: number
  /** On a blocking cycle. */
  cycle: boolean
}

export interface WorkflowMetrics {
  nodes: number
  edges: number
  /** Waves on the critical path (0 for an empty workflow). */
  depth: number
  /** The widest wave: the most nodes that could ever run at once. */
  width: number
  /** One entry per cycle: its nodes' keys, sorted. Empty = startable. */
  cycles: string[][]
}

export interface WorkflowLayout {
  placements: Map<string, NodePlacement>
  /** `from\nto` of every edge inside a cycle. */
  cycleEdges: Set<string>
  metrics: WorkflowMetrics
}

const BARYCENTER_SWEEPS = 4

const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

export const edgeKey = (from: string, to: string) => `${from}\n${to}`

/** Tarjan, iterative (a long chain must not blow the stack). Returns the
 *  component index of every node. */
function components(ids: readonly string[], out: Map<string, string[]>) {
  const index = new Map<string, number>()
  const low = new Map<string, number>()
  const onStack = new Set<string>()
  const stack: string[] = []
  const component = new Map<string, number>()
  let counter = 0
  let count = 0

  for (const root of ids) {
    if (index.has(root)) continue
    const work: Array<{ id: string; next: number }> = [{ id: root, next: 0 }]
    index.set(root, counter)
    low.set(root, counter)
    counter += 1
    stack.push(root)
    onStack.add(root)
    while (work.length > 0) {
      const frame = work[work.length - 1]!
      const targets = out.get(frame.id) ?? []
      if (frame.next < targets.length) {
        const target = targets[frame.next]!
        frame.next += 1
        if (!index.has(target)) {
          index.set(target, counter)
          low.set(target, counter)
          counter += 1
          stack.push(target)
          onStack.add(target)
          work.push({ id: target, next: 0 })
        } else if (onStack.has(target)) {
          low.set(frame.id, Math.min(low.get(frame.id)!, index.get(target)!))
        }
        continue
      }
      work.pop()
      const parent = work[work.length - 1]
      if (parent) {
        low.set(parent.id, Math.min(low.get(parent.id)!, low.get(frame.id)!))
      }
      if (low.get(frame.id) === index.get(frame.id)) {
        let member: string
        do {
          member = stack.pop()!
          onStack.delete(member)
          component.set(member, count)
        } while (member !== frame.id)
        count += 1
      }
    }
  }
  return component
}

export function layoutWorkflow(
  nodes: readonly LayoutNode[],
  edges: readonly LayoutEdge[]
): WorkflowLayout {
  const keyOf = new Map(nodes.map((node) => [node.id, node.key]))
  const byKey = (a: string, b: string) =>
    byText(keyOf.get(a) ?? a, keyOf.get(b) ?? b) || byText(a, b)
  const ids = nodes.map((node) => node.id).sort(byKey)

  // Known ends only, one edge per pair.
  const seen = new Set<string>()
  const clean: Array<[string, string]> = []
  for (const [from, to] of edges) {
    if (!keyOf.has(from) || !keyOf.has(to)) continue
    const key = edgeKey(from, to)
    if (seen.has(key)) continue
    seen.add(key)
    clean.push([from, to])
  }
  clean.sort((a, b) => byKey(a[0], b[0]) || byKey(a[1], b[1]))

  const out = new Map<string, string[]>()
  for (const [from, to] of clean) out.set(from, [...(out.get(from) ?? []), to])

  const component = components(ids, out)
  const size = new Map<number, number>()
  for (const c of component.values()) size.set(c, (size.get(c) ?? 0) + 1)
  const selfLoops = new Set(clean.filter(([f, t]) => f === t).map(([f]) => f))
  const onCycle = (id: string) =>
    (size.get(component.get(id)!) ?? 0) > 1 || selfLoops.has(id)

  const cycleEdges = new Set<string>()
  const acyclic: Array<[string, string]> = []
  for (const [from, to] of clean) {
    if (component.get(from) === component.get(to)) cycleEdges.add(edgeKey(from, to))
    else acyclic.push([from, to])
  }

  // Longest path (Kahn).
  const wave = new Map(ids.map((id) => [id, 0]))
  const pending = new Map(ids.map((id) => [id, 0]))
  const forward = new Map<string, string[]>()
  const backward = new Map<string, string[]>()
  for (const [from, to] of acyclic) {
    pending.set(to, (pending.get(to) ?? 0) + 1)
    forward.set(from, [...(forward.get(from) ?? []), to])
    backward.set(to, [...(backward.get(to) ?? []), from])
  }
  const ready = ids.filter((id) => pending.get(id) === 0)
  while (ready.length > 0) {
    const id = ready.pop()!
    for (const target of forward.get(id) ?? []) {
      wave.set(target, Math.max(wave.get(target)!, wave.get(id)! + 1))
      const left = pending.get(target)! - 1
      pending.set(target, left)
      if (left === 0) ready.push(target)
    }
  }

  // Layers, in identifier order to begin with.
  const layers: string[][] = []
  for (const id of ids) {
    const w = wave.get(id)!
    ;(layers[w] ??= []).push(id)
  }
  const lane = new Map<string, number>()
  const stamp = () => {
    for (const layer of layers) layer?.forEach((id, i) => lane.set(id, i))
  }
  stamp()
  const sweep = (order: number[], neighbours: Map<string, string[]>) => {
    for (const w of order) {
      const layer = layers[w]
      if (!layer) continue
      const score = new Map<string, number>()
      for (const id of layer) {
        const near = neighbours.get(id) ?? []
        score.set(
          id,
          near.length === 0
            ? lane.get(id)!
            : near.reduce((sum, other) => sum + lane.get(other)!, 0) / near.length
        )
      }
      layer.sort((a, b) => score.get(a)! - score.get(b)! || byKey(a, b))
      layer.forEach((id, i) => lane.set(id, i))
    }
  }
  const down = layers.map((_, i) => i).slice(1)
  const up = layers.map((_, i) => i).slice(0, -1).reverse()
  for (let i = 0; i < BARYCENTER_SWEEPS; i += 1) {
    sweep(down, backward)
    sweep(up, forward)
  }

  const placements = new Map<string, NodePlacement>()
  for (const id of ids) {
    placements.set(id, { wave: wave.get(id)!, lane: lane.get(id)!, cycle: onCycle(id) })
  }

  const cycleGroups = new Map<number, string[]>()
  for (const id of ids) {
    if (!onCycle(id)) continue
    const c = component.get(id)!
    cycleGroups.set(c, [...(cycleGroups.get(c) ?? []), keyOf.get(id) ?? id])
  }
  const cycles = [...cycleGroups.values()]
    .map((keys) => keys.sort(byText))
    .sort((a, b) => byText(a[0]!, b[0]!))

  return {
    placements,
    cycleEdges,
    metrics: {
      nodes: ids.length,
      edges: clean.length,
      depth: layers.length,
      width: layers.reduce((max, layer) => Math.max(max, layer?.length ?? 0), 0),
      cycles,
    },
  }
}
