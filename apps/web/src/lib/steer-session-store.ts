import { trpc } from "@/lib/trpc-client"
import { releaseTurnLog } from "@/lib/session-turn-events"
import {
  ackAnswer,
  answerKey,
  parseConfigState,
  parseContextLayout,
  parseRateLimit,
  parseSessionUsage,
  parseBackgroundTasks,
  parseTaskList,
  parseQueue,
  parseToolKind,
  parseToolPreview,
  parseScriptLog,
  type ScriptLog,
  parseTurnState,
  parseWorkflow,
  runningWorkflow as newestRunningWorkflow,
  applyQuestionResolved,
  beginAnswer,
  clearAnswer,
  consumeEcho,
  createActivityCoalescer,
  mergeNarrationFragment,
  failAnswer,
  isAnswerLocked,
  pushEcho,
  takeQueuedEchoes,
  resumesAfterCompaction,
  spliceBeforeQuestion,
  upsertQuestion,
  ANSWER_ACK_TIMEOUT_MS,
  COMPACTION_TIMEOUT_MS,
  feedItemBytes,
  trimFeed,
  HISTORY_PAGE_LIMIT,
  type AnswerStates,
  stampBackgroundStarts,
  type BackgroundTask,
  type TaskListEntry,
  type EchoEntry,
  type QueuedMessage,
  type ContextSegment,
  type SessionConfigState,
  type SessionRateLimitState,
  type SessionUsageState,
  type ExpToolPreview,
  type ToolKind,
  type TurnState,
  type WorkflowState,
} from "@/lib/agent-feed"
import {
  markPendingImageUploaded,
  stagePendingImages,
  type PendingAttachment,
} from "@/lib/pending-images"
import type { SessionStatus } from "@exp/db-schema/domain"
import { currentSteerCarrier } from "@/lib/enet/steer-bridge"
import {
  STEER_ENET_STALE_MS,
  steerOfflineRedial,
} from "@exp/domain-contract/steer-enet"

import { CLOSE_ENET_LOST, openEnetSteer } from "@/lib/enet/steer-enet-viewer"

// EXP-621: the viewer connection to the session's feed, lifted OUT of
// AgentSessionView into a module-level per-session store (the pattern of
// lib/collections.ts). The socket, feed, phase, answer state AND the
// composer draft all live here, so leaving the session page, switching dock
// tabs or navigating around the app detaches the VIEW without dropping the
// CONNECTION — reopening renders the retained feed instantly, no
// "Connecting…" phase and no full activity replay. VAPP-43: the feed rides
// eNet's chat channel to the device itself (lib/enet/steer-enet-viewer.ts,
// fixtures/steer-enet.json); the steer relay carries no session content.

// ── Wire protocol (the steer frames of apps/steer-relay/src/protocol.ts, ─────
// ── carried over eNet) ───────────────────────────────────────────────────────

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
const INPUT_CHUNK_CHARS = 4096
/** Redial backoff while the desktop's publisher is still starting (or its
 *  device is unreachable over eNet): 3s doubling to 30s, so many waiting
 *  viewers never redial in lockstep. */
const STARTING_RETRY_BASE_MS = 3_000
const STARTING_RETRY_MAX_MS = 30_000
/** A disposal grace once a store is neither kept by the dock strip's reaper
 *  nor subscribed — long enough to survive transient empty live-query results
 *  and route remounts, which must never kill a background socket. */
const RETAIN_GRACE_MS = 60_000
/** A store whose session ENDED and lost its last subscriber lingers briefly
 *  so a quick re-open still shows the tail, then self-disposes. */
const ENDED_GRACE_MS = 5_000
/** EXP-625: a joined feed ALWAYS answers the store's join (the replay that
 *  proved it), so silence after the join means
 *  a dead socket: one that opened but never delivers a frame. Close it so
 *  the normal onclose path runs and the user gets a Reconnect affordance
 *  instead of an eternal "Connecting…". */
const JOIN_ACK_TIMEOUT_MS = 15_000
/** EXP-648: the device sends every joined viewer a `keepalive` frame every
 *  `keepaliveMs` (fixtures/steer-enet.json), so three of those missing on a
 *  nominally live socket means the socket is dead — one the OS killed under
 *  a suspended tab without ever delivering a close frame — not that the
 *  agent is quiet (an agent parked on a question or plan approval sends
 *  nothing for minutes). A wakeup kick redials such a socket silently under
 *  the `live` phase. Mirrors Android `liveStaleMs` / iOS `liveStaleSeconds`. */
const LIVE_STALE_MS = 45_000
/** EXP-656/EXP-751: a join replay is STAGED, not applied — an `activity_reset`
 *  opens a buffer instead of wiping the feed, and the whole replay swaps in as
 *  ONE commit when the relay's `activity_synced` marker arrives. These bound
 *  the fallback for a publisher-driven republish that carries no marker:
 *  the replay arrives as one burst, so 400ms of silence means it is over, and
 *  a stalled republish commits what it has at the cap rather than holding the
 *  buffer. Android `SteerTimings.replayQuietMs`/`replayMaxMs`, iOS
 *  `replayQuietSeconds`/`replayMaxSeconds` — move all three in lockstep. */
export const REPLAY_QUIET_MS = 400
export const REPLAY_MAX_MS = 3_000

/** Equal jitter (half fixed, half random) — desynchronizes viewers that
 *  started waiting together while keeping a floor on the delay. */
function startingRetryDelay(retries: number): number {
  const capped = Math.min(
    STARTING_RETRY_BASE_MS * 2 ** retries,
    STARTING_RETRY_MAX_MS
  )
  return capped / 2 + Math.random() * (capped / 2)
}

export interface QuestionOption {
  label: string
  /** The option token the `answer` frame carries back — the desktop maps it
   *  onto its own picker. */
  key: string
  /** Claude's per-option blurb (protocol v2), rendered under the label. */
  description?: string
  /** EXP-513: claude's synthetic free-text row ("Type something.") —
   *  selecting it reveals an inline input and the typed reply rides the
   *  answer frame's `text`. */
  freeText?: boolean
}

export type ActivityEvent =
  // `beforeQuestionId` (EXP-483) anchors late-flushed prose above the
  // already-published question card it was written before.
  // `messageId` (EXP-772) is the ACP coalescer's flush key: consecutive
  // narration events that share one are FRAGMENTS of a single assistant
  // message and merge into one bubble. `subagentId` (EXP-773) scopes the
  // prose to a subagent, which hides it from the main feed.
  | {
      kind: `narration`
      text: string
      beforeQuestionId?: string
      messageId?: string
      subagentId?: string
      at?: number
    }
  // `subagentId` (protocol v2) nests the call under its subagent group.
  // EXP-785: `id` is the ACP tool-call id a later `tool_update` folds in by;
  // `toolKind` is ACP's kind bucket (the wire key is never `kind`).
  | {
      kind: `tool`
      name: string
      detail?: string
      id?: string
      toolKind?: string
      subagentId?: string
      at?: number
    }
  // EXP-785/786/895: a call settled, an edit's per-call diff and an execute
  // call's output — a LOG row on the wire that folds INTO the tool row with
  // that `id`, never a row of its own; an id this feed does not hold is
  // dropped.
  | {
      kind: `tool_update`
      id: string
      status?: `completed` | `failed`
      diff?: string
      /** EXP-895: what an `execute` call printed, published once on the settle
       *  and tail-cut by the publisher. */
      output?: string
      /** EXP-846: the Exponential MCP tool result the engine distilled — only
       *  ever present for an `exponential_*` call. */
      preview?: ExpToolPreview
      /** EXP-1279: a code-mode `exec` call's log (the calls it made, their
       *  total, its wall time) — only ever on a `script` call's settle. */
      script?: unknown
      at?: number
    }
  | { kind: `diff`; diff: string; at?: number }
  // EXP-78 (member-only on the relay): a human turn from the transcript…
  // `subagentId` (EXP-773): a turn addressed to a subagent, shown in that
  // subagent's view only.
  | { kind: `user_message`; text: string; subagentId?: string; at?: number }
  // …and an interactive question (AskUserQuestion / plan approval).
  // `planMode` marks an ExitPlanMode plan-approval picker (EXP-97) — absent
  // on generic questions.
  // Protocol v2 (EXP-249) adds the identity fields: `id` makes the card
  // answerable through the semantic `answer` frame and lets a re-emission
  // replace the card in place; `askId` + `index`/`total` group a
  // multi-question ask into one stepper (an `askId` event WITHOUT `index` is
  // the ask's final review/submit step).
  | {
      kind: `question`
      text: string
      options: QuestionOption[]
      multiSelect?: boolean
      planMode?: boolean
      id: string
      askId?: string
      index?: number
      total?: number
      header?: string
      at?: number
    }
  // Resolution of a question (by id, else the whole ask), the desktop's
  // confirmation that an answer was injected, a subagent's lifecycle, and an
  // informational permission prompt — all protocol v2.
  | {
      kind: `question_resolved`
      id?: string
      askId?: string
      answers?: string[]
      dismissed?: boolean
      at?: number
    }
  | { kind: `answer_ack`; id: string; askId?: string; at?: number }
  | {
      kind: `subagent`
      id: string
      agentType: string
      /** EXP-850 §3/§4: the workflow card this agent belongs to (the spawning
       *  `Workflow` call's id). Its edges nest under that card; EXP-1225: its
       *  rows are its own tab's. Absent on an ordinary subagent. */
      workflowId?: string
      /** EXP-847: the spawning Agent tool call's `description` (else its
       *  `name`) — what the chips and the top bar say; `agentType` stays the
       *  secondary caption. Absent on older publishers. */
      title?: string
      /** EXP-856: `duplicate` is a WARNING edge (a second copy of an agent
       *  that is still running), never a lifecycle state — the agent's own
       *  `started`/`completed` edges keep flowing under the same id. An older
       *  client that does not know the value ignores the row. */
      status: `started` | `completed` | `duplicate`
      detail?: string
      // EXP-748: the tool calls the publisher attributed to the subagent,
      // stamped on the COMPLETED edge. Authoritative over the tool rows the
      // feed can see — a replay buffer evicts subagent tool rows first, so
      // the visible ones undercount. Absent on older publishers.
      toolCalls?: number
      at?: number
    }
  | { kind: `permission`; tool: string; detail?: string; at?: number }
  // EXP-1324: the agent's reasoning, a Trace line ("Thinking" → "Thought for
  // 5s"). `started` opens the row, `ended` (same `id`) settles it with the
  // duration; no text rides the wire. A row whose `ended` never came reads
  // settled, without a duration, once the turn ends.
  | {
      kind: `thought`
      id: string
      state: `started` | `ended`
      durationMs?: number
      subagentId?: string
      at?: number
    }
  // EXP-724: the agent is folding its context. `started` opens the
  // indeterminate strip, `ended` closes it AND leaves a marker row behind.
  // Codex publishes no start marker for its automatic compaction, so a BARE
  // `ended` is normal and still writes the marker.
  | {
      kind: `compaction`
      phase: `started` | `ended`
      trigger?: `manual` | `auto`
      at?: number
    }
  // EXP-746 (ACP engine): the agent's live configuration and its context
  // meter. Both are LATEST-WINS STATE — the newest event replaces the
  // previous one in a snapshot SLOT and never appends a feed row (the `diff`
  // precedent). The relay replays its latest of each right after the log.
  // EXP-877: `options` rides the wire again — the fold keeps the list so the
  // composer footer can read the model in force off it (`sessionModel`).
  | {
      kind: `config_state`
      currentMode?: string
      options?: SessionConfigState[`options`]
      modes?: SessionConfigState[`modes`]
      commands?: SessionConfigState[`commands`]
      at?: number
    }
  | {
      kind: `usage`
      contextUsed: number
      contextSize: number
      costUsd?: number
      at?: number
    }
  // EXP-1051: where the window `usage` measures actually WENT — the layers the
  // launcher put in before the first turn. A latest-wins slot beside `usage`
  // (the device publishes one per conversation, again after a `/clear`,
  // nothing on a compaction); `conversation`/`free` are derived from `usage`
  // by `lib/context-layout.ts` and are never on the wire.
  | {
      kind: `context_layout`
      segments: {
        key: string
        tokens: number
        source: `measured` | `estimated`
        detail?: string
      }[]
      at?: number
    }
  // A transient API failure (timeout, dropped connection, rejected request):
  // an INLINE transcript row like narration, never a latest-wins slot and
  // never a rate-limit wall. Mirrored ×4.
  | {
      kind: `api_error`
      message: string
      errorType?: string
      subagentId?: string
      at?: number
    }
  // EXP-784: the rate-limit window, the fourth latest-wins slot. An empty or
  // `ok` status CLEARS it.
  | {
      kind: `rate_limit`
      status: string
      resetsAt?: number
      message?: string
      at?: number
    }
  // EXP-848: the agent's turn edges — the FIFTH latest-wins slot, never a feed
  // row. Replayed (journal + history_chunk) so a late joiner learns the state;
  // absent means idle, which is what every client assumes by default.
  | {
      kind: `turn`
      state: TurnState
      /** EXP-850 §5: when this turn began (ms epoch) — on BOTH states. It
       *  picks the working caption's verb and measures its duration; a NEW
       *  value resets the token counter. */
      startedAt?: number
      /** Output tokens produced in this turn so far, republished at most
       *  every `steerWorking.tokenTickMs`. A republish that omits it never
       *  blanks what the slot already learned. */
      tokens?: number
      at?: number
    }
  // EXP-850 §2: everything the CLI is running in the background — the FULL
  // current list on every frame, so an empty array closes the strip. The
  // sixth latest-wins slot; never a feed row.
  | {
      kind: `background_tasks`
      tasks?: unknown
      at?: number
    }
  // EXP-927: the agent's OWN task list (TodoWrite / Task* / codex plan), in
  // full on every frame — the bottom strip's first block. A latest-wins
  // slot; never a feed row.
  | {
      kind: `task_list`
      entries?: unknown
      at?: number
    }
  // EXP-861: the messages the DEVICE holds until the current turn (or
  // compaction) ends — the FULL queue on every frame, so an empty array
  // clears the strip. A latest-wins slot; never a feed row.
  | {
      kind: `queue`
      messages?: unknown
      at?: number
    }
  // EXP-850 §3: a workflow's live card — latest-wins PER `id` (the spawning
  // `Workflow` tool call's id), patched onto the tool row with that id.
  | {
      kind: `workflow`
      id: string
      name: string
      description?: string
      status?: string
      phases?: unknown
      agents?: unknown
      summary?: string
      at?: number
    }

