// Pure helpers for the agent-session activity feed (EXP-78; steer protocol v2
// in EXP-249) — kept out of the component so the local-echo dedupe, the
// answerable-question rule, the multi-question stepper and the answer lock
// state machine are unit testable.

// EXP-746: the two chip labels live in steer-commands.ts (the ×4 copy home);
// the import is one-way — steer-commands only takes TYPES from here, so the
// merged `/` catalog and the chips never form a runtime cycle.
import {
  CONFIG_DEFAULT_VALUE_LABEL,
  CONFIG_MODE_LABEL,
} from "@/lib/steer-commands"
import { formatResetCountdown } from "@/lib/agent-usage"
import { contract, toolGroupSummary } from "@exp/domain-contract"
// EXP-787: the transcript's rhythm is a shared token group, read straight from
// the canonical tokens.json (@exp/design-tokens is not a dependency of this
// app — see design-tokens.test.ts, which reads the same file by path).
import designTokens from "../../../../packages/design-tokens/tokens.json" with { type: "json" }

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

/** EXP-783: the transcript keeps the WHOLE run — the renderer paints a WINDOW
 *  over it (`agent-session.tsx`) and grows it upward, so keeping everything
 *  costs nothing per frame. These are safety ceilings on a tab's memory, not
 *  a display cap, and they are sized to the device journal (JOURNAL_FILE_CAP)
 *  because that file is exactly what a replay reads back.
 *
 *  The relay's ACTIVITY_LOG_CAP is deliberately NOT matched any more: it
 *  bounds the tail a joining viewer replays, and older pages are asked for
 *  (`history_page`) rather than pushed.
 *
 *  Every number here is the contract's `steerFeed` section (EXP-795): the
 *  desktop, iOS and Android reducers read the same generated constants, so
 *  the budgets, the trim target and the window cannot drift per client. */
export const FEED_BYTE_CAP = contract.steerFeed.byteCap
/** The companion item ceiling — a run of tiny events would sit far under the
 *  byte budget while costing an array slot each. */
export const FEED_ITEM_CAP = contract.steerFeed.itemCap
/** How many of the run's newest rows the transcript renders, and how much
 *  older transcript one "Load earlier" pulls in. */
export const FEED_WINDOW = contract.steerFeed.window
export const FEED_WINDOW_STEP = contract.steerFeed.windowStep
/** Events per `history_page` ask — the relay's schema rejects anything
 *  larger. */
export const HISTORY_PAGE_LIMIT = contract.steerFeed.historyPageMax

/** What one row weighs against FEED_BYTE_CAP: the text it carries plus a flat
 *  per-item overhead standing in for the object itself. An estimate on
 *  purpose — this budget bounds memory, it does not account for it. */
export function feedItemBytes(item: {
  kind: string
  text?: string
  name?: string
  detail?: string
  tool?: string
  answer?: string
  header?: string
  /** EXP-786: a tool row's folded per-call diff weighs too. */
  diff?: string
  options?: { label: string; key: string }[]
}): number {
  const OVERHEAD = contract.steerFeed.itemOverheadBytes
  const len = (value?: string) => value?.length ?? 0
  return (
    OVERHEAD +
    len(item.text) +
    len(item.name) +
    len(item.detail) +
    len(item.tool) +
    len(item.answer) +
    len(item.header) +
    len(item.diff) +
    (item.options?.reduce((sum, o) => sum + o.label.length + o.key.length, 0) ?? 0)
  )
}

/** EXP-785: ACP's tool-call kind buckets, the contract's `toolKind.values`
 *  (locked by agent-feed.test.ts). A `tool` event carries one as `toolKind`
 *  — never `kind`, which is the event discriminator. */
export type ToolKind =
  | `read`
  | `edit`
  | `delete`
  | `move`
  | `search`
  | `execute`
  | `think`
  | `fetch`
  | `switch_mode`
  | `other`

export const TOOL_KINDS: readonly ToolKind[] = [
  `read`,
  `edit`,
  `delete`,
  `move`,
  `search`,
  `execute`,
  `think`,
  `fetch`,
  `switch_mode`,
  `other`,
]

/** A wire `toolKind`, or undefined for anything this build does not know. */
export function parseToolKind(value: unknown): ToolKind | undefined {
  return typeof value === `string` && (TOOL_KINDS as readonly string[]).includes(value)
    ? (value as ToolKind)
    : undefined
}

/** Evict from the OLDEST end until the feed is inside both budgets, with the
 *  same 90% hysteresis the desktop uses (`steer::feed::trim`): evicting down
 *  to the budget exactly would evict again on the very next event, and every
 *  eviction reallocates. The newest row always survives. */
export function trimFeed<T extends { kind: string }>(
  feed: T[],
  bytes: number
): { feed: T[]; bytes: number } {
  if (bytes <= FEED_BYTE_CAP && feed.length <= FEED_ITEM_CAP) return { feed, bytes }
  const percent = contract.steerFeed.trimTargetPercent
  const byteTarget = Math.floor((FEED_BYTE_CAP * percent) / 100)
  const itemTarget = Math.floor((FEED_ITEM_CAP * percent) / 100)
  let remaining = bytes
  let dropTo = 0
  while (
    dropTo + 1 < feed.length &&
    (remaining > byteTarget || feed.length - dropTo > itemTarget)
  ) {
    remaining -= feedItemBytes(feed[dropTo])
    dropTo++
  }
  return dropTo > 0
    ? { feed: feed.slice(dropTo), bytes: Math.max(0, remaining) }
    : { feed, bytes }
}

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

// ── Compaction (EXP-724) ─────────────────────────────────────────────────────

