// Pure helpers for the agent-session activity feed (EXP-78) — kept out of the
// component so the local-echo dedupe and the answerable-question rule are unit
// testable.

/** A locally-echoed steered message awaiting its transcript-derived twin. */
export interface EchoEntry {
  text: string
  at: number
}

/** At most this many un-matched echoes are remembered. */
export const ECHO_CAP = 8
/** Echoes older than this stop matching — a mid-turn steered message can take
 *  a while to hit the transcript, but an unmatched echo must not swallow an
 *  identical message sent much later from another device. */
export const ECHO_TTL_MS = 5 * 60_000

/** Record a just-sent message so its transcript-derived `user_message` event
 *  is not appended a second time. Mutates `echoes` in place. */
export function pushEcho(echoes: EchoEntry[], text: string, now: number): void {
  echoes.push({ text: text.trim(), at: now })
  if (echoes.length > ECHO_CAP) echoes.splice(0, echoes.length - ECHO_CAP)
}

/** Whether an incoming `user_message` matches a recent local echo. Consumes
 *  the matched entry (and evicts expired ones) — returns true when the event
 *  should be SKIPPED. */
export function consumeEcho(
  echoes: EchoEntry[],
  text: string,
  now: number
): boolean {
  for (let i = echoes.length - 1; i >= 0; i--) {
    if (now - echoes[i].at > ECHO_TTL_MS) echoes.splice(i, 1)
  }
  const needle = text.trim()
  const index = echoes.findIndex((e) => e.text === needle)
  if (index === -1) return false
  echoes.splice(index, 1)
  return true
}

/** Ids of the TRAILING consecutive run of `question` items — the only ones
 *  still answerable. A multi-question batch lands back-to-back and the TUI
 *  auto-advances in order, so the whole trailing run stays active; any later
 *  event means the session moved on and every earlier question is stale. */
export function trailingQuestionIds(
  feed: readonly { id: number; kind: string }[]
): Set<number> {
  const ids = new Set<number>()
  for (let i = feed.length - 1; i >= 0 && feed[i].kind === `question`; i--) {
    ids.add(feed[i].id)
  }
  return ids
}

/** A render row over the flat feed (EXP-97): either one feed item, or a run
 *  of ≥2 CONSECUTIVE `tool` items collapsed into a single "N tool calls" row.
 *  `id` of a run is the FIRST tool's id, so the row key (and its expanded
 *  state) stays stable while the trailing run keeps growing. */
export type FeedRow<T extends { id: number; kind: string }> =
  | { kind: `single`; item: T }
  | { kind: `toolRun`; id: number; items: T[] }

/** Group consecutive runs of ≥2 `tool` items into `toolRun` rows — a pure
 *  render-time projection: the flat feed (and `trailingQuestionIds` over it)
 *  is never restructured, so answerability logic is unaffected. */
export function groupToolRuns<T extends { id: number; kind: string }>(
  feed: readonly T[]
): FeedRow<T>[] {
  const rows: FeedRow<T>[] = []
  for (let i = 0; i < feed.length; i++) {
    if (feed[i].kind !== `tool`) {
      rows.push({ kind: `single`, item: feed[i] })
      continue
    }
    let end = i
    while (end + 1 < feed.length && feed[end + 1].kind === `tool`) end++
    if (end === i) rows.push({ kind: `single`, item: feed[i] })
    else rows.push({ kind: `toolRun`, id: feed[i].id, items: feed.slice(i, end + 1) })
    i = end
  }
  return rows
}