type ServerFrame =
  // EXP-783: `seq` is the publisher's own monotonic index, echoed by the
  // relay. Every publisher numbers its frames, so every row is addressable.
  | { t: `activity`; event: ActivityEvent; seq: number }
  // Protocol v2: "clear your feed now" — sent before every join replay and
  // whenever the desktop re-publishes its full history.
  | { t: `activity_reset` }
  // EXP-656: the relay's end-of-replay marker, sent to the joining viewer
  // right after its join replay (never after a publisher-driven reset).
  // EXP-783: it now names the SPAN it replayed. Everything this client holds
  // BELOW `firstSeq` is a prefix the replay does not restate, so it is KEPT;
  // `truncated` says the relay's log is a TAIL, so the pages under it have to
  // be asked for from the device (`history_page`).
  | {
      t: `activity_synced`
      firstSeq: number
      lastSeq: number
      truncated?: boolean
    }
  // EXP-783: one page of OLDER transcript, answering this viewer's
  // `history_page`. PREPENDED, never appended. `seqs` runs parallel to
  // `events`: the page is trimmed against what is already on screen by it.
  | {
      t: `history_chunk`
      requestId: string
      events: ActivityEvent[]
      seqs: number[]
      done: boolean
    }
  // EXP-648: the device's liveness beat to joined viewers. Carries nothing;
  // its only effect is the `lastFrameAt` stamp taken above the switch.
  | { t: `keepalive` }
  | { t: `bye`; outcome?: string }
  | { t: `error`; code: string; message?: string }
  | { t: string }

function parseServerFrame(raw: string): ServerFrame | null {
  try {
    const json = JSON.parse(raw) as unknown
    if (!json || typeof json !== `object`) return null
    if (typeof (json as { t?: unknown }).t !== `string`) return null
    return json as ServerFrame
  } catch {
    return null
  }
}

// ── Viewer state ─────────────────────────────────────────────────────────────

export type ViewerPhase =
  | { kind: `idle` }
  | { kind: `connecting` }
  // no_such_session while the synced row still says running — the desktop is
  // still dialing its publisher socket; the store auto-redials with jittered
  // backoff (3s → 30s).
  | { kind: `starting` }
  | { kind: `live` }
  // The session ended (the device's `bye`, or no feed for an ended row:
  // VAPP-43, the device-offline copy).
  | { kind: `ended`; detail?: string }
  // Unexpected feed loss — offer a manual Reconnect (a fresh dial).
  // `terminal` (EXP-648) marks a "no" a retry cannot turn into a "yes"
  // (the device refusing this viewer). The wakeup kicks leave those
  // alone instead of redialing on every tab switch; the explicit Reconnect
  // button still tries.
  | { kind: `closed`; detail?: string; terminal?: boolean }

/** EXP-783: the publisher's monotonic index for the event behind a row. The
 *  only monotonic anchor on the wire — it is what lets a join replay be
 *  spliced onto a transcript prefix already on screen, and what an older-page
 *  request is addressed relative to. A row this client made up (the echo of a
 *  message it just sent) inherits the newest sequence it has seen, so the feed
 *  stays monotonic. */
interface FeedSeq {
  seq: number
}

export type FeedItem = FeedSeq &
  (
  | {
      id: number
      kind: `narration`
      text: string
      /** EXP-772: the ACP message this bubble belongs to — later fragments
       *  with the same id append to it instead of opening a row. */
      messageId?: string
      /** EXP-773: rendered inside that subagent's view, not in Main. */
      subagentId?: string
    }
  // EXP-724: the quiet "Context compacted" hairline a finished compaction
  // leaves in the transcript. Carries nothing — the copy is a constant.
  | { id: number; kind: `compaction` }
  | {
      id: number
      kind: `tool`
      name: string
      detail?: string
      subagentId?: string
      /** EXP-785: the ACP tool-call id — the `tool_update` fold key. Absent
       *  on rows from a pre-EXP-785 publisher, which never settle. */
      callId?: string
      /** EXP-785: ACP's kind bucket, when the publisher sent one. */
      toolKind?: ToolKind
      /** EXP-785: a `tool_update` with a status landed — the call ENDED. */
      settled?: boolean
      /** EXP-1241: when that settle was stamped (its `at`), so a comment
       *  card can carry a time before the comment row syncs. */
      settledAt?: number
      /** EXP-785: that status was `failed` (a later `completed` clears it).
       *  Group captions list these last. */
      failed?: boolean
      /** EXP-786: the per-call unified diff an `edit` published, already cut
       *  to the contract's caps by the publisher. */
      diff?: string
      /** EXP-895: what an `execute` call PRINTED, as its settle published it
       *  — redacted and tail-cut to the contract's caps (a cut output OPENS
       *  with the `\ N more lines truncated` marker: the dropped lines were
       *  at the front). Absent until the call settles. */
      output?: string
      /** EXP-846: the Exponential MCP result preview a `tool_update` folded
       *  in (issue/PR/list…). Plumbed now, rendered in a later phase. */
      preview?: ExpToolPreview
      /** EXP-1279: the code-mode log a `script` call's settle carried — the
       *  Script card's calls; absent from an older codemode. */
      script?: ScriptLog
      /** EXP-850 §3: this call IS a workflow (`workflows.get(callId)` holds
       *  its card). The row renders as the card, never as a tool row, and it
       *  never joins a collapsed tool run. */
      workflowId?: string
    }
  | {
      id: number
      kind: `user_message`
      text: string
      subagentId?: string
      /** EXP-1245: when the relay stamped it; the turns thread slots it. */
      at?: number
    }
  | { id: number; kind: `permission`; tool: string; detail?: string }
  // EXP-1324: one reasoning stretch, its Trace line. `settled` once its
  // `ended` edge (or the turn's end) landed; `durationMs` only from the edge.
  | {
      id: number
      kind: `thought`
      thoughtId: string
      settled: boolean
      durationMs?: number
      subagentId?: string
    }
  // A transient API failure shown where it happened (never the rate-limit
  // banner); `errorType` = the API's error type when it sent one.
  | {
      id: number
      kind: `api_error`
      message: string
      errorType?: string
      subagentId?: string
    }
  | {
      id: number
      kind: `subagent`
      subagentId: string
      agentType: string
      /** EXP-847: the spawning call's description — shown instead of
       *  `agentType` wherever a subagent is named. */
      title?: string
      status: `started` | `completed` | `duplicate`
      detail?: string
      /** EXP-850 §3: the workflow card this agent belongs to. */
      workflowId?: string
      /** EXP-748: the publisher's own tool-call count for the subagent (the
       *  completed edge carries it); absent on the started edge and on older
       *  publishers. `summarizeSubagentRow` prefers it over the visible rows. */
      toolCalls?: number
      /** EXP-1324: the edge's stamp — the subagent line's duration badge. */
      at?: number
    }
  | {
      id: number
      kind: `question`
      text: string
      options: QuestionOption[]
      multiSelect: boolean
      planMode: boolean
      /** Wire identity (protocol v2): what the `answer` frame names, and
       *  what a re-emission replaces the card by. */
      questionId: string
      askId?: string
      index?: number
      total?: number
      header?: string
      /** Set once the question resolved — a resolved card renders `answer`
       *  (or "Dismissed") and is never active again. */
      resolved?: boolean
      answer?: string
      dismissed?: boolean
    }
  )

export type QuestionItem = Extract<FeedItem, { kind: `question` }>
export type ToolItem = Extract<FeedItem, { kind: `tool` }>

/** `Omit` that distributes over the FeedItem union (plain `Omit` collapses a
 *  union to its common keys, losing the per-kind fields). `id` and `seq` are
 *  both stamped by `append`, so a caller never carries them. */
type NewFeedItem = FeedItem extends infer T
  ? T extends FeedItem
    ? Omit<T, `id` | `seq`>
    : never
  : never

/** A composer image or file pending upload — the draft survives disconnects
 *  and view unmounts, so `uploadedId` also persists a mid-batch upload across
 *  them and a retry only uploads the rest. */
export type PendingSteerImage = PendingAttachment

/** EXP-724: an in-flight compaction — the strip's whole state. */
export interface CompactionState {
  /** When the fold started (the event's `at`, else arrival time) — the
   *  backstop deadline is measured from here, so a REPLAYED `started` from
   *  minutes ago expires immediately instead of restarting the clock. */
  startedAt: number
  /** `manual` = this viewer (or another) asked for it; absent on older
   *  desktops. */
  trigger?: `manual` | `auto`
}