/** A backstop on the `compaction started` strip. Compaction really does take
 *  minutes (10-170s measured locally), but a publisher that dies mid-fold, or
 *  an agent whose end marker never arrives, must not leave an indeterminate
 *  bar running forever. Mirrors the desktop's COMPACTION_TIMEOUT (180s) —
 *  move all four clients in lockstep. */
export const COMPACTION_TIMEOUT_MS = 180_000

/** Whether an activity kind proves the agent is WORKING again, which ends a
 *  compaction strip that never got its `ended` marker (codex publishes no
 *  start marker at all, and any agent can drop one on a crash).
 *
 *  Deliberately narrow: `user_message` is the human typing, `diff` is the
 *  branch bar refreshing, and the answer/permission/resolution events are all
 *  bookkeeping around a card that was already on screen — none of them means
 *  the agent resumed. */
export function resumesAfterCompaction(kind: string): boolean {
  return (
    kind === `narration` ||
    kind === `tool` ||
    kind === `question` ||
    kind === `subagent`
  )
}

// ── Frame coalescing (REV-33) ────────────────────────────────────────────────

/** How long incoming activity frames buffer before flushing to state. A join
 *  replay (and a desktop re-publish after `activity_reset`) fans the relay's
 *  whole log — up to the relay's ACTIVITY_LOG_CAP frames — out as individual ws messages, each its
 *  own browser task; applying them one state update at a time re-rendered the
 *  full non-virtualized feed per frame, O(n²) element work that froze the tab
 *  for seconds. One batched apply per window is a handful of renders instead,
 *  and 50ms is imperceptible on live events. */
export const ACTIVITY_FLUSH_MS = 50

export interface ActivityCoalescer<Op> {
  /** Buffer one op. The FIRST op in a quiet window schedules the flush and
   *  later ops ride it (a throttle, not a debounce — a continuous stream can
   *  never starve rendering). */
  enqueue(op: Op): void
  /** Drop the un-flushed ops and the pending timer (socket teardown). */
  cancel(): void
}

/** Hand buffered ops to `apply` in arrival order, at most once per `delayMs`.
 *  `apply` runs the whole batch in one synchronous pass so React folds every
 *  state update it makes into a single render. */
export function createActivityCoalescer<Op>(
  apply: (batch: Op[]) => void,
  delayMs: number = ACTIVITY_FLUSH_MS
): ActivityCoalescer<Op> {
  let pending: Op[] = []
  let timer: ReturnType<typeof setTimeout> | null = null
  const flush = () => {
    timer = null
    const batch = pending
    pending = []
    if (batch.length > 0) apply(batch)
  }
  return {
    enqueue(op) {
      pending.push(op)
      timer ??= setTimeout(flush, delayMs)
    },
    cancel() {
      if (timer !== null) clearTimeout(timer)
      timer = null
      pending = []
    },
  }
}

/** The feed `question` item the helpers reason over. `questionId` is the wire
 *  id (protocol v2) every card carries: resolution arrives as explicit events
 *  naming it, and the card is answerable through the `answer` frame. It is
 *  declared optional only because these helpers walk a MIXED feed, whose
 *  other kinds have none. */
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

/** Insert `item` immediately BEFORE the first question card matching
 *  `anchor` (its `askId` or wire `questionId`) — EXP-483: claude withholds
 *  the transcript entry carrying an ask/plan tool_use, prose included, until
 *  the picker resolves, so that prose arrives AFTER the already-published
 *  card and tags itself with `beforeQuestionId` to be spliced back above it.
 *  Matches resolved cards too (the twin normally flushes post-answer). Null
 *  when no card matches (evicted, legacy producer) — the caller appends. */
export function spliceBeforeQuestion<
  T extends { kind: string; questionId?: string; askId?: string },
