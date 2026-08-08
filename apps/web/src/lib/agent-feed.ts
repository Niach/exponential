// Pure helpers for the agent-session activity feed (EXP-78; steer protocol v2
// in EXP-249) — kept out of the component so the local-echo dedupe, the
// answerable-question rule, the multi-question stepper and the answer lock
// state machine are unit testable.

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

/** Client-side feed cap — old events fall off the top. Matches the relay's
 *  ACTIVITY_LOG_CAP so a full-history replay renders in full. */
export const FEED_CAP = 2000

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
 *  the legacy no-protocol-change signal that a pending plan approval was
 *  answered. Protocol v2 desktops still emit it beside `question_resolved`. */
export const PLAN_RESOLVED_NARRATION = `Plan approval answered.`

/** The desktop's answered-question narration prefix (steer/src/activity.rs,
 *  EXP-197): one `Question answered: <answer>` narration per question flushes
 *  with the transcript once an AskUserQuestion resolves — folded into the
 *  earliest unanswered question card instead of rendering as a narration. */
export const QUESTION_ANSWERED_PREFIX = `Question answered: `

/** The desktop's dismissed-question narration (EXP-197) — the ask resolved
 *  WITHOUT answers (Esc / rejected); retires every pending question card. */
export const QUESTION_DISMISSED_NARRATION = `Question dismissed.`

/** The feed `question` item the helpers reason over. `questionId` is the wire
 *  id (protocol v2): present ⇒ resolution arrives as explicit events and the
 *  card is answerable through the semantic `answer` frame; absent ⇒ a legacy
 *  card, answerable by raw keystroke and retired by narration matching. */
export interface QuestionLike {
  id: number
  kind: string
  planMode?: boolean
  /** Set once the question resolved (EXP-197) — never answerable again. */
  resolved?: boolean
  /** The chosen answer, when the resolution carried one. */
  answer?: string
  /** The resolution carried no answer (Esc / rejected). */
  dismissed?: boolean
  questionId?: string
  askId?: string
  index?: number
  total?: number
  text?: string
}

/** Whether the desktop publishes question identities — new clients then prefer
 *  the semantic resolution events and ignore the legacy magic narrations. */
export function hasSemanticQuestions(
  feed: readonly { kind: string; questionId?: string }[]
): boolean {
  return feed.some((i) => i.kind === `question` && i.questionId !== undefined)
}

/** Fold an answer into the EARLIEST unanswered non-plan question card
 *  (answers arrive in question order, so earliest-first keeps multi-question
 *  asks aligned). Legacy path only. Null when no card is waiting — the caller
 *  falls back to rendering the narration so the answer is never lost. */
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
 *  Legacy path only. Null when nothing was pending. */
export function dismissPendingQuestions<T extends QuestionLike>(
  feed: readonly T[]
): T[] | null {
  const pending = (i: T) =>
    i.kind === `question` && i.planMode !== true && i.resolved !== true
  if (!feed.some(pending)) return null
  return feed.map((i) => (pending(i) ? { ...i, resolved: true } : i))
}

/** Replace the card carrying `questionId` in place — protocol v2 re-emits a
 *  question as the desktop learns more about it (augmented options, a header),
 *  and the card must keep its feed position, its local identity and any
 *  resolution already applied. Null when the id is unknown (append instead). */
export function upsertQuestion<T extends QuestionLike>(
  feed: readonly T[],
  questionId: string,
  next: Omit<T, `id`>
): T[] | null {
  const index = feed.findIndex(
    (i) => i.kind === `question` && i.questionId === questionId
  )
  if (index < 0) return null
  const prev = feed[index]
  const merged = [...feed]
  merged[index] = {
    ...prev,
    ...next,
    id: prev.id,
    resolved: prev.resolved,
    answer: prev.answer,
    dismissed: prev.dismissed,
  }
  return merged
}

/** A `question_resolved` event (protocol v2). */
export interface QuestionResolution {
  id?: string
  askId?: string
  answers?: string[]
  dismissed?: boolean
}

