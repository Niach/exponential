import { createLiveQueryCollection, type Collection } from "@tanstack/react-db"

// #PERF-7: ONE live query per identical on-demand read. `useLiveQuery`
// builds a fresh live query collection per hook (garbage-collected 1 ms
// after its last subscriber), so two components asking the same subset of
// an on-demand collection (the chat's composer and its quick replies both
// read the root's newest messages) each sent their own `subset__*` request,
// and reopening a chat a second later fetched them all again. A shared live
// query is keyed by what it reads: every hook with the same key subscribes
// to the same collection, which stays loaded `SHARED_GC_MS` after its last
// subscriber leaves (back and forth between chats reloads nothing).

/** How long an unused shared live query keeps its subset loaded. */
export const SHARED_GC_MS = 60_000

type LiveQueryConfig = Parameters<typeof createLiveQueryCollection>[0]
type QueryFn = Extract<LiveQueryConfig, { query: unknown }>[`query`]
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type SharedCollection = Collection<any, string | number>

const shared = new Map<string, SharedCollection>()

/** The live query collection for `key` (built from `query` on first use, or
 *  again once the previous one was cleaned up). */
export function sharedLiveQuery(key: string, query: QueryFn): SharedCollection {
  const hit = shared.get(key)
  if (hit && hit.status !== `cleaned-up`) return hit
  for (const [staleKey, collection] of shared) {
    if (collection.status === `cleaned-up`) shared.delete(staleKey)
  }
  const collection = createLiveQueryCollection({ query, gcTime: SHARED_GC_MS }) as unknown as SharedCollection
  shared.set(key, collection)
  return collection
}
