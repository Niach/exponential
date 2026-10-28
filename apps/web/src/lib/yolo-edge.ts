// EXP-1413: a lifecycle edge of a thread (a turn ending, a session ending,
// `pr_open`) runs the yolo automerge. Loaded lazily: the merge pulls in the
// PR stack, which the session router and the MCP server must not import.

/** The edges still running (their pass and any re-run it coalesced). */
const pending = new Set<Promise<void>>()

/** Fire and forget, AFTER the edge's write committed, so the merge sees the
 *  tree idle. */
export function yoloEdge(rootId: string | null | undefined): void {
  if (!rootId) return
  const edge: Promise<void> = import(`@/lib/yolo-tree-merge`)
    .then(({ maybeMergeYoloThread }) => maybeMergeYoloThread(rootId))
    .then(() => {})
    .catch((err) => console.error(`[yolo] thread ${rootId} edge failed`, err))
    .finally(() => pending.delete(edge))
  pending.add(edge)
}

/** Tests: resolves once every fired edge has finished, so "nothing merged"
 *  is read after the edge ran, never after a guessed delay. */
export async function drainYoloEdges(): Promise<void> {
  while (pending.size > 0) await Promise.all([...pending])
}
