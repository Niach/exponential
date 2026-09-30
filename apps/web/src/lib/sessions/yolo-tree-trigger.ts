// EXP-1146: the fire-and-forget edge hook every session lifecycle path calls
// (the busy→idle edge, the client end, the kill, the stale sweep; the MCP
// `sessions_end` and `pr_open` tools call `maybeMergeYoloTree` directly).
// A separate, import-free module on purpose: the session and steer routers
// are part of `appRouter`, which the tree merge itself calls into, so the
// implementation is loaded lazily and the routers only ever import this stub.
// Never throws — a merge must never fail the edge that fired it.
export function fireYoloTreeMerge(sessionId: string): void {
  void import(`@/lib/yolo-tree-merge`)
    .then((mod) => mod.maybeMergeYoloTree(sessionId))
    .catch((err) => {
      console.error(`[yolo-tree] trigger for ${sessionId} failed:`, err)
    })
}