/** Apply a `question_resolved` event: retire the card with the matching wire
 *  id, else EVERY card of `askId`, else every pending card. Answers land
 *  positionally on the answer-consuming cards (the ask's submit step consumes
 *  none); a by-id resolution folds all its answers into that one card. Null
 *  when nothing matched. */
export function applyQuestionResolved<T extends QuestionLike>(
  feed: readonly T[],
  resolution: QuestionResolution
): T[] | null {
  const answers = resolution.answers ?? []
  const matches = (item: T) => {
    if (item.kind !== `question`) return false
    if (resolution.id !== undefined) return item.questionId === resolution.id
    if (resolution.askId !== undefined) return item.askId === resolution.askId
    return item.resolved !== true
  }
  // The submit step of an ask (askId, no index) is a confirmation, not a
  // question — it never consumes one of the ask's answers.
  const consumesAnswer = (item: T) =>
    item.askId === undefined || item.index !== undefined
  let matched = false
  let cursor = 0
  const next = feed.map((item) => {
    if (!matches(item)) return item
    matched = true
    let answer: string | undefined
    if (resolution.dismissed !== true && consumesAnswer(item)) {
      answer =
        resolution.id !== undefined
          ? answers.join(`, `) || undefined
          : answers[cursor++]
    }
    return {
      ...item,
      resolved: true,
      dismissed: resolution.dismissed === true ? true : item.dismissed,
      answer: answer ?? item.answer,
    }
  })
  return matched ? next : null
}

/** Ids of the `question` items still answerable.
 *
 *  Protocol v2 cards (a wire `questionId`) are identity-scoped: they stay
 *  answerable until an explicit `question_resolved` retires them, no matter
 *  what flushes in behind them.
 *
 *  Legacy cards keep the EXP-174 heuristic: the TRAILING consecutive question
 *  run (a multi-question batch lands back-to-back and the TUI auto-advances in
 *  order), PLUS any plan-approval card with no resolution signal after it —
 *  plan questions are published from the live terminal grid the moment the
 *  picker appears while the transcript tail lags, so tool rows and narration
 *  flush in BEHIND a picker that is still on screen. Only a newer question or
 *  the desktop's explicit `PLAN_RESOLVED_NARRATION` proves a plan picker
 *  resolved — a human message does NOT (steering mid-plan leaves the picker
 *  up). */
export function activeQuestionIds(
  feed: readonly {
    id: number
    kind: string
    planMode?: boolean
    resolved?: boolean
    questionId?: string
    text?: string
  }[]
): Set<number> {
  const ids = new Set<number>()
  for (const item of feed) {
    if (
      item.kind === `question` &&
      item.questionId !== undefined &&
      item.resolved !== true
    )
      ids.add(item.id)
  }
  // Still inside the trailing consecutive question run.
  let trailing = true
  // A resolution signal lies after the current position.
  let retired = false
  for (let i = feed.length - 1; i >= 0; i--) {
    const item = feed[i]
    if (item.kind === `question`) {
      if (item.resolved === true || item.questionId !== undefined) {
        // An answered/dismissed card is itself a resolution signal (it proves
        // the TUI moved past it), as is any newer question (EXP-197).
        trailing = false
        retired = true
      } else {
        if (trailing || (item.planMode === true && !retired)) ids.add(item.id)
        retired = true
      }
    } else {
      trailing = false
      if (
        item.kind === `narration` &&
        item.text?.trim() === PLAN_RESOLVED_NARRATION
      )
        retired = true
    }
    if (retired && !trailing) break
  }
  return ids
}

// ── Answer lock state machine (protocol v2) ──────────────────────────────────

export type AnswerStatus = `sending` | `acked` | `error`

export interface AnswerState {
  /** What was sent — option keys for a semantic answer, keystrokes legacy. */
  keys: string[]
  /** Option labels, rendered while the card is locked. */
  labels: string[]
  status: AnswerStatus
}

export type AnswerStates = Record<string, AnswerState>

/** No `answer_ack` within this long re-enables the card with an inline note —
 *  the desktop may be an older build, or the injection was lost. Derived from
 *  the desktop's worst-case ack budget (EXP-347): ANSWER_RETRY_TTL 4s +
 *  ANSWER_SETTLE 2s + PLAN_SUBMIT_PROBE 0.5s + ~1.5s tick/relay margin —
 *  iOS/Android parity, move all three in lockstep. */
