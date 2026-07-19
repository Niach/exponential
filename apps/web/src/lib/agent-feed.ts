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

/** The desktop's plan-picker resolution narration (steer/src/activity.rs) —
 *  the no-protocol-change signal that a pending plan approval was answered. */
export const PLAN_RESOLVED_NARRATION = `Plan approval answered.`

/** The desktop's answered-question narration prefix (steer/src/activity.rs,
 *  EXP-197): one `Question answered: <answer>` narration per question flushes
 *  with the transcript once an AskUserQuestion resolves — folded into the
 *  earliest unanswered question card instead of rendering as a narration. */
export const QUESTION_ANSWERED_PREFIX = `Question answered: `

/** The desktop's dismissed-question narration (EXP-197) — the ask resolved
 *  WITHOUT answers (Esc / rejected); retires every pending question card. */
export const QUESTION_DISMISSED_NARRATION = `Question dismissed.`

interface QuestionLike {
  id: number
  kind: string
  planMode?: boolean
  /** Set once the question resolved (EXP-197) — never answerable again. */
  resolved?: boolean
  /** The chosen answer, when the resolution carried one. */
  answer?: string
}

/** Fold an answer into the EARLIEST unanswered non-plan question card
 *  (answers arrive in question order, so earliest-first keeps multi-question
 *  asks aligned). Null when no card is waiting — the caller falls back to
 *  rendering the narration so the answer is never lost. */
export function attachQuestionAnswer<T extends QuestionLike>(
  feed: readonly T[],
  answer: string
): T[] | null {
  const index = feed.findIndex(
    (i) => i.kind === `question` && i.planMode !== true && i.resolved !== true
  )
  if (index < 0) return null
  const next = [...feed]
  next[index] = { ...next[index], resolved: true, answer }
  return next
}

/** Retire every pending non-plan question card (the ask was dismissed).
 *  Null when nothing was pending. */
export function dismissPendingQuestions<T extends QuestionLike>(
  feed: readonly T[]
): T[] | null {
  const pending = (i: T) =>
    i.kind === `question` && i.planMode !== true && i.resolved !== true
  if (!feed.some(pending)) return null
  return feed.map((i) => (pending(i) ? { ...i, resolved: true } : i))
}

/** Ids of the `question` items still answerable (EXP-174): the TRAILING
 *  consecutive question run (a multi-question batch lands back-to-back and
 *  the TUI auto-advances in order; any later event means the session moved
 *  on), PLUS any plan-approval question with no resolution signal after it.
 *  Plan questions are published from the live terminal grid the moment the
 *  picker appears, while the transcript tail lags — so tool rows and
 *  narration can flush in BEHIND a plan card whose picker is still on
 *  screen. Only a newer question, a human message, or the desktop's explicit
 *  `PLAN_RESOLVED_NARRATION` proves a plan picker actually resolved. */
export function activeQuestionIds(
  feed: readonly {
    id: number
    kind: string
    planMode?: boolean
    resolved?: boolean
    text?: string
  }[]
): Set<number> {
  const ids = new Set<number>()
  // Still inside the trailing consecutive question run.
  let trailing = true
  // A resolution signal lies after the current position.
  let retired = false
  for (let i = feed.length - 1; i >= 0; i--) {
    const item = feed[i]
    if (item.kind === `question`) {
      if (item.resolved === true) {
        // An answered/dismissed card is itself a resolution signal (it
        // proves the TUI moved past it) and is never active (EXP-197).
        trailing = false
        retired = true
      } else {
        if (trailing || (item.planMode === true && !retired)) ids.add(item.id)
        retired = true
      }
    } else {
      trailing = false
      if (item.kind === `user_message`) retired = true
      else if (
        item.kind === `narration` &&
        item.text?.trim() === PLAN_RESOLVED_NARRATION
      )
        retired = true
    }
    if (retired && !trailing) break
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
 *  render-time projection: the flat feed (and `activeQuestionIds` over it)
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