export interface SteerSessionSnapshot {
  phase: ViewerPhase
  feed: FeedItem[]
  latestDiff: string | null
  /** Non-null while the agent is compacting (EXP-724). */
  compacting: CompactionState | null
  /** EXP-746: the agent's live configuration behind the composer chips — a
   *  latest-wins SLOT, like `latestDiff`, never a feed row. Null until the
   *  engine publishes one (every PTY run stays null). */
  config: SessionConfigState | null
  /** EXP-746: the run's own context/spend meter, same latest-wins rule. */
  usage: SessionUsageState | null
  /** EXP-1051: how that window is laid out — the attributed layers, in the
   *  device's order. Null until the run publishes one (every pre-EXP-1051
   *  device stays null, and the bar then reads as conversation alone). */
  contextLayout: ContextSegment[] | null
  /** EXP-784: the agent's rate-limit window, the fourth slot. Null = not
   *  limited (or cleared by an empty/`ok` status). */
  rateLimit: SessionRateLimitState | null
  /** EXP-848: the agent's turn state, the fifth slot — `ended` (idle) until a
   *  `turn` event says otherwise, so nothing pulses by default. THE input to
   *  `sessionIsWorking`. */
  turnState: TurnState
  /** EXP-850 §5: when the current turn began (ms epoch), or null — the
   *  working caption's verb and its clock. */
  turnStartedAt: number | null
  /** Output tokens this turn has produced so far, or null while unknown. */
  turnTokens: number | null
  /** EXP-850 §2: what the CLI is running in the background — the strip above
   *  the composer. Empty = no strip. */
  backgroundTasks: BackgroundTask[]
  /** EXP-927: the agent's own task list, in order — the strip's first block.
   *  Empty = the agent keeps none. */
  taskList: TaskListEntry[]
  /** EXP-861: the messages the device is holding for the next turn — the
   *  strip above the composer. Empty = no strip. */
  queue: QueuedMessage[]
  /** EXP-850 §3: every workflow this run published, by id (the spawning
   *  `Workflow` call's id). A SIDE MAP, never feed rows: the tool row with
   *  that id renders as the card. */
  workflows: ReadonlyMap<string, WorkflowState>
  /** The newest workflow still running — the caption source (§5/§7). */
  runningWorkflow: WorkflowState | null
  answerStates: AnswerStates
  /** The socket is actually open. Distinct from the phase: a silent
   *  slow-consumer redial keeps `phase: live` while the socket is briefly
   *  down, and send affordances should dim honestly for that gap. */
  connected: boolean
  /** EXP-783: there is transcript BELOW the oldest row on screen, and this
   *  client can ask the device for it — what the transcript's "Load earlier"
   *  affordance is gated on. */
  canLoadEarlier: boolean
  /** VAPP-171: the `ended` phase is the device-offline one (no feed: its
   *  `detail` is the "{device} is offline" copy the empty column shows ×4). */
  deviceOffline: boolean
}

export interface SteerDraftSnapshot {
  text: string
  images: PendingSteerImage[]
}

export interface AddDraftImagesResult {
  /** Files refused for size (the "images up to 10 MB and files up to 50 MB"
   *  toast). */
  rejected: number
  /** Images dropped over MAX_STEER_IMAGES (the "up to N images" toast). */
  overflow: number
  /** Files dropped over MAX_STEER_FILES (the "up to N files" toast). */
  fileOverflow: number
  /** How many attachments (both kinds) joined the strip. */
  added: number
  /** How many of them are IMAGES — the composer numbers its `[Image #N]`
   *  markers from the image count it already knows (EXP-698). */
  addedImages: number
}

interface SteerStoreDeps {
  /** VAPP-33: the feed over eNet's chat channel, joined and shaped like a
   *  WebSocket; null = NO FEED (VAPP-43: there is no relay fallback). */
  openEnet: (codingSessionId: string) => Promise<WebSocket | null>
}

const defaultDeps: SteerStoreDeps = {
  openEnet: async (codingSessionId) =>
    (await openEnetSteer(codingSessionId, {
      carrier: currentSteerCarrier,
      forSession: (id) =>
        trpc.installs.forSession.query({ sessionId: id }, { context: { skipErrorToast: true } }),
    })) as unknown as WebSocket | null,
}

export interface SteerSessionStore {
  readonly sessionId: string
  subscribe(listener: () => void): () => void
  getSnapshot(): SteerSessionSnapshot
  getDraftSnapshot(): SteerDraftSnapshot
  /** Idempotent: dials only from `idle`/`closed`. A store that is already
   *  live (or mid-dial) is left alone — that is what makes reopening a
   *  session instant. */
  connect(): void
  /** Force a fresh dial (the manual Reconnect button). */
  reconnect(): void
  /** EXP-625: a wakeup nudge (tab visible again, network back, the host
   *  device came online). Acts on whether a dial is actually ALIVE, not on
   *  the phase alone: it revives a closed or provably stuck store and cuts
   *  short a `starting` backoff, and is a cheap no-op everywhere else, so
   *  callers may fire it freely. */
  kick(reason: string): void
  /** EXP-783: pull in the page of transcript BELOW the oldest row on screen.
   *  Called when the reader reaches the top of the rendered window. `false`
   *  when there is nothing to ask for (or an ask is already in flight). */
  loadEarlier(): boolean
  /** The synced row is the truth for "still running" inside the redial
   *  loops — the OWNING view feeds it; an unwatched store falls back to the
   *  device's own signals plus the dock reaper. EXP-1293: the `sessions`
   *  row's status — `queued` reads as starting, `ended` as over. */
  noteSessionStatus(status: SessionStatus): void
  /** EXP-773: the host machine's label, for the history phases' copy. */
  noteDeviceLabel(label: string | null): void
  sendMessage(text: string): boolean
  answerQuestion(
    item: QuestionItem,
    keys: string[],
    labels: string[],
    text?: string
  ): void
  /** EXP-746: switch the session mode — the composer's ONE live control
   *  since EXP-772. False = the socket is down and nothing went out. */
  setMode(id: string): boolean
  /** EXP-790: stop the agent's current turn (the composer's Stop glyph, shown
   *  while it works and the field is empty). Fire-and-forget like `setMode`:
   *  the publisher cancels the turn and the feed shows the outcome. False =
   *  the socket is down and nothing went out. */
  interrupt(): boolean
  /** EXP-1342: a Stop sent OUTSIDE this socket (`sessions.stop`, the web's
   *  Stop): the device drops every unread message with the turn, so the
   *  bar's text goes back to the draft (in order, `mergeHeldIntoDraft`) and
   *  a dropped message never renders as a row. True = text came back. */
  noteStopped(): boolean
  /** EXP-1342: hand the draft's TEXT to a composer that keeps its own field
   *  (the web `SessionComposer`) and clear it here; `` = nothing waiting. */
  takeDraftText(): string
  /** EXP-861: revoke a queued message. Fire-and-forget like `interrupt`: the
   *  device's next `queue` frame is the confirmation; the entry is dropped
   *  from the local slot right away. False = the socket is down. */
  unqueue(id: string): boolean
  /** EXP-866: drop the live rate-limit slot NOW — called when the person
   *  requests an account switch, so the wall does not outlive the run it
   *  belonged to. The device's next `rate_limit` frame re-arms it if real. */
  clearRateLimit(): void
  setDraftText(text: string): void
  addDraftImages(files: File[]): AddDraftImagesResult
  removeDraftImage(url: string): void
  setDraftImageUploaded(url: string, uploadedId: string, uploadedName?: string): void
  clearDraftAfterSend(): void
  dispose(): void
}

interface InternalSteerSessionStore extends SteerSessionStore {
  /** Registry internals (reaper bookkeeping). */
  _subscriberCount(): number
  _scheduleReap(): void
  _cancelReap(): void
}

/** EXP-861/EXP-1342: the queue's text handed back to a composer, in order:
 *  an empty draft becomes the text, a draft in progress keeps its place and
 *  the text lands after a blank line (STEER-WIRE-EXP-850 Stop rule). */
export function mergeHeldIntoDraft(draft: string, held: readonly string[]): string {
  if (held.length === 0) return draft
  const text = held.join(`\n`)
  return draft.trim() ? `${draft}\n\n${text}` : text
}

