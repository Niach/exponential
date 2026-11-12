import { readFileSync } from "node:fs"
import { vi } from "vitest"
import { contract } from "@exp/domain-contract"
import { createSteerSessionStore } from "@/lib/steer-session-store"
import { ACTIVITY_FLUSH_MS, FEED_WINDOW, groupFeedRows, shownFeedRows } from "@/lib/agent-feed"

// Large chats (steer-enet.json REPLAY TAIL): the journals the perf tests open,
// replayed the way a device answers a `join`.

export const REPLAY_TAIL = contract.steerFeed.replayTail
export const SLOT_KINDS = new Set([`config_state`, `usage`, `context_layout`, `rate_limit`, `turn`, `queue`, `background_tasks`, `task_list`, `diff`, `workflow`])

export type Event = Record<string, unknown> & { kind: string }

export function syntheticJournal(count: number): Event[] {
  const events: Event[] = []
  const output = `x`.repeat(1800)
  for (let i = 0; events.length < count; i++) {
    if (i % 40 === 0) events.push({ kind: `user_message`, text: `request ${i}` })
    if (i % 40 === 0) events.push({ kind: `turn`, state: `started` })
    events.push({ kind: `narration`, text: `Looking at **file ${i}**:\n\n- one\n- two` })
    events.push({ kind: `tool`, name: `Bash`, detail: `cat file-${i}`, id: `t${i}`, toolKind: `execute` })
    events.push({ kind: `tool_update`, id: `t${i}`, status: `completed`, output })
  }
  return events.slice(0, count)
}

export function realJournal(path: string): Event[] {
  const events: Event[] = []
  for (const line of readFileSync(path, `utf8`).split(`\n`)) {
    if (!line.trim()) continue
    try {
      const event = JSON.parse(line) as Event
      if (event.kind !== `diff_ref`) events.push(event)
    } catch {
      // a torn line, skipped like the device does
    }
  }
  return events
}

/** The device's join replay: the log (or its newest `tail`), then the newest
 *  event of each latest-wins slot. */
export function replay(events: Event[], tail: number | null): { frames: string[]; truncated: boolean } {
  const log: [number, Event][] = []
  const slots = new Map<string, [number, Event]>()
  events.forEach((event, seq) => (SLOT_KINDS.has(event.kind) ? slots.set(event.kind, [seq, event]) : log.push([seq, event])))
  const kept = tail === null ? log : log.slice(-tail)
  const frames = [`{"t":"activity_reset"}`]
  for (const [seq, event] of [...kept, ...slots.values()]) frames.push(JSON.stringify({ t: `activity`, event, seq }))
  const truncated = kept.length < log.length
  frames.push(JSON.stringify({ t: `activity_synced`, firstSeq: kept[0]?.[0] ?? 0, lastSeq: kept.at(-1)?.[0] ?? 0, ...(truncated ? { truncated } : {}) }))
  return { frames, truncated }
}

/** Wall time (the fake timers leave `performance` real). */
const wallMs = () => performance.now()

export async function openChat(frames: string[], window = FEED_WINDOW) {
  let socket: { onopen?: () => void; onmessage?: (e: { data: string }) => void; readyState: number; send: () => void; close: () => void } | null = null
  const store = createSteerSessionStore(
    `session-1`,
    {
      openFeed: async () => {
        socket = { readyState: 0, send: () => {}, close: () => {} }
        return socket as unknown as WebSocket
      },
    },
    () => {}
  )
  store.connect()
  await vi.advanceTimersByTimeAsync(0)
  const s = socket!
  s.readyState = WebSocket.OPEN
  s.onopen?.()
  const bytes = frames.reduce((sum, frame) => sum + frame.length, 0)
  const started = wallMs()
  for (const data of frames) s.onmessage?.({ data })
  // The store coalesces frames per `ACTIVITY_FLUSH_MS`: run that clock.
  await vi.advanceTimersByTimeAsync(ACTIVITY_FLUSH_MS)
  const feed = store.getSnapshot().feed
  const windowStart = Math.max(0, feed.length - window)
  const rows = shownFeedRows(groupFeedRows(feed, windowStart))
  const ms = wallMs() - started
  const snapshot = store.getSnapshot()
  store.dispose()
  return { bytes, items: feed.length, rows: rows.length, ms, canLoadEarlier: snapshot.canLoadEarlier, snapshot }
}

