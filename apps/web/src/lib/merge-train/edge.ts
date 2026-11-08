// EXP-1709: a lifecycle edge of a thread (a turn ending, a session ending,
// `pr open`, a draft readied) passes the merge train over the thread's
// repositories. Loaded lazily: the worker pulls in the PR stack, which the
// session router and the MCP server must not import.
import type { EdgeScope } from "@/lib/merge-train/worker"

const pending = new Set<Promise<void>>()

/** Fire and forget, AFTER the edge's write committed. */
export function trainEdge(rootId: string | null | undefined): void {
  if (!rootId) return
  trainEdgeFor({ rootId })
}

export function trainEdgeFor(scope: EdgeScope): void {
  const edge: Promise<void> = import(`@/lib/merge-train/worker`)
    .then(async ({ mergeTrainEdge, drainMergeTrainEdges }) => {
      mergeTrainEdge(scope)
      await drainMergeTrainEdges()
    })
    .catch((err) => console.error(`[merge-train] edge failed`, err))
    .finally(() => pending.delete(edge))
  pending.add(edge)
}

/** Tests: resolves once every fired edge has finished. */
export async function drainTrainEdges(): Promise<void> {
  while (pending.size > 0) await Promise.all([...pending])
}