export const ANSWER_ACK_TIMEOUT_MS = 8_000

/** The key a card's answer state is tracked under: the wire question id when
 *  the desktop publishes one, else the local feed id. */
export function answerKey(item: { id: number; questionId?: string }): string {
  return item.questionId ?? `#${item.id}`
}

/** True while a card must stay locked — an answer awaiting its ack, or one
 *  already confirmed. */
export function isAnswerLocked(state: AnswerState | undefined): boolean {
  return state?.status === `sending` || state?.status === `acked`
}

/** Lock a card the instant its answer goes out — no button may fire twice. */
export function beginAnswer(
  states: AnswerStates,
  key: string,
  keys: string[],
  labels: string[]
): AnswerStates {
  return { ...states, [key]: { keys, labels, status: `sending` } }
}

/** `answer_ack`: the desktop injected the answer — stay locked, confirmed. */
export function ackAnswer(states: AnswerStates, key: string): AnswerStates {
  const state = states[key]
  if (!state || state.status === `acked`) return states
  return { ...states, [key]: { ...state, status: `acked` } }
}

/** The ack never came — re-enable the card. An acked card stays locked. */
export function failAnswer(states: AnswerStates, key: string): AnswerStates {
  const state = states[key]
  if (state?.status !== `sending`) return states
  return { ...states, [key]: { ...state, status: `error` } }
}

/** Resolution finalizes a card — the feed item carries the answer from here. */
export function clearAnswer(states: AnswerStates, key: string): AnswerStates {
  if (!(key in states)) return states
  const next = { ...states }
  delete next[key]
  return next
}

// ── Multi-question stepper (protocol v2 `askId` groups) ───────────────────────

export type StepPhase = `answered` | `current` | `pending`

export interface AskStep<T> {
  item: T
  phase: StepPhase
  /** What to show on an answered step — the resolved answer when it arrived,
   *  else the locally picked labels. */
  answer?: string
}

export interface AskView<T> {
  /** The ask's numbered questions, in `index` order. */
  steps: AskStep<T>[]
  /** The final review/submit step, once the desktop published it. */
  submit: AskStep<T> | null
  /** Every published step is answered, no submit step arrived yet — the
   *  desktop is still walking the picker. */
  waiting: boolean
  /** 1-based position of the current step, and the ask's question count. */
  position: number
  total: number
}

/** Project one ask's question cards into a claude-style stepper: answered
 *  steps collapse with their answer, exactly one step is `current`, the rest
 *  wait. A step counts as answered once it resolved OR its answer is locked
 *  in flight — the lock is what makes the stepper advance the moment an
 *  `answer_ack` lands. */
export function askStepperView<T extends QuestionLike>(
  items: readonly T[],
  states: AnswerStates
): AskView<T> {
  const numbered = items
    .filter((i) => i.index !== undefined)
    .sort((a, b) => (a.index ?? 0) - (b.index ?? 0) || a.id - b.id)
  const submitItem =
    [...items].reverse().find((i) => i.index === undefined) ?? null

  const done = (item: T) =>
    item.resolved === true || isAnswerLocked(states[answerKey(item)])
  const answerOf = (item: T) => {
    if (item.answer !== undefined) return item.answer
    const labels = states[answerKey(item)]?.labels
    return labels?.length ? labels.join(`, `) : undefined
  }

  let currentTaken = false
  const steps: AskStep<T>[] = numbered.map((item) => {
    if (done(item))
      return { item, phase: `answered`, answer: answerOf(item) }
    if (!currentTaken) {
      currentTaken = true
      return { item, phase: `current` }
    }
    return { item, phase: `pending` }
  })

  let submit: AskStep<T> | null = null
  if (submitItem) {
    submit = done(submitItem)
      ? { item: submitItem, phase: `answered`, answer: answerOf(submitItem) }
      : { item: submitItem, phase: currentTaken ? `pending` : `current` }
  }

  const currentStep = steps.find((s) => s.phase === `current`)
  const total = numbered[0]?.total ?? numbered.length
  return {
    steps,
    submit,
    waiting:
      numbered.length > 0 &&
      !currentTaken &&
      submitItem === null &&
      !numbered.every((i) => i.resolved === true),
    position: currentStep?.item.index ?? total,
    total,
  }
}

