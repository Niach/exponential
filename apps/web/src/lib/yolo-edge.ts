// EXP-1413: a lifecycle edge of a thread (a turn ending, a session ending,
// `pr_open`) runs the yolo automerge. Loaded lazily: the merge pulls in the
// PR stack, which the session router and the MCP server must not import.

/** Fire and forget, AFTER the edge's write committed, so the merge sees the
 *  tree idle. */
export function yoloEdge(rootId: string | null | undefined): void {
  if (!rootId) return
  void import(`@/lib/yolo-tree-merge`)
    .then(({ maybeMergeYoloThread }) => maybeMergeYoloThread(rootId))
    .catch((err) => console.error(`[yolo] thread ${rootId} edge failed`, err))
}
