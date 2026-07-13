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