// ── Markdown detection ───────────────────────────────────────────────────────

/** Syntax that makes a piece of feed text worth handing to the markdown
 *  renderer. Deliberately conservative: agent narration is mostly prose, and
 *  the plain path already linkifies bare URLs, so a miss costs nothing while a
 *  false positive would rewrite ordinary sentences (a URL's `_`/`-`/`#` must
 *  never read as emphasis, a heading or a rule). */
const MARKDOWN_PATTERNS: readonly RegExp[] = [
  // **bold**, *italic*, __bold__, ~~strike~~ — the opening marker must be
  // followed by a non-space, which is what keeps `2 * 3` and `a -- b` out.
  /(^|[^\w*])\*\*[^\s*][^*\n]*\*\*/,
  /(^|[^\w*])\*[^\s*][^*\n]*\*(?![\w*])/,
  /(^|[^\w_])__[^\s_][^_\n]*__/,
  /(^|[^~])~~[^\s~][^~\n]*~~/,
  // `inline code`
  /`[^`\n]+`/,
  // # heading
  /^ {0,3}#{1,6} +\S/m,
  // > blockquote
  /^ {0,3}> ?\S/m,
  // - / * / + / 1. list item
  /^ {0,3}([-*+]|\d{1,9}[.)]) +\S/m,
  // ``` fenced code
  /^ {0,3}(```|~~~)/m,
  // --- thematic break
  /^ {0,3}([-*_]) *(\1 *){2,}$/m,
  // ![alt](src) and [text](href) — the image form is the whole point of
  // rendering markdown in the feed at all (EXP-440).
  /!?\[[^\]\n]*\]\([^)\s]/,
  // | table | row |
  /^ {0,3}\|.*\|/m,
  // An indented code block, which markdown only opens after a blank line —
  // requiring that blank line keeps wrapped/indented prose out.
  /\n[ \t]*\n {4,}\S/,
]

/** Whether feed text should render through the markdown pipeline rather than
 *  as plain linkified text (EXP-440). */
export function looksLikeMarkdown(text: string): boolean {
  return MARKDOWN_PATTERNS.some((pattern) => pattern.test(text))
}

// ── Render rows ──────────────────────────────────────────────────────────────

/** A render row over the flat feed: one feed item, a run of ≥2 CONSECUTIVE
 *  plain `tool` items collapsed into a "N tool calls" row (EXP-97), one ask's
 *  question cards collapsed into a stepper, or a subagent's events plus the
 *  tool calls it made. `id` of a group is its FIRST item's id, so the row key
 *  (and its expanded state) stays stable while the group keeps growing. */
export type FeedRow<T extends { id: number; kind: string }> =
  | { kind: `single`; item: T }
  | { kind: `toolRun`; id: number; items: T[] }
  | { kind: `ask`; id: number; askId: string; items: T[] }
  | { kind: `subagent`; id: number; subagentId: string; items: T[] }

/** Group the flat feed into render rows — a pure projection: the feed (and
 *  `activeQuestionIds` over it) is never restructured, so answerability logic
 *  is unaffected. Grouped items are pulled out of their in-place position into
 *  the row their group opened. */
export function groupFeedRows<
  T extends {
    id: number
    kind: string
    askId?: string
    subagentId?: string
  },
>(feed: readonly T[]): FeedRow<T>[] {
  const rows: FeedRow<T>[] = []
  const askRows = new Map<string, Extract<FeedRow<T>, { kind: `ask` }>>()
  const subagentRows = new Map<
    string,
    Extract<FeedRow<T>, { kind: `subagent` }>
  >()
  for (let i = 0; i < feed.length; i++) {
    const item = feed[i]
    if (item.kind === `question` && item.askId !== undefined) {
      const open = askRows.get(item.askId)
      if (open) {
        open.items.push(item)
        continue
      }
      const row = {
        kind: `ask` as const,
        id: item.id,
        askId: item.askId,
        items: [item],
      }
      askRows.set(item.askId, row)
      rows.push(row)
      continue
    }
    if (
      (item.kind === `subagent` || item.kind === `tool`) &&
      item.subagentId !== undefined
    ) {
      const open = subagentRows.get(item.subagentId)
      if (open) {
        open.items.push(item)
        continue
      }
      const row = {
        kind: `subagent` as const,
        id: item.id,
        subagentId: item.subagentId,
        items: [item],
      }
      subagentRows.set(item.subagentId, row)
      rows.push(row)
      continue
    }
    if (item.kind !== `tool`) {
      rows.push({ kind: `single`, item })
      continue
    }
    let end = i
    while (
      end + 1 < feed.length &&
      feed[end + 1].kind === `tool` &&
      feed[end + 1].subagentId === undefined
    )
      end++
    if (end === i) rows.push({ kind: `single`, item })
    else
      rows.push({ kind: `toolRun`, id: item.id, items: feed.slice(i, end + 1) })
    i = end
  }
  return rows
}

/** `subagent.agentType` when the desktop's hook payload carried none — old
 *  desktop builds also stamp it onto the COMPLETED edge, so it is a sentinel
 *  the label selection must skip past, never a type to prefer (EXP-350). */
export const SUBAGENT_FALLBACK_TYPE = `agent`

/** One subagent's summary for tab navigation (EXP-356). */
export interface SubagentSummary {
  subagentId: string
  agentType: string
  done: boolean
  detail?: string
  toolCount: number
}

/** Every subagent seen in the feed, in first-appearance order, each summarized
 *  like its group row (EXP-356) — the session view renders one conversation
 *  tab per entry. */
export function collectSubagents<
  T extends {
    kind: string
    subagentId?: string
    agentType?: string
    status?: string
    detail?: string
  },
>(feed: readonly T[]): SubagentSummary[] {
  const order: string[] = []
  const byId = new Map<string, T[]>()
  for (const item of feed) {
    if (
      (item.kind !== `subagent` && item.kind !== `tool`) ||
      item.subagentId === undefined
    )
      continue
    let bucket = byId.get(item.subagentId)
    if (!bucket) {
      bucket = []
      byId.set(item.subagentId, bucket)
      order.push(item.subagentId)
    }
    bucket.push(item)
  }
  return order.map((subagentId) => ({
    subagentId,
    ...summarizeSubagentRow(byId.get(subagentId) ?? []),
  }))
}

/** The tabs the strip actually shows (EXP-387): running subagents, plus the
 *  focused one even when done — a completion never yanks the user out of a
 *  conversation they are reading; the tab disappears once they click away.
 *  Completed runs stay readable via their inline group row in Main. */
export function visibleSubagentTabs(
  agents: readonly SubagentSummary[],
  selected: string | null
): SubagentSummary[] {
  return agents.filter((a) => !a.done || a.subagentId === selected)
}

/** What a subagent group row displays (EXP-350) — one place for the label /
 *  status / detail selection so all clients can mirror it:
 *  - `agentType`: the first marker's real type — a later marker carrying the
 *    fallback (an old desktop's completed edge) can never degrade the label;
 *  - `done`: any marker completed;
 *  - `detail`: the LATEST non-empty detail (the completed edge restates the
 *    freshest);
 *  - `toolCount`: the tool calls attributed to the subagent. */
export function summarizeSubagentRow<
  T extends { kind: string; agentType?: string; status?: string; detail?: string },
>(
  items: readonly T[]
): { agentType: string; done: boolean; detail?: string; toolCount: number } {
  const markers = items.filter((i) => i.kind === `subagent`)
  const types = markers
    .map((m) => m.agentType?.trim() ?? ``)
    .filter((t) => t !== ``)
  return {
    agentType:
      types.find((t) => t !== SUBAGENT_FALLBACK_TYPE) ??
      types[0] ??
      SUBAGENT_FALLBACK_TYPE,
    done: markers.some((m) => m.status === `completed`),
    detail: [...markers].reverse().find((m) => m.detail?.trim())?.detail,
    toolCount: items.filter((i) => i.kind === `tool`).length,
  }
}