>(feed: readonly T[], anchor: string, item: T): T[] | null {
  const index = feed.findIndex(
    (i) =>
      i.kind === `question` && (i.askId === anchor || i.questionId === anchor)
  )
  if (index < 0) return null
  return [...feed.slice(0, index), item, ...feed.slice(index)]
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

/** Ids of the `question` items still answerable. Every card carries a wire
 *  `questionId` and those are IDENTITY-scoped: they stay answerable until an
 *  explicit `question_resolved` retires them, no matter what flushes in
 *  behind them. */
export function activeQuestionIds(
  feed: readonly {
    id: number
    kind: string
    resolved?: boolean
    questionId?: string
  }[]
): Set<number> {
  const ids = new Set<number>()
  for (const item of feed) {
    if (item.kind === `question` && item.resolved !== true) ids.add(item.id)
  }
  return ids
}

// ── Answer lock state machine (protocol v2) ──────────────────────────────────

export type AnswerStatus = `sending` | `acked` | `error`

export interface AnswerState {
  /** The option keys the `answer` frame carried. */
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

/** The key a card's answer state is tracked under: its wire question id. The
 *  local-id fallback only keeps the lookup TOTAL over a mixed feed — a row
 *  that is not a question card never carries answer state. */
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
  /** EXP-820: the ask is over — its submit step resolved, its lone step
   *  resolved, or it was dismissed. A complete ask waits for nothing and its
   *  answered steps can no longer be revisited. */
  complete: boolean
  /** 1-based position of the current step, and the ask's question count. */
  position: number
  total: number
}

/** EXP-820: whether an ask is finished — the ONE rule ×4 (desktop
 *  `ask_complete`, iOS/Android `askComplete`): its submit step resolved, or a
 *  one-question ask's lone step resolved (the engine submits that on its
 *  answer, there is no review step), or any step was dismissed. Until then an
 *  answered step is still open to a change of mind. */
export function askComplete<
  T extends {
    index?: number
    total?: number
    resolved?: boolean
    dismissed?: boolean
  },
>(items: readonly T[]): boolean {
  const numbered = items.filter((i) => i.index !== undefined)
  const submit = [...items].reverse().find((i) => i.index === undefined)
  if (submit?.resolved === true) return true
  if (items.some((i) => i.dismissed === true)) return true
  const total = numbered[0]?.total ?? numbered.length
  return (
    total <= 1 &&
    numbered.length > 0 &&
    numbered.every((i) => i.resolved === true)
  )
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
  const complete = askComplete(items)
  return {
    steps,
    submit,
    waiting:
      !complete &&
      numbered.length > 0 &&
      !currentTaken &&
      submitItem === null &&
      !numbered.every((i) => i.resolved === true),
    complete,
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

/** EXP-773: the subagent a row belongs to, or `null` for a main-feed row —
 *  the accessor every caller uses, so a `FeedItem` member without the field
 *  never has to be narrowed at the call site. */
export function subagentIdOf(item: {
  kind: string
  subagentId?: string
}): string | null {
  return isSubagentScoped(item) ? (item.subagentId as string) : null
}

/** EXP-773: a row that belongs to a subagent rather than to the main feed.
 *  `subagent`/`tool` carried the id from the start; `narration` and
 *  `user_message` carry it since the ACP mapper stamps the parent tool call. */
export function isSubagentScoped(item: {
  kind: string
  subagentId?: string
}): boolean {
  if (item.subagentId === undefined) return false
  return (
    item.kind === `subagent` ||
    item.kind === `tool` ||
    item.kind === `narration` ||
    item.kind === `user_message`
  )
}

/** EXP-772: append a narration fragment onto the feed's LAST row when both
 *  carry the same `messageId` — the ACP coalescer flushes one assistant
 *  message in several events, and a row per flush shredded a paragraph into
 *  bubbles. Returns the new feed, or `null` when there is nothing to merge
 *  into (a different message, a row in between, no id at all). */
export function mergeNarrationFragment<
  T extends { kind: string; messageId?: string; text?: string; subagentId?: string },
>(
  feed: readonly T[],
  fragment: { messageId?: string; text: string; subagentId?: string }
): T[] | null {
  const id = fragment.messageId
  if (id === undefined || id === ``) return null
  const last = feed[feed.length - 1]
  if (!last || last.kind !== `narration`) return null
  if (last.messageId !== id) return null
  // A fragment that landed in a different scope is a different bubble.
  if (last.subagentId !== fragment.subagentId) return null
  return [
    ...feed.slice(0, -1),
    { ...last, text: `${last.text ?? ``}${fragment.text}` },
  ]
}

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
>(feed: readonly T[], start = 0): FeedRow<T>[] {
  const rows: FeedRow<T>[] = []
  const askRows = new Map<string, Extract<FeedRow<T>, { kind: `ask` }>>()
  const subagentRows = new Map<
    string,
    Extract<FeedRow<T>, { kind: `subagent` }>
  >()
  // EXP-783: `start` restricts the projection to the rendered WINDOW. The
  // grouping state begins empty there, so a window that cuts through a tool
  // run, an ask or a subagent's calls opens a FRESH group at the boundary —
  // keyed on the first item the reader can actually see. `start = 0` is the
  // whole projection, byte for byte.
  for (let i = Math.max(0, Math.min(start, feed.length)); i < feed.length; i++) {
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
    // EXP-773: everything a subagent produced — its lifecycle markers, its
    // tool calls AND the prose/user turns the mapper stamped with its id —
    // belongs to that subagent's row, never to the main feed.
    const scopedTo = isSubagentScoped(item) ? subagentIdOf(item) : null
    if (scopedTo !== null) {
      const open = subagentRows.get(scopedTo)
      if (open) {
        open.items.push(item)
        continue
      }
      const row = {
        kind: `subagent` as const,
        id: item.id,
        subagentId: scopedTo,
        items: [item],
      }
      subagentRows.set(scopedTo, row)
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

// ── Transcript rhythm (EXP-787) ──────────────────────────────────────────────
// The space ABOVE a row, chosen from the row before it. ONE derivation,
// mirrored ×4 (desktop steer::feed, ExpCore AgentFeed, Android AgentFeed) and
// fed by the shared `transcript` token group.

/** What a transcript row weighs in the gap ladder: a sent user message
 *  (`turn`), agent prose and the cards that read like prose (`prose`), or the
 *  compact machine rows (`tool`). */
export type RowClass = `turn` | `prose` | `tool`

/** The ladder class of a render row. The trailing "Working…" indicator is not
 *  a feed row — the renderer passes `tool` for it directly. */
export function rowClass<T extends { id: number; kind: string }>(
  row: FeedRow<T>
): RowClass {
  if (row.kind === `toolRun` || row.kind === `subagent`) return `tool`
  if (row.kind === `ask`) return `prose`
  switch (row.item.kind) {
    case `user_message`:
      return `turn`
    case `tool`:
    case `subagent`:
    case `permission`:
      return `tool`
    default:
      // narration, question, compaction and anything a newer publisher adds:
      // prose is the safe default — it never crowds an unknown row.
      return `prose`
  }
}

/** Which token a pairing lands on, `null` for the first rendered row. The
 *  renderer needs the NAME (it paints the matching `--transcript-gap-*` custom
 *  property) and the natives need the value, so the rule is derived once here
 *  and `transcriptGap` reads the number off it. */
export type TranscriptGapToken = `gapTurn` | `gapBlock` | `gapTool` | `gapDefault`

/** The ladder itself. Order matters: a user turn opens and closes a paragraph
 *  of its own, so it wins over every other pairing. */
export function transcriptGapToken(
  prev: RowClass | null,
  cur: RowClass
): TranscriptGapToken | null {
  if (prev === null) return null
  if (prev === `turn` || cur === `turn`) return `gapTurn`
  if (prev === `tool` && cur === `tool`) return `gapDefault`
  if (prev === `tool` || cur === `tool`) return `gapTool`
  return `gapBlock`
}

/** The gap above `cur`, given the row before it (`null` = the first rendered
 *  row, which gets none). */
export function transcriptGap(prev: RowClass | null, cur: RowClass): number {
  const token = transcriptGapToken(prev, cur)
  return token === null ? 0 : designTokens.transcript[token]
}

/** `subagent.agentType` when the desktop's hook payload carried none — old
 *  desktop builds also stamp it onto the COMPLETED edge, so it is a sentinel
 *  the label selection must skip past, never a type to prefer (EXP-350). */
export const SUBAGENT_FALLBACK_TYPE = `agent`

/** One subagent's summary for tab navigation (EXP-356). */
export interface SubagentSummary {
  subagentId: string
  agentType: string
  /** EXP-847: the spawning Agent call's description, when the publisher sent
   *  one — `subagentLabel` prefers it over `agentType`. */
  title?: string
  done: boolean
  detail?: string
  toolCount: number
}

/** EXP-847: what a subagent is CALLED on screen — the spawning call's
 *  description, falling back to the agent type (all a pre-EXP-847 publisher
 *  sends). Mirrored ×4 so the chips read the same everywhere. */
export function subagentLabel(summary: {
  agentType: string
  title?: string
}): string {
  const title = summary.title?.trim()
  return title ? title : summary.agentType
}

/** Every subagent seen in the feed, in first-appearance order, each summarized
 *  like its group row (EXP-356) — the session view renders one conversation
 *  tab per entry. */
export function collectSubagents<
  T extends {
    kind: string
    subagentId?: string
    agentType?: string
    title?: string
    status?: string
    detail?: string
  },
>(feed: readonly T[]): SubagentSummary[] {
  const order: string[] = []
  const byId = new Map<string, T[]>()
  for (const item of feed) {
    const subagentId = subagentIdOf(item)
    if (subagentId === null) continue
    let bucket = byId.get(subagentId)
    if (!bucket) {
      bucket = []
      byId.set(subagentId, bucket)
      order.push(subagentId)
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
 *  - `title` (EXP-847): the first marker's non-empty description, what
 *    `subagentLabel` shows instead of the type;
 *  - `done`: any marker completed;
 *  - `detail`: the LATEST non-empty detail (the completed edge restates the
 *    freshest);
 *  - `toolCount`: the tool calls attributed to the subagent — the publisher's
 *    own `toolCalls` on a marker wins over the tool rows still in the feed
 *    (EXP-748: replay buffers evict subagent tool rows first, so the visible
 *    ones undercount), and the visible count wins when no marker reports. */
export function summarizeSubagentRow<
  T extends {
    kind: string
    agentType?: string
    title?: string
    status?: string
    detail?: string
    toolCalls?: number
  },
>(
  items: readonly T[]
): {
  agentType: string
  title?: string
  done: boolean
  detail?: string
  toolCount: number
} {
  const markers = items.filter((i) => i.kind === `subagent`)
  const types = markers
    .map((m) => m.agentType?.trim() ?? ``)
    .filter((t) => t !== ``)
  const reported = markers.reduce(
    (max, m) => (typeof m.toolCalls === `number` && m.toolCalls > max ? m.toolCalls : max),
    0
  )
  return {
    agentType:
      types.find((t) => t !== SUBAGENT_FALLBACK_TYPE) ??
      types[0] ??
      SUBAGENT_FALLBACK_TYPE,
    title: markers.map((m) => m.title?.trim()).find((t) => t),
    done: markers.some((m) => m.status === `completed`),
    detail: [...markers].reverse().find((m) => m.detail?.trim())?.detail,
    toolCount: Math.max(items.filter((i) => i.kind === `tool`).length, reported),
  }
}

// ── Live agent config + usage (EXP-746) ──────────────────────────────────────
// The ACP engine publishes the agent's configuration and its context meter as
// LATEST-WINS STATE (relay `LATEST_WINS_KINDS`), never as feed rows — the
// `diff` precedent. The folds below are the ×4 rule home: iOS
// AgentFeed.applyConfigState/applyUsage, Android AgentFeed.kt, desktop
// feed.rs SessionConfig/SessionUsage.

/** One selectable value of a config option (`opus`, `high`). */
export interface SessionConfigValue {
  id: string
  label: string
}

/** One selectable session mode (`plan`, `acceptEdits`, …). */
export interface SessionConfigMode {
  id: string
  label: string
  description?: string
}

/** One command the AGENT itself advertises (ACP `available_commands_update`)
 *  — merged into the `/` menu behind the contract catalog. */
export interface SessionConfigCommand {
  name: string
  description: string
  hint?: string
}

/** EXP-746: the agent's live configuration — the modes it can switch to, the
 *  one in force and the commands it advertises. Latest-wins STATE, never a
 *  feed row (the `latestDiff` precedent). Mirrored ×4: iOS
 *  AgentSessionConfig, Android SessionConfigState, desktop feed.rs
 *  SessionConfig.
 *
 *  EXP-772: `options` is gone. Model, effort and every other picker left the
 *  mid-session UI — the engine publishes an empty option list and no client
 *  renders one, so the fold drops the member rather than carrying a value
 *  nothing reads. The MODE is the composer's one live control. */
export interface SessionConfigState {
  currentMode?: string
  modes: SessionConfigMode[]
  commands: SessionConfigCommand[]
}

/** EXP-746: the run's context window and spend as the engine last measured
 *  it. A TOKEN count, never a percent — the device's rate-limit windows
 *  (lib/agent-usage.ts) already own the 0-100 vocabulary. */
export interface SessionUsageState {
  contextUsed: number
  contextSize: number
  costUsd?: number
}

function isEventRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === `object` && value !== null && !Array.isArray(value)
}

/** A wire id: present, a string, non-blank. Machine fields are never
 *  trimmed — the publisher's id is what `set_mode` has to send back. */
function wireId(value: unknown): string | null {
  return typeof value === `string` && value.length > 0 ? value : null
}

function wireText(value: unknown): string {
  return typeof value === `string` ? value : ``
}

/** Tolerant fold of a `config_state` event; `null` for an unusable payload —
 *  the caller then KEEPS the previous snapshot rather than blanking the mode
 *  chip. Mirrors AgentFeed.applyConfigState / applyActivityEvent's arm.
 *
 *  EXP-772: a payload has to carry at least ONE of the arrays to be a config
 *  state at all. Older publishers send `options` (ignored now), the engine
 *  sends `modes`/`commands`; an object with none of them is noise. */
export function parseConfigState(event: unknown): SessionConfigState | null {
  if (!isEventRecord(event)) return null
  if (
    !Array.isArray(event.modes) &&
    !Array.isArray(event.commands) &&
    !Array.isArray(event.options)
  ) {
    return null
  }
  const modes: SessionConfigMode[] = []
  if (Array.isArray(event.modes)) {
    for (const entry of event.modes) {
      if (!isEventRecord(entry)) continue
      const id = wireId(entry.id)
      if (id === null) continue
      const mode: SessionConfigMode = { id, label: wireText(entry.label) || id }
      if (typeof entry.description === `string`) {
        mode.description = entry.description
      }
      modes.push(mode)
    }
  }
  const commands: SessionConfigCommand[] = []
  if (Array.isArray(event.commands)) {
    for (const entry of event.commands) {
      if (!isEventRecord(entry)) continue
      const name = wireId(entry.name)
      if (name === null) continue
      const command: SessionConfigCommand = {
        name,
        description: wireText(entry.description),
      }
      if (typeof entry.hint === `string`) command.hint = entry.hint
      commands.push(command)
    }
  }
  const state: SessionConfigState = { modes, commands }
  const currentMode = wireId(event.currentMode)
  if (currentMode !== null) state.currentMode = currentMode
  return state
}

/** `null` for an unusable payload OR a zero `contextSize` ("unknown"). Unlike
 *  the config fold, a null here CLEARS the slot: a context meter is only
 *  worth showing while the engine is measuring one, and a stale bar beside a
 *  live run reads as current. */
export function parseSessionUsage(event: unknown): SessionUsageState | null {
  if (!isEventRecord(event)) return null
  const used = event.contextUsed
  const size = event.contextSize
  if (typeof used !== `number` || !Number.isFinite(used) || used < 0) return null
  if (typeof size !== `number` || !Number.isFinite(size) || size <= 0) return null
  const usage: SessionUsageState = {
    contextUsed: Math.round(used),
    contextSize: Math.round(size),
  }
  const cost = event.costUsd
  if (typeof cost === `number` && Number.isFinite(cost) && cost >= 0) {
    usage.costUsd = cost
  }
  return usage
}

/** EXP-784: the agent's rate-limit window as it last reported it — the
 *  fourth latest-wins slot beside `SessionUsageState` (`journal.rs`,
 *  `hub.ts` LATEST_WINS_KINDS, iOS `AgentFeed.applyRateLimit`, Android
 *  `AgentFeed.kt`, desktop `feed.rs` SessionRateLimit). `status` is the
 *  agent's own word (`allowed_warning`, `rejected`, …). */
export interface SessionRateLimitState {
  status: string
  /** Unix ms when the window resets, when the agent named one. */
  resetsAt?: number
  message?: string
}

/** EXP-784: the `rate_limit.status` values that CLEAR the slot. */
export function rateLimitClears(status: string): boolean {
  const trimmed = status.trim()
  return trimmed === `` || trimmed.toLowerCase() === `ok`
}

/** Fold a `rate_limit` event; `null` CLEARS the slot — for an empty/`ok`
 *  status (the agent is not limited any more) AND for an unusable payload:
 *  a stale "rate limited" banner beside a live run is the worse error. */
export function parseRateLimit(event: unknown): SessionRateLimitState | null {
  if (!isEventRecord(event)) return null
  const status = event.status
  if (typeof status !== `string` || rateLimitClears(status)) return null
  const state: SessionRateLimitState = { status: status.trim() }
  const resetsAt = event.resetsAt
  if (typeof resetsAt === `number` && Number.isFinite(resetsAt) && resetsAt >= 0) {
    state.resetsAt = resetsAt
  }
  const message = event.message
  if (typeof message === `string` && message.trim()) state.message = message.trim()
  return state
}

/** The composer's ONE chip (EXP-772): the session mode. */
export interface ConfigChip {
  /** The literal `mode` — the chip row has a single member now. */
  id: string
  /** Its leading label ("Mode"). */
  label: string
  /** The mode id in force. */
  value: string
  /** What it currently reads ("Plan", CONFIG_DEFAULT_VALUE_LABEL). */
  valueLabel: string
  /** Every mode the run advertised; one entry = read-only. */
  values: SessionConfigValue[]
}

/** EXP-772: the mode chip, or `null` when the run advertises no modes (codex
 *  advertises none, so its composer draws nothing). Model, effort and the
 *  other option pickers left the mid-session UI entirely. */
export function modeChip(
  config: SessionConfigState | null | undefined
): ConfigChip | null {
  if (!config || config.modes.length === 0) return null
  const current = config.currentMode ?? ``
  const mode = config.modes.find((m) => m.id === current)
  return {
    id: `mode`,
    label: CONFIG_MODE_LABEL,
    value: current,
    valueLabel: mode?.label ?? (current || CONFIG_DEFAULT_VALUE_LABEL),
    values: config.modes.map((m) => ({ id: m.id, label: m.label })),
  }
}

/** EXP-847: the session header's READ-ONLY plan chip — the mode chip's label
 *  while the run is in PLAN mode, null otherwise. Never a control: EXP-790
 *  keeps mode a launch-time choice, so this only makes the state visible, and
 *  an approved `ExitPlanMode` clears `currentMode` and the chip with it. */
export function planModeChipLabel(
  config: SessionConfigState | null | undefined
): string | null {
  const chip = modeChip(config)
  return chip && chip.value === `plan` ? chip.valueLabel : null
}

/** EXP-772: the plan/build PAIR — exactly two modes, one of them `plan`.
 *  That shape (claude) draws a "Plan" switch instead of a two-value chip;
 *  anything else falls back to the chip. `null` when the run is not that
 *  shape. */
export interface PlanModeToggle {
  /** The mode to switch to when turning plan ON. */
  planId: string
  /** The mode to switch back to when turning it OFF. */
  buildId: string
  /** Plan mode is in force right now. */
  active: boolean
}

export const PLAN_MODE_ID = `plan`

export function planModeToggle(
  config: SessionConfigState | null | undefined
): PlanModeToggle | null {
  if (!config || config.modes.length !== 2) return null
  const plan = config.modes.find((m) => m.id === PLAN_MODE_ID)
  const build = config.modes.find((m) => m.id !== PLAN_MODE_ID)
  if (!plan || !build) return null
  return {
    planId: plan.id,
    buildId: build.id,
    active: config.currentMode === plan.id,
  }
}

// ── EXP-788: the composer answers the pending card ──────────────────────────

/** The option key a free-text answer rides when the card offers no free-text
 *  row of its own — the desktop mapper's `FREE_TEXT_KEY`: never a value, the
 *  typed reply is `answer.text`. */
export const FREE_TEXT_KEY = `text`

export interface AnswerableOption {
  key: string
  label: string
  freeText?: boolean
}

export interface AnswerableCard extends QuestionLike {
  options: AnswerableOption[]
  multiSelect: boolean
}

/** The ONE card the composer's free text answers: the newest active card
 *  (`activeQuestionIds`) whose answer is not already in flight — plan or
 *  question alike. Null = the composer sends an ordinary message. */
export function pendingAnswerable<
  T extends { id: number; kind: string; questionId?: string },
>(
  feed: readonly T[],
  activeIds: ReadonlySet<number>,
  states: AnswerStates
): Extract<T, { kind: `question` }> | null {
  for (let i = feed.length - 1; i >= 0; i--) {
    const item = feed[i]
    if (item.kind !== `question` || !activeIds.has(item.id)) continue
    if (isAnswerLocked(states[answerKey(item)])) continue
    return item as Extract<T, { kind: `question` }>
  }
  return null
}

/** EXP-820: does a card wait on THIS viewer, on the tab it is looking at?
 *  True is what HIDES the composer (the card is the whole input). Narrower
 *  than `activeQuestionIds` on purpose:
 *  - an id-less card (a desktop too old to publish wire ids) is unanswerable
 *    here, so it cannot take the composer away;
 *  - only the cards the visible tab RENDERS count — a subagent tab shows that
 *    agent's stream alone, so a card scoped elsewhere hides nothing there
 *    (the same `subagentIdOf` scoping `groupFeedRows` uses);
 *  - an answer whose ack timed out (`error`) re-enables the card but frees
 *    the composer: a `question_resolved` lost in a relay drop must not hide
 *    it for good.
 *  A locked card (`sending`/`acked`) stays pending — its resolution is due. */
export function hasPendingCard<
  T extends { id: number; kind: string; questionId?: string; subagentId?: string },
>(
  feed: readonly T[],
  activeIds: ReadonlySet<number>,
  states: AnswerStates,
  activeAgent: string | null
): boolean {
  return feed.some(
    (item) =>
      item.kind === `question` &&
      activeIds.has(item.id) &&
      item.questionId !== undefined &&
      subagentIdOf(item) === activeAgent &&
      states[answerKey(item)]?.status !== `error`
  )
}

/** What a typed reply to the pending card sends. A plan card has no free
 *  answer of its own: the reply picks "No, keep planning" (the LAST option,
 *  whose description says it sends the next message back to planning) and
 *  the text follows as that next message (`followUp`). A question rides its
 *  own free-text row when it has one, else the mapper's `text` key, with the
 *  reply as `answer.text`. Null when the card cannot take a typed reply. */
export interface FreeAnswer {
  keys: string[]
  labels: string[]
  text?: string
  /** Sent as an ordinary message right after the answer frame. */
  followUp?: string
}

export function freeAnswerFor(
  item: AnswerableCard,
  text: string
): FreeAnswer | null {
  const trimmed = text.trim()
  if (trimmed.length === 0) return null
  if (item.planMode === true) {
    if (item.options.length < 2) return null
    const reject = item.options[item.options.length - 1]
    return { keys: [reject.key], labels: [reject.label], followUp: trimmed }
  }
  const row = item.options.find((option) => option.freeText === true)
  return { keys: [row?.key ?? FREE_TEXT_KEY], labels: [trimmed], text: trimmed }
}

/** The number chip an option wears — `1`..`9` for the first nine, none past
 *  that (there is no key for a tenth). */
export const HOTKEY_MAX = 9

export function optionHotkey(index: number): string | null {
  return index >= 0 && index < HOTKEY_MAX ? String(index + 1) : null
}

/** The option a digit key selects, or null (`0`, a letter, past the end). */
export function optionForHotkey<T>(options: readonly T[], key: string): T | null {
  if (!/^[1-9]$/.test(key)) return null
  return options[Number(key) - 1] ?? null
}

// EXP-820: the free answer is typed INSIDE the card — a question's free-text
// row and a plan's reject row open an inline field in place; the composer is
// hidden while a card is pending. Copy hand-mirrored ×4 (desktop
// `steer_viewer`, iOS `QuestionCard`, Android `QuestionCard`).
/** The inline field under a question's "Type something." row. */
export const FREE_TEXT_PLACEHOLDER = `Type your answer…`
/** The inline field under a plan's "No, keep planning" row. */
export const PLAN_FEEDBACK_PLACEHOLDER = `Tell the agent what to change…`
/** The way back from an answered step being revisited to the step the ask
 *  is actually on. */
export const BACK_TO_CURRENT_STEP = `Back to current step`

/** EXP-820: whether picking `option` on `item` opens the inline field instead
 *  of answering at once — a question's free-text row, or a plan's reject
 *  (its LAST option, the one whose next message goes back to planning). */
export function opensInlineField(
  item: { planMode?: boolean; options: readonly { freeText?: boolean }[] },
  index: number
): boolean {
  const option = item.options[index]
  if (!option) return false
  if (option.freeText === true) return true
  return (
    item.planMode === true &&
    item.options.length >= 2 &&
    index === item.options.length - 1
  )
}

// ── EXP-846: the Exponential MCP tool row ───────────────────────────────────

/** One `expToolDisplay` row off the contract. */
export interface ExpToolDisplay {
  name: string
  progressive: string
  done: string
  subjectKey: string
  result: string
}

/** EXP-846: the Exponential MCP tool a call NAMES, or null for anything else.
 *  Mirrors the engine's `exp_tool_row` (`crates/engine/src/mapper.rs`): the
 *  contract PREFIX has to sit right in front of a known row name, whatever
 *  namespace an adapter put in front of THAT
 *  (`mcp__exponential__exponential_issues_create` on claude, the bare
 *  `exponential_issues_create` elsewhere) — which is what keeps another MCP
 *  server's `issues_create` out. */
export function expToolDisplay(
  name: string | null | undefined
): ExpToolDisplay | null {
  if (!name) return null
  const trimmed = name.trim()
  const prefix = contract.expToolDisplay.prefix
  for (const row of contract.expToolDisplay.tools) {
    if (trimmed.length <= row.name.length) continue
    if (!trimmed.endsWith(row.name)) continue
    if (trimmed.slice(0, trimmed.length - row.name.length).endsWith(prefix)) {
      return row as ExpToolDisplay
    }
  }
  return null
}

/** EXP-846: what an Exponential tool row READS — the contract's progressive
 *  caption while the call is in flight, its done caption once it settled. */
export function expToolCaption(
  display: ExpToolDisplay,
  settled: boolean
): string {
  return settled ? display.done : display.progressive
}

// ── EXP-785: the collapsed tool group's caption ─────────────────────────────

/** The `toolGroupSummary` input for a run of tool rows: a row from a
 *  pre-EXP-785 publisher (no kind) counts as `other`. */
export function toolGroupCaption(
  items: readonly {
    toolKind?: ToolKind
    detail?: string
    failed?: boolean
  }[]
): string {
  return toolGroupSummary(
    items.map((item) => ({
      kind: item.toolKind ?? `other`,
      detail: item.detail ?? null,
      failed: item.failed === true,
    }))
  )
}

// ── EXP-786: the per-call diff ──────────────────────────────────────────────

/** A publisher-cut diff ends in ONE metadata line saying how much it dropped
 *  (`\ 120 more lines truncated`). Split it off: the diff proper renders as a
 *  diff, the note as a muted footer. */
const DIFF_TRUNCATION_LINE = /(?:^|\n)\\ (\d+) more lines? truncated\s*$/

export function splitTruncatedDiff(diff: string): {
  diff: string
  truncated: number | null
} {
  const match = DIFF_TRUNCATION_LINE.exec(diff)
  if (!match) return { diff, truncated: null }
  return {
    diff: diff.slice(0, match.index),
    truncated: Number(match[1]),
  }
}

export function diffTruncationNote(lines: number): string {
  return `${lines} more line${lines === 1 ? `` : `s`} truncated`
}

// ── EXP-784: the rate-limit banner ──────────────────────────────────────────

/** `resetsAt` is unix MS on the wire; a publisher that sent SECONDS (any
 *  value that would land before 1973 read as ms) is scaled up. */
export function rateLimitResetsAtMs(resetsAt: number): number {
  return resetsAt < 1e11 ? resetsAt * 1000 : resetsAt
}

/** EXP-818: whether a rate-limit report is a WALL worth a banner. Claude
 *  files `allowed_warning` on every turn past ~75% of a window while it keeps
 *  working — the Usage sheet already shows that percentage, and a "rate
 *  limited" banner over a run that is visibly working was wrong. Only
 *  `rejected`, or a notice the agent itself wrote, is a banner. Desktop
 *  `rate_limit_is_wall` twin. */
export function rateLimitIsWall(state: SessionRateLimitState): boolean {
  return state.status.trim() === `rejected` || Boolean(state.message?.trim())
}

/** EXP-831: how long past its `resetsAt` a wall still renders. The engine
 *  clears the slot on the run's next activity or its next rate-limit event;
 *  until one of those arrives (and on a journal replayed after the fact) the
 *  clock is the only thing that can drop a banner whose reset has come and
 *  gone. One minute covers clock skew between the agent's stamp and ours. */
export const RATE_LIMIT_EXPIRY_GRACE_MS = 60_000

/** EXP-831: whether a wall's reset time has passed (by more than the grace).
 *  Byte-mirrored ×4 (desktop `steer::rate_limit_expired`, iOS
 *  `AgentFeed.rateLimitExpired`, Android `rateLimitExpired`). A wall with no
 *  reset time never expires by the clock. */
export function rateLimitExpired(state: SessionRateLimitState, now: Date): boolean {
  if (state.resetsAt === undefined) return false
  return now.getTime() - rateLimitResetsAtMs(state.resetsAt) > RATE_LIMIT_EXPIRY_GRACE_MS
}

/** The banner's two strings: the agent's own message (else `Rate limit
 *  reached`) and `resets in 2h 10m` (EXP-818: relative, the usage cards'
 *  countdown — a clock reading `00:00` looked like a zero) when a reset is
 *  known. `null` when the report is not a wall (`rateLimitIsWall`) or the
 *  wall's reset is behind us (`rateLimitExpired`, EXP-831 — a banner that
 *  outlived its own reset over a visibly working run). */
export function rateLimitBanner(
  state: SessionRateLimitState,
  now: Date = new Date()
): { text: string; resets: string | null } | null {
  if (!rateLimitIsWall(state) || rateLimitExpired(state, now)) return null
  const text = state.message ?? `Rate limit reached`
  const resets =
    state.resetsAt === undefined
      ? null
      : formatResetCountdown(new Date(rateLimitResetsAtMs(state.resetsAt)).toISOString(), now)
  return { text, resets }
}


/** EXP-848: the agent's TURN state — the fifth latest-wins slot (`journal.rs`,
 *  `hub.ts` LATEST_WINS_KINDS, iOS/Android `AgentFeed`, desktop `feed.rs`).
 *  `started` = the agent is executing a turn; `ended` = the turn is over
 *  (end_turn, a cancel, a prompt error). Every client DEFAULTS to `ended`
 *  before any event arrives, so a run never pulses on arrival. */
export type TurnState = (typeof contract.turnState.values)[number]

/** Fold a `turn` event. An unreadable payload keeps the previous slot (the
 *  `config_state` rule): blanking to idle mid-turn would stop the working
 *  indicator over an agent that is still thinking. */
export function parseTurnState(event: unknown): TurnState | null {
  if (!isEventRecord(event)) return null
  const state = event.state
  if (typeof state !== `string`) return null
  const trimmed = state.trim()
  return (contract.turnState.values as readonly string[]).includes(trimmed)
    ? (trimmed as TurnState)
    : null
}

/** EXP-846: the preview the engine distils from an Exponential MCP tool's JSON
 *  result (`expToolDisplay`), folded onto the `tool` row by `tool_update`.
 *  Every field is optional — a result that carried none yields null. Phase 1
 *  PLUMBS it (wire → reducer → row); the custom rendering is a later phase. */
export interface ExpToolPreview {
  id?: string
  identifier?: string
  title?: string
  url?: string
  count?: number
  status?: string
}

/** The engine caps every preview string; the client re-clamps because the
 *  wire is a device's word, not ours. */
const PREVIEW_FIELD_MAX = 200

export function parseToolPreview(value: unknown): ExpToolPreview | null {
  if (!isEventRecord(value)) return null
  const out: ExpToolPreview = {}
  for (const key of [`id`, `identifier`, `title`, `url`, `status`] as const) {
    const raw = value[key]
    if (typeof raw !== `string`) continue
    const trimmed = raw.trim()
    if (trimmed) out[key] = trimmed.slice(0, PREVIEW_FIELD_MAX)
  }
  const count = value.count
  if (typeof count === `number` && Number.isFinite(count) && count >= 0) {
    out.count = Math.round(count)
  }
  return Object.keys(out).length > 0 ? out : null
}

/** EXP-848: THE "the agent is working right now" predicate — one copy per
 *  client (desktop `feed.rs`, iOS + Android `AgentFeed`), driving both the
 *  "Working…" footer and the composer's Stop glyph.
 *
 *  `running` alone is NOT it: a live run between turns is idle, which is why
 *  the turn slot exists (default `ended`, so nothing pulses before the first
 *  edge). The negatives are the four states that each own their own UI:
 *  a trailing question/plan, the synced `needs_input` flag, the usage wall
 *  (`blocked`) and a compaction strip. */
export function sessionIsWorking(input: {
  live: boolean
  sessionEnded: boolean
  turnState: TurnState
  awaitingInput: boolean
  needsInput: boolean
  blocked: boolean
  compacting: boolean
}): boolean {
  return (
    input.live &&
    !input.sessionEnded &&
    input.turnState === `started` &&
    !input.awaitingInput &&
    !input.needsInput &&
    !input.blocked &&
    !input.compacting
  )
}
