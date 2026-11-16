// #FEED-260 (steer-enet.json SYNCED TAIL + OFFLINE LINE): what a NOT-live
// transcript draws after its feed, off the session's synced messages.
import {
  steerSyncedTail,
  type SteerSyncedMessage,
  type SteerTailFeedRow,
} from "@exp/domain-contract/steer-enet"
import { isSubagentScoped } from "@/lib/agent-feed"
import type { FeedItem } from "@/lib/steer-session-store"

const isoAt = (ms: number | undefined): string | null =>
  typeof ms === `number` && Number.isFinite(ms) ? new Date(ms).toISOString() : null

/** The feed's MAIN-lane rows as `steerSyncedTail` reads them: a narration
 *  (its text, its first fragment's `at`), an input, anything else with its
 *  known time (a settled tool's). */
export function syncedTailFeedRows(feed: readonly FeedItem[]): SteerTailFeedRow[] {
  const rows: SteerTailFeedRow[] = []
  for (const item of feed) {
    if (isSubagentScoped(item)) continue
    switch (item.kind) {
      case `narration`:
        rows.push({ author: `agent`, text: item.text, at: isoAt(item.at) })
        break
      case `user_message`:
        rows.push({ author: `user`, text: item.text, at: isoAt(item.at) })
        break
      case `tool`:
        rows.push({ author: null, text: null, at: isoAt(item.settled ? item.settledAt : undefined) })
        break
      default:
        rows.push({ author: null, text: null, at: null })
    }
  }
  return rows
}

/** The synced messages (bodies OPENED, `toTreeMessage`) a not-live
 *  transcript draws after its feed, oldest first; none while live. */
export function syncedTailMessages<T extends SteerSyncedMessage>(input: {
  live: boolean
  feed: readonly FeedItem[]
  messages: readonly T[]
  max?: number
}): T[] {
  if (input.live) return []
  return steerSyncedTail({
    live: input.live,
    feed: syncedTailFeedRows(input.feed),
    messages: input.messages,
    max: input.max,
  })
}