// Exported for tests; app code goes through acquireSteerSession.
export function createSteerSessionStore(
  sessionId: string,
  deps: SteerStoreDeps = defaultDeps,
  onDispose?: () => void
): InternalSteerSessionStore {
  let disposed = false
  // Bumped per dial so a superseded socket's callbacks are inert.
  let generation = 0
  let ws: WebSocket | null = null
  let retryTimer: ReturnType<typeof setTimeout> | null = null
  // Consecutive auto-redials (`starting` and slow-consumer alike) — drives
  // the backoff; a live connection resets it so the next stall starts fast.
  let retries = 0
  let sessionStatus: SessionStatus | null = null
  /** EXP-773: the host machine's label, for the device-offline copy. The
   *  owning view feeds it off the synced devices row. */
  let deviceLabel: string | null = null
  /** VAPP-169: the `ended` phase is the device-offline one (NO FEED for an
   *  ended row), which keeps redialing: the stale timer, a wakeup, the
   *  device's presence flip online (fixtures/steer-enet.json OFFLINE REDIAL). */
  let deviceOffline = false
  /** VAPP-173: a redial's feed answered while the row still read ended, so
   *  the phase went `ended` from the ROW, not from `bye`: the row flipping
   *  back to running under that open feed lifts it to live. */
  let endedByRow = false
  // EXP-625: dial liveness, so a wakeup can tell a dial that is merely young
  // from one that is stuck. The timer is generation-scoped.
  let joinAckTimer: ReturnType<typeof setTimeout> | null = null
  let dialStartedAt = 0
  let lastFrameAt = 0

  let phase: ViewerPhase = { kind: `idle` }
  let feed: FeedItem[] = []
  /** EXP-783: the running weight of `feed`, so the byte budget is an integer
   *  compare per append rather than a walk. */
  let feedBytes = 0
  /** EXP-783: the wire sequence of the event being folded in right now, so
   *  `append` can stamp it without threading it through every case. It stays
   *  on the last one seen between folds: a row this client appends itself (the
   *  echo of a message it just sent) belongs directly after it. */
  let currentSeq = 0
  let latestDiff: string | null = null
  let compacting: CompactionState | null = null
  let config: SessionConfigState | null = null
  let usage: SessionUsageState | null = null
  // EXP-1051: the context-window layout, latest-wins beside `usage`.
  let contextLayout: ContextSegment[] | null = null
  let rateLimit: SessionRateLimitState | null = null
  // EXP-848: idle until the publisher says otherwise.
  let turnState: TurnState = `ended`
  // EXP-850 §5: the turn's start and its token count — both unknown until a
  // `turn` frame carries them (codex publishes neither, which is why the
  // caption's duration/token group is optional).
  let turnStartedAt: number | null = null
  let turnTokens: number | null = null
  // EXP-850 §2/§3: the two new latest-wins slots. `workflows` is keyed per
  // workflow id, in first-appearance order (a Map preserves insertion order).
  let backgroundTasks: BackgroundTask[] = []
  // EXP-1324: when this client first saw each background task id — kept
  // across replays, so a reconnect never restarts a task's elapsed time.
  const backgroundFirstSeen = new Map<string, number>()
  // EXP-927: the agent's own task list, latest-wins whole.
  let taskList: TaskListEntry[] = []
  // EXP-861: the device-held queue, the seventh latest-wins slot.
  let queue: QueuedMessage[] = []
  let workflows = new Map<string, WorkflowState>()
  let compactionTimer: ReturnType<typeof setTimeout> | null = null
  let answerStates: AnswerStates = {}
  let connected = false
  let nextId = 0
  /** Locally-echoed sent messages awaiting their transcript-derived event. */
  const recentEchoes: EchoEntry[] = []
  /** EXP-1342: messages a Stop dropped (their text went back to the draft):
   *  a straggling `user_message` for one is skipped, never painted beside
   *  the text the person is about to resend. */
  const droppedEchoes: EchoEntry[] = []
  /** Per-card `answer_ack` deadlines (see ANSWER_ACK_TIMEOUT_MS). */
  const ackTimers = new Map<string, ReturnType<typeof setTimeout>>()
  /** EXP-751: the replay being staged, or null when nothing is staging. Holds
   *  every `activity` event since the last `activity_reset`, in arrival
   *  order; the visible feed is folded from them in one go at commit. */
  let staged: { event: ActivityEvent; seq: number }[] | null = null
  /** Messages this client sent WHILE staging: the replay predates them, so
   *  the commit re-appends whatever it did not carry back. */
  let stagedEchoes: string[] = []
  /** EXP-783: the `history_page` request in flight, if any — a chunk for any
   *  other id is not ours. */
  let historyRequest: string | null = null
  /** The relay said its replay log is a TAIL: there IS older transcript to
   *  ask the device for. */
  let historyTruncated = false
  /** A page came back with nothing new — stop asking. */
  let historyExhausted = false
  /** Numbers the request ids, so a late chunk from a superseded ask is
   *  dropped rather than prepended twice. */
  let historyRequests = 0
  /** Commit deadlines: REPLAY_QUIET_MS since the last staged frame, and
   *  REPLAY_MAX_MS since the reset. */
  let stageQuietTimer: ReturnType<typeof setTimeout> | null = null
  let stageCapTimer: ReturnType<typeof setTimeout> | null = null
  /** `activity_reset` ops enqueued but not yet applied — a keepalive that
   *  lands in that window still has to ride the queue to end the replay. */
  let queuedResets = 0
  /** True while `commitStaging` folds the replay: the replay is authoritative,
   *  so its `user_message` rows never dedupe against a local echo. */
  let foldingReplay = false

  let draftText = ``
  let draftImages: PendingSteerImage[] = []

  const listeners = new Set<() => void>()
  // Snapshots are cached and replaced per mutation — useSyncExternalStore
  // compares by Object.is, so a per-call allocation would render-loop, and
  // the split keeps keystrokes from re-rendering the feed (and frames from
  // re-rendering the composer).
  let snapshot: SteerSessionSnapshot = {
    phase,
    feed,
    latestDiff,
    compacting,
    config,
    usage,
    contextLayout,
    rateLimit,
    turnState,
    turnStartedAt,
    turnTokens,
    backgroundTasks,
    taskList,
    queue,
    workflows,
    runningWorkflow: null,
    answerStates,
    connected,
    canLoadEarlier: false,
    deviceOffline,
  }
  let draftSnapshot: SteerDraftSnapshot = { text: draftText, images: draftImages }

  const notify = () => {
    for (const listener of listeners) listener()
  }
  const commit = () => {
    snapshot = {
      phase,
      feed,
      latestDiff,
      compacting,
      config,
      usage,
      contextLayout,
      rateLimit,
      turnState,
      turnStartedAt,
      turnTokens,
      backgroundTasks,
      taskList,
      queue,
      workflows,
      runningWorkflow: newestRunningWorkflow([...workflows.values()]),
      answerStates,
      connected,
      // EXP-796: a page can only be asked for over an OPEN feed — once the
      // device dropped this viewer, the button goes.
      canLoadEarlier: historyTruncated && !historyExhausted && connected,
      deviceOffline,
    }
    notify()
  }
  const commitDraft = () => {
    draftSnapshot = { text: draftText, images: draftImages }
    notify()
  }
  /** EXP-861: the queue dies with the run (an unattended end, a kill, a
   *  crash) — the held text is handed back to the composer, in order, the
   *  way the strip's X hands one entry back, rather than lost. A non-empty
   *  draft keeps its place; the queued text lands after a blank line. */
  const restoreQueueToDraft = (): boolean => {
    if (queue.length === 0) return false
    draftText = mergeHeldIntoDraft(draftText, queue.map((entry) => entry.text))
    queue = []
    commitDraft()
    return true
  }
  /** EXP-1342: a Stop drops the unread messages — remember them so a late
   *  `user_message` for one never paints a row, then hand the text back. */
  const dropQueueOnStop = (): boolean => {
    const now = Date.now()
    for (const entry of queue) pushEcho(droppedEchoes, entry.text, now)
    return restoreQueueToDraft()
  }

  const clearAckTimer = (key: string) => {
    const timer = ackTimers.get(key)
    if (timer) {
      clearTimeout(timer)
      ackTimers.delete(key)
    }
  }

  const clearRetryTimer = () => {
    if (retryTimer) {
      clearTimeout(retryTimer)
      retryTimer = null
    }
  }

  /** EXP-625: drop the dial deadline. Every path that abandons a dial
   *  (a new dial, reconnect, dispose, the socket closing) goes through here. */
  const clearDialTimers = () => {
    if (joinAckTimer) {
      clearTimeout(joinAckTimer)
      joinAckTimer = null
    }
  }

  /** EXP-724: drop the compaction strip and its backstop. Every path that
   *  ends a fold (the `ended` marker, the agent visibly resuming, a feed
   *  reset, the session ending, disposal) goes through here. */
  const clearCompaction = () => {
    compacting = null
    if (compactionTimer) {
      clearTimeout(compactionTimer)
      compactionTimer = null
    }
  }

  /** The strip can never stick: a publisher that dies mid-fold, or an agent
   *  whose end marker is lost, expires on its own. The delay is measured from
   *  the fold's OWN start, so a `started` replayed out of the relay's log
   *  minutes later expires on the next tick instead of running a fresh 3
   *  minutes. */
  const armCompactionBackstop = (startedAt: number) => {
    if (compactionTimer) clearTimeout(compactionTimer)
    compactionTimer = setTimeout(
      () => {
        compactionTimer = null
        if (!compacting) return
        compacting = null
        commit()
      },
      Math.max(0, COMPACTION_TIMEOUT_MS - (Date.now() - startedAt))
    )
  }

  /** Returns whether the phase actually changed — a per-frame commit on an
   *  already-live store would notify (and re-render) once per replayed
   *  message, undoing the REV-33 coalescing. */
  const markLive = (): boolean => {
    retries = 0
    if (phase.kind === `live`) return false
    // EXP-796: an ended phase never lifts back to live on the same dial —
    // keepalives and pages are not a run, and the synced row stays the
    // source of truth (a redial resets the phase).
    // VAPP-169: a device-offline ended that a redial reached: the transcript
    // replaces the offline copy (a stored replay stays plain `ended`).
    if (phase.kind === `ended`) {
      if (!deviceOffline) return false
      deviceOffline = false
      endedByRow = sessionStatus === `ended`
      phase = endedByRow ? { kind: `ended` } : { kind: `live` }
      return true
    }
    phase = { kind: `live` }
    return true
  }

  /** EXP-783: the feed keeps the WHOLE run — see FEED_BYTE_CAP. Everything
   *  that grows it goes through here, so the budget has one seam. */
  const setFeed = (next: FeedItem[], added?: FeedItem[]) => {
    if (added) {
      for (const item of added) feedBytes += feedItemBytes(item)
    } else {
      feedBytes = next.reduce((sum, item) => sum + feedItemBytes(item), 0)
    }
    const trimmed = trimFeed(next, feedBytes)
    feed = trimmed.feed
    feedBytes = trimmed.bytes
  }

  const append = (item: NewFeedItem) => {
    const row = { ...item, id: nextId++, seq: currentSeq } as FeedItem
    setFeed([...feed, row], [row])
  }

  const handleActivity = (event: ActivityEvent) => {
    // EXP-724: the agent narrating, calling a tool, asking or spawning a
    // subagent proves it is working again — close a strip whose `ended`
    // marker never arrived.
    if (compacting && resumesAfterCompaction(event.kind)) clearCompaction()
    switch (event.kind) {
      case `narration`: {
        const trimmed = event.text.trim()
        if (!trimmed) return
        // EXP-483: prose from the withheld ask/plan entry flushes AFTER
        // its already-published card — splice it back above the card.
        const anchor = event.beforeQuestionId
        if (anchor !== undefined) {
          const item: FeedItem = {
            id: nextId++,
            kind: `narration`,
            text: event.text,
            messageId: event.messageId,
            subagentId: event.subagentId,
            seq: currentSeq,
          }
          setFeed(spliceBeforeQuestion(feed, anchor, item) ?? [...feed, item], [item])
          return
        }
        // EXP-772: the engine flushes ONE assistant message in several
        // narration events keyed by `messageId` — a fragment landing right
        // behind its own message appends to that bubble instead of shredding
        // the paragraph into rows.
        const merged = mergeNarrationFragment(feed, {
          messageId: event.messageId,
          text: event.text,
          subagentId: event.subagentId,
        })
        if (merged) {
          feedBytes += event.text.length
          setFeed(merged, [])
          return
        }
        append({
          kind: `narration`,
          text: event.text,
          messageId: event.messageId,
          subagentId: event.subagentId,
        })
        return
      }
      case `api_error`: {
        // An inline row only: the `rateLimit` slot is for real walls.
        if (!event.message.trim()) return
        append({
          kind: `api_error`,
          message: event.message,
          errorType: event.errorType?.trim() ? event.errorType : undefined,
          subagentId: event.subagentId,
        })
        return
      }
      case `tool`: {
        const detail = event.detail?.trim() ? event.detail : undefined
        const callId = event.id?.trim() ? event.id : undefined
        append({
          kind: `tool`,
          name: event.name,
          detail,
          subagentId: event.subagentId,
          callId,
          toolKind: parseToolKind(event.toolKind),
          // EXP-850 §3: the card may have arrived BEFORE its tool row (the
          // relay replays the workflow slots after the log), so the row is
          // stamped on creation too — the two orders end in the same feed.
          workflowId: callId && workflows.has(callId) ? callId : undefined,
        })
        return
      }
      case `tool_update`: {
        // EXP-785/786: folded INTO the newest tool row with that call id —
        // never a row of its own. An id this feed does not hold (evicted, or
        // below the window) is dropped.
        if (!event.id) return
        let at = -1
        for (let i = feed.length - 1; i >= 0; i--) {
          const item = feed[i]
          if (item.kind === `tool` && item.callId === event.id) {
            at = i
            break
          }
        }
        if (at < 0) return
        const current = feed[at] as ToolItem
        const next: ToolItem = { ...current }
        if (event.status === `completed` || event.status === `failed`) {
          next.settled = true
          next.failed = event.status === `failed`
          if (typeof event.at === `number` && Number.isFinite(event.at))
            next.settledAt = event.at
        }
        if (typeof event.diff === `string` && event.diff.trim()) next.diff = event.diff
        // EXP-895: the settle's command output. An update carrying none never
        // clears what an earlier one folded in.
        if (typeof event.output === `string` && event.output.trim())
          next.output = event.output
        // EXP-846: the Exponential-tool result preview, stored on the row.
        const preview = parseToolPreview(event.preview)
        if (preview) next.preview = preview
        const script = parseScriptLog(event.script)
        if (script) next.script = script
        feedBytes += feedItemBytes(next) - feedItemBytes(current)
        const updated = feed.slice()
        updated[at] = next
        setFeed(updated, [])
        return
      }
      case `user_message`: {
        if (!event.text.trim()) return
        // A message this client just sent was already echoed locally — skip
        // its transcript-derived twin. Never inside a replay fold: the replay
        // is authoritative and the echo FIFO was emptied when it began.
        if (!foldingReplay && consumeEcho(recentEchoes, event.text, Date.now()))
          return
        // EXP-1342: a message a Stop dropped (its text is back in the draft).
        if (!foldingReplay && consumeEcho(droppedEchoes, event.text, Date.now()))
          return
        append({
          kind: `user_message`,
          text: event.text,
          subagentId: event.subagentId,
          at:
            typeof event.at === `number` && Number.isFinite(event.at)
              ? event.at
              : undefined,
        })
        return
      }
      case `question`: {
        if (!event.text.trim() || !event.options?.length) return
        const item: Omit<QuestionItem, `id` | `seq`> = {
          kind: `question`,
          text: event.text,
          options: event.options,
          multiSelect: event.multiSelect === true,
          planMode: event.planMode === true,
          questionId: event.id,
          askId: event.askId,
          index: event.index,
          total: event.total,
          header: event.header,
        }
        // A re-emission of a known id replaces the card in place (the
        // desktop augments options as it learns them). The patch carries no
        // `seq`, so the card KEEPS the one it was first published under — it
        // did not move in the transcript.
        const replaced = upsertQuestion(
          feed,
          event.id,
          item as Omit<QuestionItem, `id`>
        )
        if (replaced) {
          // A card replaced IN PLACE: its options grew, so re-derive rather
          // than accumulate.
          setFeed(replaced)
        } else {
          const row = { ...item, id: nextId++, seq: currentSeq } as FeedItem
          setFeed([...feed, row], [row])
        }
        return
      }
      case `question_resolved`: {
        const resolved = applyQuestionResolved(feed, event)
        // A resolution writes answers into cards anywhere in the transcript;
        // the running byte count is cheaper to re-derive than to track.
        if (resolved) setFeed(resolved)
        return
      }
      case `answer_ack`: {
        if (!event.id) return
        clearAckTimer(event.id)
        answerStates = ackAnswer(answerStates, event.id)
        return
      }
      case `subagent`: {
        if (!event.id) return
        append({
          kind: `subagent`,
          subagentId: event.id,
          agentType: event.agentType,
          title: event.title?.trim() ? event.title.trim() : undefined,
          // EXP-856: `duplicate` is its own edge (an amber warning row);
          // anything this build does not know reads as `started`, which is
          // what an older client does with a newer status.
          status:
            event.status === `completed`
              ? `completed`
              : event.status === `duplicate`
                ? `duplicate`
                : `started`,
          detail: event.detail?.trim() ? event.detail : undefined,
          toolCalls:
            typeof event.toolCalls === `number` ? event.toolCalls : undefined,
          workflowId: event.workflowId?.trim()
            ? event.workflowId.trim()
            : undefined,
          at:
            typeof event.at === `number` && Number.isFinite(event.at)
              ? event.at
              : undefined,
        })
        return
      }
      case `thought`: {
        // EXP-1324: `started` appends the Reasoning row; `ended` settles the
        // newest row with the same id. An `ended` this feed has no row for
        // (evicted, below the window) is dropped, like a `tool_update`.
        const thoughtId = typeof event.id === `string` ? event.id.trim() : ``
        if (!thoughtId) return
        const subagentId = event.subagentId?.trim() ? event.subagentId : undefined
        if (event.state === `started`) {
          append({ kind: `thought`, thoughtId, settled: false, subagentId })
          return
        }
        if (event.state !== `ended`) return
        let at = -1
        for (let i = feed.length - 1; i >= 0; i--) {
          const item = feed[i]
          if (item.kind === `thought` && item.thoughtId === thoughtId) {
            at = i
            break
          }
        }
        if (at < 0) return
        const current = feed[at] as Extract<FeedItem, { kind: `thought` }>
        const durationMs =
          typeof event.durationMs === `number` &&
          Number.isFinite(event.durationMs) &&
          event.durationMs >= 0
            ? Math.round(event.durationMs)
            : undefined
        const updated = feed.slice()
        updated[at] = { ...current, settled: true, durationMs }
        setFeed(updated, [])
        return
      }
      case `permission`: {
        if (!event.tool?.trim()) return
        append({
          kind: `permission`,
          tool: event.tool,
          detail: event.detail?.trim() ? event.detail : undefined,
        })
        return
      }
      case `diff`: {
        // Diffs never enter the feed — the latest replaces the previous one
        // behind the pinned "Latest changes" strip. EXP-688: an EMPTY frame
        // is the publisher saying the branch no longer differs, so it clears
        // the bar rather than leaving a stale diff standing.
        latestDiff = event.diff.trim() ? event.diff : null
        return
      }
      case `compaction`: {
        if (event.phase === `started`) {
          const startedAt =
            typeof event.at === `number` && Number.isFinite(event.at)
              ? event.at
              : Date.now()
          compacting = { startedAt, trigger: event.trigger }
          armCompactionBackstop(startedAt)
          return
        }
        if (event.phase !== `ended`) return
        clearCompaction()
        // An UNMATCHED `ended` still writes the marker: codex publishes no
        // start marker for its automatic compaction, and the fold happened
        // either way.
        append({ kind: `compaction` })
        return
      }
      case `config_state`: {
        // EXP-746: a SLOT, not a row. A payload we cannot read keeps the
        // previous snapshot standing — blanking the chips mid-run would read
        // as "the agent lost its settings", which is never what a malformed
        // frame means.
        const next = parseConfigState(event)
        if (next) config = next
        return
      }
      case `usage`: {
        // Same slot rule, opposite null handling: an unusable payload (a zero
        // context window included) CLEARS the meter — a stale used/size beside
        // a live run reads as current, and "unknown" is what a zero size says.
        usage = parseSessionUsage(event)
        return
      }
      case `context_layout`: {
        // EXP-1051: the `config_state` null rule, not the `usage` one — the
        // layers do NOT change mid-conversation, so an unreadable frame keeps
        // the standing layout rather than collapsing the bar into one
        // undifferentiated conversation block.
        const next = parseContextLayout(event)
        if (next) contextLayout = next
        return
      }
      case `rate_limit`: {
        // EXP-784: the fourth slot. Null clears — an empty/`ok` status says
        // the window lifted, and an unreadable payload must not leave a
        // stale "rate limited" banner beside a live run.
        rateLimit = parseRateLimit(event)
        return
      }
      case `turn`: {
        // EXP-848: the fifth slot — never a feed row. An unreadable payload
        // keeps the current state (the `config_state` rule): guessing `ended`
        // mid-turn would stop the working indicator over a thinking agent.
        const next = parseTurnState(event)
        if (next) turnState = next
        // EXP-1324: a turn that ended settles every thought still open (an
        // `ended` edge that never came reads settled, without a duration).
        if (next === `ended` && feed.some((i) => i.kind === `thought` && !i.settled)) {
          setFeed(
            feed.map((i) => (i.kind === `thought` && !i.settled ? { ...i, settled: true } : i)),
            []
          )
        }
        // EXP-850 §5: `startedAt`/`tokens` ride the same slot. A republish
        // that omits either NEVER blanks what the slot already learned (the
        // publisher throttles the token tick, so most frames carry the start
        // alone); a NEW start is a new turn, so the token count resets.
        const startedAt = event.startedAt
        if (typeof startedAt === `number` && Number.isFinite(startedAt)) {
          if (startedAt !== turnStartedAt) {
            turnStartedAt = startedAt
            turnTokens = null
          }
        }
        const tokens = event.tokens
        if (typeof tokens === `number` && Number.isFinite(tokens) && tokens >= 0) {
          turnTokens = Math.round(tokens)
        }
        return
      }
      case `background_tasks`: {
        // EXP-850 §2: the FULL list, so an empty array closes the strip. An
        // unreadable payload keeps the previous list (the `config_state`
        // rule) rather than hiding work that is still running.
        const tasks = parseBackgroundTasks(event)
        // EXP-1324: every task carries a start (the wire's, else first sight).
        if (tasks)
          backgroundTasks = stampBackgroundStarts(tasks, backgroundFirstSeen, Date.now())
        return
      }
      case `task_list`: {
        // EXP-927: the FULL list; an unreadable payload keeps the previous
        // one (the same rule).
        const entries = parseTaskList(event)
        if (entries) taskList = entries
        return
      }
      case `queue`: {
        // EXP-861: the FULL queue, so an empty array closes the strip; an
        // unreadable payload keeps the previous list (the same rule).
        const messages = parseQueue(event)
        if (!messages) return
        queue = messages
        // A message this client echoed that the engine is HOLDING: the strip
        // is its only home until delivery, so the echo row goes (see
        // takeQueuedEchoes — otherwise a delivery past ECHO_TTL_MS doubles it).
        const taken = takeQueuedEchoes(recentEchoes, messages)
        const rowIds = new Set<number>()
        for (const echo of taken) {
          if (echo.rowId !== undefined) rowIds.add(echo.rowId)
        }
        if (rowIds.size > 0) setFeed(feed.filter((row) => !rowIds.has(row.id)))
        return
      }
      case `workflow`: {
        // EXP-850 §3: latest-wins PER id in a side map — never a feed row.
        // The tool row that spawned it carries the same id and renders as the
        // card; its own `tool_update` settle folds into that row, so the card
        // is never doubled.
        const workflow = parseWorkflow(event)
        if (!workflow) return
        const next = new Map(workflows)
        next.set(workflow.id, workflow)
        workflows = next
        const at = feed.findIndex(
          (item) => item.kind === `tool` && item.callId === workflow.id
        )
        if (at >= 0) {
          const current = feed[at] as ToolItem
          if (current.workflowId === workflow.id) return
          const updated = feed.slice()
          updated[at] = { ...current, workflowId: workflow.id }
          setFeed(updated, [])
        }
        return
      }
      default:
        // Future kinds from a newer desktop: ignore, never crash the socket.
        return
    }
  }

  /** A resolved card carries its own answer — drop its lock so a stale ack
   *  deadline can't flip a finished card into the retry state. */
  const reconcileResolvedAnswers = () => {
    for (const item of feed) {
      if (item.kind !== `question` || item.resolved !== true) continue
      const key = answerKey(item)
      if (!(key in answerStates)) continue
      clearAckTimer(key)
      answerStates = clearAnswer(answerStates, key)
    }
  }

  // ── Staged replay (EXP-656 → web EXP-751) ────────────────────────────────
  //
  // The relay answers EVERY viewer join with `activity_reset` + a full replay
  // of the room log, and a publisher reconnect fans out the same pair. Doing
  // what the frame literally says — empty the feed, then re-append N events
  // — painted the rows as they streamed, so a reconnect visibly rebuilt the
  // feed (and blanked the chips and the usage line until their replayed
  // snapshots landed). The burst is buffered and swapped in as ONE commit
  // instead: same result, no intermediate state, and the replayed prefix
  // keeps the row ids the reader is anchored on. Android
  // `SteerConnection.commitStaging` / iOS `SteerReplayStaging` are the
  // originals; the rules are theirs.

  const clearStagingTimers = () => {
    if (stageQuietTimer) {
      clearTimeout(stageQuietTimer)
      stageQuietTimer = null
    }
    if (stageCapTimer) {
      clearTimeout(stageCapTimer)
      stageCapTimer = null
    }
  }

  /** The timer fallbacks commit OUTSIDE the coalescer's flush, so they
   *  publish the snapshot themselves. */
  const commitStagingFromTimer = (why: string) => {
    if (disposed || staged === null) return
    commitStaging(why)
    reconcileResolvedAnswers()
    commit()
  }

  /** `activity_reset`: open (or restart) the staging buffer. The VISIBLE feed
   *  is untouched — a second reset mid-replay means the publisher restarted
   *  its stream, so the half we buffered is dead, never committed. */
  const beginStaging = () => {
    clearStagingTimers()
    staged = []
    stagedEchoes = []
    // The replay is the ONLY copy of everything sent before the reset, so
    // its transcript rows must render — only echoes sent DURING the window
    // (pushed after this) still dedupe their late twins.
    recentEchoes.length = 0
    // A republish that never goes quiet still has to land eventually.
    stageCapTimer = setTimeout(() => commitStagingFromTimer(`cap`), REPLAY_MAX_MS)
  }

  /** Buffer one replayed event and push the quiet deadline out. */
  const stageEvent = (event: ActivityEvent, seq: number) => {
    if (staged === null) return
    staged.push({ event, seq })
    if (stageQuietTimer) clearTimeout(stageQuietTimer)
    stageQuietTimer = setTimeout(
      () => commitStagingFromTimer(`quiet`),
      REPLAY_QUIET_MS
    )
  }

  /** Whether the folded feed already ends with this echo — the replay is
   *  authoritative, so anything it carried back must not be duplicated. */
  const tailCarriesEcho = (text: string, window: number): boolean => {
    const needle = text.trim()
    for (let i = feed.length - 1; i >= 0 && i >= feed.length - window; i--) {
      const item = feed[i]
      if (item.kind === `user_message` && item.text.trim() === needle) return true
    }
    return false
  }

  /**
   * Swap the staged replay in as the feed — everything derived from the old
   * log goes with it and is re-derived from the replay in the SAME pass: the
   * feed, the diff bar and the EXP-746 slots (the relay replays its latest
   * `config_state`/`usage` right after the log, so the chips repaint with the
   * rows instead of blanking first), the compaction strip, the answer locks.
   * Two things are carried across the swap: messages this client sent during
   * the window (the replay predates them) and locks on cards the replay
   * brought back — the tap that locked them may be milliseconds old, and a
   * card that came back unlocked would fire twice.
   */
  const commitStaging = (_why: string, firstSeq?: number) => {
    const events = staged
    if (events === null) return
    clearStagingTimers()
    staged = null
    const echoes = stagedEchoes
    stagedEchoes = []

    const carried: AnswerStates = {}
    for (const [key, state] of Object.entries(answerStates)) {
      if (isAnswerLocked(state)) carried[key] = state
    }
    // The oldest visible row's id: replaying the same history from here hands
    // the unchanged prefix the ids (React keys) it already had, so the rows
    // the reader is anchored on keep their identity across the swap. Safe
    // BECAUSE the swap is one commit — the old rows and the rewound counter
    // never coexist in a render.
    const anchorId = feed[0]?.id
    // EXP-783: everything this client holds BELOW the replay's oldest
    // sequence is a prefix the replay does not restate — pages a reader
    // scrolled back to load, which the full swap used to throw away. A commit
    // with no span at all (a publisher-driven republish, which carries no
    // `activity_synced`) still swaps the whole feed: there is nothing to
    // measure the prefix against.
    let retained: FeedItem[] = []
    if (firstSeq !== undefined) {
      let split = 0
      while (split < feed.length && feed[split].seq < firstSeq) split++
      retained = feed.slice(0, split)
    }
    const retainedNextId =
      retained.length > 0 ? retained[retained.length - 1].id + 1 : undefined
    feed = []
    feedBytes = 0
    latestDiff = null
    config = null
    usage = null
    // EXP-1051: the replay restates the device's layout too.
    contextLayout = null
    rateLimit = null
    // EXP-848: the replay carries the device's latest `turn` — until it lands,
    // idle (the same rule as a fresh store).
    turnState = `ended`
    // EXP-850: the same rule for the three slots the replay restates.
    turnStartedAt = null
    turnTokens = null
    backgroundTasks = []
    taskList = []
    // EXP-861: the replay restates the device's queue too.
    queue = []
    workflows = new Map()
    clearCompaction()
    // Seeded BEFORE the fold so a replayed `answer_ack`/`question_resolved`
    // for a carried lock lands on it; locks whose card the replay did not
    // bring back are dropped right after.
    answerStates = carried
    if (anchorId !== undefined) nextId = anchorId
    // The retained prefix keeps its rows AND its ids; the replay continues
    // numbering above them, so no row identity is reused.
    if (retained.length > 0) {
      setFeed(retained)
      if (retainedNextId !== undefined) nextId = retainedNextId
    }
    foldingReplay = true
    try {
      for (const { event, seq } of events) {
        currentSeq = seq
        handleActivity(event)
      }
    } finally {
      foldingReplay = false
    }
    // `currentSeq` stays on the replay's last event: the echoes below are this
    // client's own rows and belong right after it.
    // …unless the replay's `queue` says the device is still HOLDING one: the
    // strip carries it until delivery (the same rule as the live frame).
    const heldTexts = new Set(queue.map((entry) => entry.text.trim()))
    for (const text of echoes) {
      if (heldTexts.has(text.trim())) continue
      if (!tailCarriesEcho(text, echoes.length + 1)) {
        append({ kind: `user_message`, text, at: Date.now() })
      }
    }
    const liveKeys = new Set<string>()
    for (const item of feed) {
      if (item.kind === `question`) liveKeys.add(item.questionId)
    }
    for (const key of Object.keys(answerStates)) {
      if (!liveKeys.has(key)) answerStates = clearAnswer(answerStates, key)
    }
    // An ack deadline whose lock did not carry over guards nothing.
    for (const key of [...ackTimers.keys()]) {
      if (!(key in answerStates)) clearAckTimer(key)
    }
  }

  /** EXP-783 — the oldest wire sequence on screen: what the next older-page
   *  request is asked relative to. `undefined` only on an empty feed, which is
   *  also the signal that there is nothing to page back from. */
  const oldestSeq = (): number | undefined => feed[0]?.seq

  /** EXP-783 — PREPEND one older page, oldest first.
   *
   *  The page is transcript from BELOW everything on screen, so it is folded
   *  into a scratch reducer and spliced in front: every visible row keeps its
   *  id (its React key), its answer state and its position. A page
   *  overlapping what is already held is trimmed against `oldestSeq` — a
   *  re-asked page must never double the transcript. Ignored while a replay
   *  is staging: the replay is authoritative and is about to decide what the
   *  prefix even is. */
  const prependPage = (events: ActivityEvent[], seqs: number[]) => {
    if (staged !== null || events.length === 0) return
    const oldest = oldestSeq()
    // Fold the page through the SAME reducer the live stream uses, over a
    // scratch feed, so grouping/merging behave identically.
    const savedFeed = feed
    const savedBytes = feedBytes
    const savedNextId = nextId
    const savedFolding = foldingReplay
    // The page is OLDER than everything on screen, so its sequences must not
    // become the anchor a later local row inherits.
    const savedSeq = currentSeq
    // EXP-848: the latest-wins slots belong to the NEWEST frames — an older
    // page folding its own `turn`/`config_state`/`usage`/`rate_limit` through
    // the reducer would repaint them with history (a stale `turn started` made
    // a finished run pulse). Saved here, restored below.
    const savedSlots = {
      config,
      usage,
      contextLayout,
      rateLimit,
      turnState,
      turnStartedAt,
      turnTokens,
      backgroundTasks,
      taskList,
      queue,
      workflows,
    }
    feed = []
    feedBytes = 0
    nextId = 0
    foldingReplay = true
    try {
      events.forEach((event, ix) => {
        // `seqs` runs parallel to `events` (the relay forwards the pair the
        // device unzipped), so a page row is always numbered.
        const seq = seqs[ix]
        if (oldest !== undefined && seq >= oldest) return
        currentSeq = seq
        handleActivity(event)
      })
    } finally {
      currentSeq = savedSeq
      foldingReplay = savedFolding
    }
    const page = feed
    feed = savedFeed
    feedBytes = savedBytes
    nextId = savedNextId
    config = savedSlots.config
    usage = savedSlots.usage
    contextLayout = savedSlots.contextLayout
    rateLimit = savedSlots.rateLimit
    turnState = savedSlots.turnState
    turnStartedAt = savedSlots.turnStartedAt
    turnTokens = savedSlots.turnTokens
    backgroundTasks = savedSlots.backgroundTasks
    taskList = savedSlots.taskList
    queue = savedSlots.queue
    workflows = savedSlots.workflows
    if (page.length === 0) {
      historyExhausted = true
      return
    }
    // The prepended rows take ids BELOW every id on screen, so ordering by id
    // stays the ordering of the transcript.
    const base = (savedFeed[0]?.id ?? page.length) - page.length
    const renumbered = page.map((item, offset) => ({ ...item, id: base + offset }))
    setFeed([...renumbered, ...savedFeed], renumbered)
  }

  /** EXP-783 — one `history_page` ask, at most one in flight.
   *
   *  A viewer that joined a long-running session holds only the relay's
   *  replay TAIL (`truncated` on `activity_synced` is how it knows), and the
   *  pages below it exist only in the device's journal. */
  const requestOlderPage = (): boolean => {
    if (historyRequest !== null || historyExhausted || !historyTruncated) return false
    const before = oldestSeq()
    if (before === undefined || before === 0) {
      historyExhausted = true
      return false
    }
    const requestId = `p${++historyRequests}`
    if (ws?.readyState !== WebSocket.OPEN) return false
    ws.send(
      JSON.stringify({
        t: `history_page`,
        requestId,
        beforeSeq: before,
        limit: HISTORY_PAGE_LIMIT,
      })
    )
    historyRequest = requestId
    return true
  }

  /** Drop a staged replay and KEEP the visible feed: the socket went away
   *  mid-burst, so the buffer is a partial history of a room this client is
   *  no longer joined to. The next join replays from scratch. */
  const discardStaging = () => {
    if (staged === null) return
    clearStagingTimers()
    staged = null
    stagedEchoes = []
  }

  // REV-33: a join replay fans the relay's whole activity log (up to
  // FEED_CAP frames) out as individual ws messages. Handling each one
  // directly meant one notify per frame over the full non-virtualized feed
  // — O(n²) work that froze the tab on open/reconnect. Frames buffer here
  // and apply in one synchronous pass per window instead; `activity_reset`
  // and `activity_synced` ride the same queue so neither can overtake
  // buffered frames. The queue outlives redials (order is preserved across
  // them) and only dispose cancels it.
  const activityQueue = createActivityCoalescer<
    | { t: `reset` }
    | { t: `event`; event: ActivityEvent; seq: number }
    | { t: `synced`; firstSeq: number }
    | { t: `page`; events: ActivityEvent[]; seqs: number[] }
    | { t: `keepalive` }
  >((batch) => {
    if (disposed) return
    for (const op of batch) {
      switch (op.t) {
        case `reset`:
          queuedResets = Math.max(0, queuedResets - 1)
          beginStaging()
          break
        case `event`:
          if (staged !== null) stageEvent(op.event, op.seq)
          else {
            currentSeq = op.seq
            handleActivity(op.event)
          }
          break
        case `synced`:
          // Outside a replay (a relay we joined before the window opened)
          // there is nothing to commit — never a feed change.
          commitStaging(`marker`, op.firstSeq)
          break
        case `page`:
          prependPage(op.events, op.seqs)
          break
        case `keepalive`:
          // The relay's own 15s beat: if it got a turn, the replay burst is
          // over. This is what ends a publisher-driven republish, which
          // carries no marker — and therefore no span either.
          commitStaging(`keepalive`)
          break
      }
    }
    reconcileResolvedAnswers()
    commit()
  })

  const scheduleRedial = () => {
    clearRetryTimer()
    retryTimer = setTimeout(() => {
      retryTimer = null
      void dial(true)
    }, startingRetryDelay(retries++))
  }

  /** VAPP-169: the device-offline copy redials every `staleMs` while it shows. */
  const scheduleOfflineRedial = () => {
    clearRetryTimer()
    retryTimer = setTimeout(() => {
      retryTimer = null
      if (deviceOffline && steerOfflineRedial(`offline`, `staleTimer`)) void dial(true)
    }, STEER_ENET_STALE_MS)
  }

  const dial = async (retrying: boolean) => {
    if (disposed) return
    const gen = ++generation
    clearDialTimers()
    // A superseded socket's HANDLERS are inert (the generation gate), but the
    // socket is not: left open it stays joined at the device as a duplicate
    // viewer until the run ends. Every dial abandons its predecessor, so
    // the close belongs here — that makes every caller safe, `kick`'s
    // `starting` retry (which dials straight over an in-flight dial) included.
    if (ws) {
      ws.close()
      ws = null
      // A replay the abandoned socket never finished delivering is a partial
      // history of a room this dial is leaving — keep what the reader sees.
      discardStaging()
      // A page asked for on that socket is not coming either (EXP-795): the
      // device drops the ask with the viewer, so a new one may go out.
      historyRequest = null
      if (connected) {
        connected = false
        // Dim the composer honestly for the gap; the phase itself holds.
        commit()
      }
    }
    dialStartedAt = Date.now()
    // Hold the current phase steady across auto-retry redials — flipping
    // to `connecting` per attempt makes the header flicker on every redial
    // (and a silent slow-consumer redial must not flicker at all).
    if (!retrying) {
      phase = { kind: `connecting` }
      commit()
    }

    // VAPP-43: eNet is the only carrier (its own deadlines: joinTimeoutMs).
    // No endpoint, no node, every endpoint refused or no `activity_synced`
    // in time = NO FEED, shown with the existing states: a row still alive
    // waits in `starting` and redials on the backoff, an ended row is over
    // and its transcript sits on a machine this viewer cannot reach, until
    // the device comes back (VAPP-169: the redial holds the phase).
    const sock = await deps.openEnet(sessionId).catch(() => null)
    if (disposed || gen !== generation) {
      sock?.close()
      return
    }
    if (!sock) {
      if (sessionStatus === `ended`) {
        deviceOffline = true
        phase = {
          kind: `ended`,
          detail: `${deviceLabel ?? `The device`} is offline. The transcript lives on that machine.`,
        }
        clearCompaction()
        restoreQueueToDraft()
        commit()
        onEnded()
        scheduleOfflineRedial()
      } else {
        phase = { kind: `starting` }
        commit()
        scheduleRedial()
      }
      return
    }

    // `bye` / no_such_session must win over the generic close handler.
    let sawEnd = false
    let retryStarting = false
    // A refusal a redial can only repeat (`unauthorized`): the close it
    // precedes is terminal, so the wakeup kicks leave it alone.
    let terminalError = false
    let detail: string | null = null

    ws = sock
    sock.onopen = () => {
      if (disposed || gen !== generation) return
      connected = true
      commit()
      // The feed is NEVER wiped here (protocol v2): the device sends an
      // explicit `activity_reset` immediately before its join replay, so
      // a redial that never lands keeps showing what was already there.
      sock.send(JSON.stringify({ t: `join`, channel: `activity` }))
      // NOT live yet: the phase flips to live on the first confirming
      // frame (the replay's `activity_reset`).
      // EXP-625: the join is always answered, so arm a deadline for that
      // answer. A feed that stays mute is closed here, and the onclose path
      // below turns it into an honest, retryable phase.
      joinAckTimer = setTimeout(() => {
        joinAckTimer = null
        if (disposed || gen !== generation) return
        sock.close()
      }, JOIN_ACK_TIMEOUT_MS)
    }
    sock.onmessage = (event) => {
      if (disposed || gen !== generation || typeof event.data !== `string`)
        return
      // Any frame at all proves the feed is alive (EXP-625).
      lastFrameAt = Date.now()
      if (joinAckTimer) {
        clearTimeout(joinAckTimer)
        joinAckTimer = null
      }
      const frame = parseServerFrame(event.data)
      if (!frame) return
      switch (frame.t) {
        case `activity`: {
          const f = frame as Extract<ServerFrame, { t: `activity` }>
          activityQueue.enqueue({ t: `event`, event: f.event, seq: f.seq })
          if (markLive()) commit()
          return
        }
        case `keepalive`:
          // EXP-648: already counted by the `lastFrameAt` stamp above.
          // Never a phase change and never a commit — it must not touch
          // the feed or re-render anything. The one exception: its cadence
          // proves a staged replay burst is over, so while a replay is
          // staging (or a reset is still queued ahead of it) it rides the
          // queue to end it.
          if (staged !== null || queuedResets > 0) {
            activityQueue.enqueue({ t: `keepalive` })
          }
          return
        case `activity_reset`: {
          queuedResets++
          activityQueue.enqueue({ t: `reset` })
          if (markLive()) commit()
          return
        }
        case `activity_synced`: {
          // EXP-656: the end-of-replay marker — the join succeeded and the
          // staged replay commits.
          // EXP-783: it names the span, so the commit keeps the pages this
          // client had already scrolled back to load.
          const f = frame as Extract<ServerFrame, { t: `activity_synced` }>
          historyTruncated = f.truncated === true
          activityQueue.enqueue({ t: `synced`, firstSeq: f.firstSeq })
          if (markLive()) commit()
          return
        }
        // EXP-783: one page of older transcript. It goes in FRONT of
        // everything on screen, so it rides the same queue (order with the
        // live tail matters) and never touches the staging buffer.
        case `history_chunk`: {
          const f = frame as Extract<ServerFrame, { t: `history_chunk` }>
          if (f.requestId !== historyRequest) return
          if (f.done) historyRequest = null
          activityQueue.enqueue({ t: `page`, events: f.events, seqs: f.seqs })
          return
        }
        case `bye`: {
          // The run ended, or (VAPP-43) a stored journal of an ended row
          // was served (`history`): the feed stays, with the plain ended
          // caption. The device drops this viewer and the socket closes.
          const f = frame as Extract<ServerFrame, { t: `bye` }>
          sawEnd = true
          endedByRow = false
          detail =
            f.outcome && f.outcome !== `ended` && f.outcome !== `history`
              ? f.outcome
              : null
          return
        }
        case `error`: {
          const f = frame as Extract<ServerFrame, { t: `error` }>
          if (f.code === `no_such_session`) {
            // The device no longer runs it nor holds its journal (a re-join
            // after the run went away) — auto-retry while the synced row
            // still says running.
            retryStarting = true
          } else {
            detail = f.message ?? f.code
            terminalError = f.code === `unauthorized`
          }
          sock.close()
          return
        }
        default:
          // Unknown frames from a newer device are inert.
          return
      }
    }
    sock.onclose = (event) => {
      if (disposed || gen !== generation) return
      clearDialTimers()
      ws = null
      connected = false
      endedByRow = false
      // A half-delivered replay is worth less than the last complete
      // picture — the reader keeps what they were reading (EXP-656).
      discardStaging()
      // Nor is an in-flight page ask answered on a closed feed (EXP-795).
      historyRequest = null
      if (sawEnd) {
        deviceOffline = false
        phase = { kind: `ended`, detail: detail ?? undefined }
        // A run that ended mid-fold is not compacting any more (EXP-724).
        clearCompaction()
        // EXP-861: nothing is queued on a run that is over — the device
        // dropped its queue with the turn it was waiting for, so the text
        // goes back to the draft.
        restoreQueueToDraft()
        commit()
        onEnded()
        return
      }
      if (retryStarting) {
        // An idle session is still alive and steerable (EXP-194) — only
        // a truly ended session stops the redial.
        if (sessionStatus !== `ended`) {
          phase = { kind: `starting` }
          commit()
          scheduleRedial()
        } else {
          phase = { kind: `ended` }
          restoreQueueToDraft()
          commit()
          onEnded()
        }
        return
      }
      // EXP-621: a lost eNet feed (silent past a re-join, a send that never
      // landed) is not an ending. Redial silently (the phase — usually
      // `live` — holds steady, so nothing flickers; the commit still
      // surfaces `connected: false` so the send button dims honestly for
      // the gap); the shared backoff bounds a pathological loop. An ended
      // row too (EXP-781): a stored replay cut short is retried from
      // scratch, and a device that is gone lands on the no-feed path.
      if (event.code === CLOSE_ENET_LOST) {
        commit()
        scheduleRedial()
        return
      }
      phase = {
        kind: `closed`,
        detail: detail ?? undefined,
        terminal: terminalError,
      }
      commit()
    }
  }

  // ── Steering (message-shaped; the device applies the access rule) ─────────

  /**
   * Forward raw input (chunked ≤4 KiB, never splitting a surrogate pair).
   */
  const sendInput = (data: string): boolean => {
    if (ws?.readyState !== WebSocket.OPEN) return false
    for (let i = 0; i < data.length; ) {
      let end = Math.min(i + INPUT_CHUNK_CHARS, data.length)
      const last = end < data.length ? data.charCodeAt(end - 1) : 0
      if (last >= 0xd800 && last <= 0xdbff) end += 1
      ws.send(JSON.stringify({ t: `input`, data: data.slice(i, end) }))
      i = end
    }
    return true
  }

  /** Protocol v2 answer: it reaches the desktop verbatim, which
   *  drives its own picker and confirms with `answer_ack`. The ONLY answer
   *  path (EXP-672) — a card without a wire id is not answerable from here. */
  const sendAnswerFrame = (
    questionId: string,
    askId: string | undefined,
    keys: string[],
    text?: string
  ): boolean => {
    if (ws?.readyState !== WebSocket.OPEN) return false
    ws.send(JSON.stringify({ t: `answer`, questionId, askId, keys, text }))
    return true
  }

  /** EXP-746: switch to one of the modes `config_state.modes[]` advertised.
   *  Fire-and-forget — the publisher re-emits `config_state` once it applied
   *  and that repaint IS the confirmation, so there is no optimistic write
   *  and no ack timer. EXP-772 retired the option sender beside it: nothing
   *  sends `set_config` any more. */
  const sendModeFrame = (id: string): boolean => {
    if (ws?.readyState !== WebSocket.OPEN) return false
    ws.send(JSON.stringify({ t: `set_mode`, id }))
    return true
  }

  /** EXP-790: cancel the agent's current turn. The frame reaches the
   *  publisher verbatim; the engine's `session/cancel` dismisses every
   *  open card and the feed shows the interrupted turn. */
  const sendInterruptFrame = (): boolean => {
    if (ws?.readyState !== WebSocket.OPEN) return false
    ws.send(JSON.stringify({ t: `interrupt` }))
    return true
  }

  // ── Ended/retention lifecycle (driven by the registry below) ─────────────

  let endedTimer: ReturnType<typeof setTimeout> | null = null
  let reapTimer: ReturnType<typeof setTimeout> | null = null
  const onEnded = () => {
    // A quick re-open still shows the tail; an unwatched ended store frees
    // its socket-less state shortly after.
    if (listeners.size === 0) scheduleSelfDispose(ENDED_GRACE_MS)
  }
  const scheduleSelfDispose = (delay: number) => {
    if (endedTimer) clearTimeout(endedTimer)
    endedTimer = setTimeout(() => {
      endedTimer = null
      store.dispose()
    }, delay)
  }
  const cancelSelfDispose = () => {
    if (endedTimer) {
      clearTimeout(endedTimer)
      endedTimer = null
    }
  }

  const store: InternalSteerSessionStore = {
    sessionId,
    subscribe(listener) {
      listeners.add(listener)
      cancelSelfDispose()
      return () => {
        listeners.delete(listener)
        if (listeners.size === 0 && phase.kind === `ended`) {
          scheduleSelfDispose(ENDED_GRACE_MS)
        }
      }
    },
    getSnapshot: () => snapshot,
    getDraftSnapshot: () => draftSnapshot,
    connect() {
      if (disposed) return
      if (phase.kind !== `idle` && phase.kind !== `closed`) return
      void dial(false)
    },
    reconnect() {
      if (disposed) return
      clearRetryTimer()
      clearDialTimers()
      retries = 0
      deviceOffline = false
      generation++
      ws?.close()
      ws = null
      connected = false
      void dial(false)
    },
    loadEarlier() {
      if (disposed) return false
      return requestOlderPage()
    },
    kick(reason) {
      if (disposed) return
      const current = phase
      // VAPP-169: the host device's presence flip is its own trigger.
      const trigger = reason === `device-online` ? `presenceOnline` : `wakeup`
      switch (current.kind) {
        case `closed`:
          // An unexpected close stays terminal BY DESIGN until something
          // says the world changed, and a wakeup is exactly that. A
          // session the synced row calls ended is left alone, and so is a
          // close no retry can fix (EXP-648, see ViewerPhase) — that would
          // cost one dial per visibility/online event per retained store.
          if (current.terminal) return
          if (sessionStatus !== `ended`) store.reconnect()
          return
        case `connecting`:
          // Only a dial that is provably stuck: no frame since it started
          // and already past the join deadline. A young dial is left to
          // finish (or to hit its own deadline).
          if (lastFrameAt >= dialStartedAt) return
          if (Date.now() - dialStartedAt <= JOIN_ACK_TIMEOUT_MS) return
          store.reconnect()
          return
        case `starting`:
          // The desktop's publisher may well have arrived while we were
          // away, so retry NOW instead of waiting out a 30s backoff step.
          // The phase holds, so nothing flickers.
          // EXP-796: unless a feed is OPEN and talking — the device keeps an
          // idle row's viewer joined for the publisher's takeover, which
          // arrives on it; redialing would only throw the transcript away.
          // An in-flight dial (open, nothing heard yet) is still retried:
          // only a socket that has answered THIS dial recently is left be.
          if (
            connected &&
            lastFrameAt >= dialStartedAt &&
            Date.now() - lastFrameAt <= LIVE_STALE_MS
          )
            return
          if (!steerOfflineRedial(`starting`, trigger)) return
          clearRetryTimer()
          void dial(true)
          return
        case `live`:
          // EXP-648: live on paper over a socket that has said nothing for
          // longer than three device keepalives is dead — redial under the
          // `live` phase (the CLOSE_ENET_LOST mechanics) so a socket that turns out
          // fine never flashes "Disconnected". Silence is measured from the
          // LATER of the last frame and the current dial's start: a young
          // in-flight redial (visible + online firing back to back) is left
          // to finish or hit its own deadline, while a redial whose socket
          // never opened still self-heals after the window.
          // EXP-639: never for a run the synced row already calls ended — the
          // publisher is gone, so the redial can only draw `no_such_session`
          // and park the viewer in `starting` until the row syncs. Same rule
          // the `closed` case above applies.
          if (sessionStatus === `ended`) return
          if (Date.now() - Math.max(lastFrameAt, dialStartedAt) <= LIVE_STALE_MS)
            return
          clearRetryTimer()
          void dial(true)
          return
        case `ended`:
          // VAPP-169: the device-offline copy redials (the phase holds); a
          // transcript ended by `bye` never does.
          if (!steerOfflineRedial(deviceOffline ? `offline` : `ended`, trigger)) return
          clearRetryTimer()
          void dial(true)
          return
        default:
          // `idle` needs nothing.
          return
      }
    },
    noteSessionStatus(status) {
      sessionStatus = status
      // VAPP-173: a run that came back = live (steer-enet.json OFFLINE REDIAL).
      if (status !== `ended` && endedByRow && connected && phase.kind === `ended`) {
        endedByRow = false
        phase = { kind: `live` }
        commit()
      }
    },
    noteDeviceLabel(label) {
      deviceLabel = label
    },
    /**
     * Send one message to the agent: the text, then a SEPARATE `\r` frame —
     * bundled into one write TUI apps treat the trailing return as a paste,
     * which inserts instead of submitting. The sent text is echoed into the
     * local feed immediately (EXP-78); its transcript-derived `user_message`
     * event is deduped against the echo FIFO when it arrives.
     */
    sendMessage(text) {
      if (!text || !sendInput(text)) return false
      ws?.send(JSON.stringify({ t: `input`, data: `\r` }))
      // EXP-861: sent while the agent is mid-turn or compacting (the RAW slot
      // values, not the full working predicate), the DEVICE queues it — the
      // next `queue` frame shows it in the strip and the real `user_message`
      // row arrives when it is delivered. No echo: an echo here would sit in
      // the transcript ahead of the turn it is waiting on, and its dedupe
      // would then swallow the real row.
      if (turnState === `started` || compacting !== null) return true
      // The echo is this client's own row: it sits directly after the newest
      // event it has seen, so the feed's sequences stay monotonic.
      const row = {
        id: nextId++,
        kind: `user_message` as const,
        text,
        seq: currentSeq,
      }
      // Keyed to its row: a `queue` frame naming this text takes both back.
      pushEcho(recentEchoes, text, Date.now(), row.id)
      // Sent mid-replay: the staged history predates it, so the commit has
      // to put it back (unless the replay turns out to carry it).
      if (staged !== null) stagedEchoes.push(text)
      setFeed([...feed, row], [row])
      commit()
      return true
    },
    /** Submit a card's answer and LOCK it immediately — a locked card never
     *  fires again. `answer_ack` confirms the lock; `question_resolved`
     *  finalizes it; ANSWER_ACK_TIMEOUT_MS without either re-enables the card
     *  with an inline note. */
    answerQuestion(item, keys, labels, text) {
      const key = answerKey(item)
      if (isAnswerLocked(answerStates[key])) return
      // EXP-820: a resolved step of an ask can be answered AGAIN while the
      // ask is open (the engine re-records it); a resolved lone card cannot.
      if (item.resolved === true && item.askId === undefined) return
      if (!sendAnswerFrame(item.questionId, item.askId, keys, text)) return
      answerStates = beginAnswer(answerStates, key, keys, labels)
      clearAckTimer(key)
      ackTimers.set(
        key,
        setTimeout(() => {
          ackTimers.delete(key)
          answerStates = failAnswer(answerStates, key)
          commit()
        }, ANSWER_ACK_TIMEOUT_MS)
      )
      commit()
    },
    /** EXP-746: a fire-and-forget mode switch. No optimistic slot write —
     *  the publisher's re-emitted `config_state` is the confirmation, and a
     *  refused switch simply repaints the OLD value (which is honest: the
     *  agent kept it). */
    setMode(id) {
      return sendModeFrame(id)
    },
    /** EXP-790: Stop. EXP-873: the device drops every message the agent has
     *  not read yet with the turn (the bar's lines, held and sent alike —
     *  the CLI's own `cancel_queued`), so their text goes back into the
     *  composer, in order, instead of vanishing; the empty `queue` frame is
     *  the confirmation. The draft is empty whenever the button reads Stop. */
    interrupt() {
      if (!sendInterruptFrame()) return false
      if (dropQueueOnStop()) commit()
      return true
    },
    noteStopped() {
      if (!dropQueueOnStop()) return false
      commit()
      return true
    },
    takeDraftText() {
      const text = draftText
      if (!text) return ``
      draftText = ``
      commitDraft()
      return text
    },
    /** EXP-861: the strip's X. The frame is exactly `{"t":"unqueue","id"}`;
     *  the entry leaves the local slot at once (the device's next `queue`
     *  frame either confirms it or, if the message was already delivered,
     *  the transcript row says so). */
    unqueue(id) {
      if (ws?.readyState !== WebSocket.OPEN) return false
      ws.send(JSON.stringify({ t: `unqueue`, id }))
      if (queue.some((entry) => entry.id === id)) {
        queue = queue.filter((entry) => entry.id !== id)
        commit()
      }
      return true
    },
    clearRateLimit() {
      if (rateLimit === null) return
      rateLimit = null
      commit()
    },
    setDraftText(text) {
      draftText = text
      commitDraft()
    },
    addDraftImages(files) {
      // The marker text is the composer's (it knows the caret); only the
      // strip and the per-kind caps live here.
      const staged = stagePendingImages(draftImages, files, ``, 0)
      const addedImages = staged.images
        .slice(draftImages.length)
        .filter((entry) => entry.kind === `image`).length
      if (staged.added > 0) {
        draftImages = staged.images
        commitDraft()
      }
      return {
        rejected: staged.rejected,
        overflow: staged.overflow,
        fileOverflow: staged.fileOverflow,
        added: staged.added,
        addedImages,
      }
    },
    removeDraftImage(url) {
      URL.revokeObjectURL(url)
      draftImages = draftImages.filter((image) => image.url !== url)
      commitDraft()
    },
    setDraftImageUploaded(url, uploadedId, uploadedName) {
      draftImages = markPendingImageUploaded(
        draftImages,
        url,
        uploadedId,
        uploadedName
      )
      commitDraft()
    },
    clearDraftAfterSend() {
      for (const image of draftImages) URL.revokeObjectURL(image.url)
      draftImages = []
      draftText = ``
      commitDraft()
    },
    dispose() {
      if (disposed) return
      disposed = true
      clearRetryTimer()
      clearDialTimers()
      clearCompaction()
      discardStaging()
      cancelSelfDispose()
      if (reapTimer) clearTimeout(reapTimer)
      activityQueue.cancel()
      for (const timer of ackTimers.values()) clearTimeout(timer)
      ackTimers.clear()
      for (const image of draftImages) URL.revokeObjectURL(image.url)
      generation++
      ws?.close()
      ws = null
      onDispose?.()
    },
    _subscriberCount: () => listeners.size,
    _scheduleReap() {
      // Grace before reaping: transient empty live-query results and route
      // remounts must never kill a background socket (they resolve well
      // within the window); a truly gone session also closes via `bye`.
      if (reapTimer) return
      reapTimer = setTimeout(() => {
        reapTimer = null
        if (listeners.size === 0) store.dispose()
      }, RETAIN_GRACE_MS)
    },
    _cancelReap() {
      if (reapTimer) {
        clearTimeout(reapTimer)
        reapTimer = null
      }
    },
  }

  return store
}

// ── Registry ────────────────────────────────────────────────────────────────

type RegistryStore = ReturnType<typeof createSteerSessionStore>

const stores = new Map<string, RegistryStore>()

// EXP-625: the wakeups. A backgrounded tab (or a phone whose browser froze
// the page) comes back with sockets the OS quietly killed; nothing in the
// store notices until someone clicks Reconnect. One listener pair for the
// WHOLE registry (attached with the first store, removed when the last one
// goes) nudges every retained store instead.
function kickAll(reason: string) {
  for (const store of stores.values()) store.kick(reason)
}
const onVisibilityChange = () => {
  if (document.visibilityState === `visible`) kickAll(`visible`)
}
const onOnline = () => kickAll(`online`)
let wakeupsAttached = false

function attachWakeups() {
  if (wakeupsAttached || typeof document === `undefined`) return
  wakeupsAttached = true
  document.addEventListener(`visibilitychange`, onVisibilityChange)
  window.addEventListener(`online`, onOnline)
}

function detachWakeups() {
  if (!wakeupsAttached) return
  wakeupsAttached = false
  document.removeEventListener(`visibilitychange`, onVisibilityChange)
  window.removeEventListener(`online`, onOnline)
}

/** Get-or-create the store for a session (StrictMode-safe: repeated calls
 *  return the same instance; disposal is registry-owned, never unmount-owned). */
export function acquireSteerSession(sessionId: string): SteerSessionStore {
  let store = stores.get(sessionId)
  if (!store) {
    store = createSteerSessionStore(sessionId, defaultDeps, () => {
      stores.delete(sessionId)
      // The owner's turn log was folded from this store's feed: it goes too.
      releaseTurnLog(sessionId)
      if (stores.size === 0) detachWakeups()
    })
    stores.set(sessionId, store)
    attachWakeups()
  }
  return store
}

/** The dock's reaper: keep stores for the given sessions (the user's running
 *  sessions + the expanded one); anything else that also has no subscribers
 *  is disposed after a grace period. */
export function retainSteerSessions(keep: ReadonlySet<string>): void {
  for (const store of stores.values()) {
    if (keep.has(store.sessionId) || store._subscriberCount() > 0) {
      store._cancelReap()
    } else {
      store._scheduleReap()
    }
  }
}

/** Sign-out hygiene: close every background connection immediately. */
export function disposeAllSteerSessions(): void {
  for (const store of [...stores.values()]) store.dispose()
}
