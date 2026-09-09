import { trpc } from "@/lib/trpc-client"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import {
  ackAnswer,
  answerKey,
  parseConfigState,
  parseRateLimit,
  parseSessionUsage,
  parseToolKind,
  applyQuestionResolved,
  beginAnswer,
  clearAnswer,
  consumeEcho,
  createActivityCoalescer,
  mergeNarrationFragment,
  failAnswer,
  isAnswerLocked,
  pushEcho,
  resumesAfterCompaction,
  spliceBeforeQuestion,
  upsertQuestion,
  ANSWER_ACK_TIMEOUT_MS,
  COMPACTION_TIMEOUT_MS,
  feedItemBytes,
  trimFeed,
  HISTORY_PAGE_LIMIT,
  type AnswerStates,
  type EchoEntry,
  type SessionConfigState,
  type SessionRateLimitState,
  type SessionUsageState,
  type ToolKind,
} from "@/lib/agent-feed"
import {
  isAcceptedImageContentType,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import { MAX_STEER_IMAGES } from "@/lib/steer-image-message"
import type { CodingSession } from "@/db/schema"

// EXP-621: the viewer connection to the steer relay, lifted OUT of
// AgentSessionView into a module-level per-session store (the pattern of
// lib/collections.ts). The socket, feed, phase, answer state AND the
// composer draft all live here, so leaving the session page, switching dock
// tabs or navigating around the app detaches the VIEW without dropping the
// CONNECTION — reopening renders the retained feed instantly, no
// "Connecting…" phase and no full activity replay. The relay explicitly
// allows multiple concurrent viewers and an established socket outlives its
// 60s ticket, so background connections are protocol-safe; fewer redials
// also spares the relay's per-IP connect budget.

// ── Wire protocol (activity-viewer side of apps/steer-relay/src/protocol.ts) ─

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
const INPUT_CHUNK_CHARS = 4096
/** Redial backoff while the desktop's publisher socket is still starting:
 *  3s doubling to 30s. Each redial mints a fresh ticket and opens a fresh
 *  relay socket, so a fixed cadence across many waiting viewers would eat the
 *  relay's per-IP connect budget in lockstep. */
const STARTING_RETRY_BASE_MS = 3_000
const STARTING_RETRY_MAX_MS = 30_000
/** The relay evicts a viewer whose socket saturates (a stalled tab behind an
 *  OS file dialog, a backgrounded phone) with close code 4008 — see
 *  CLOSE_SLOW_CONSUMER in apps/steer-relay/src/protocol.ts. That is an
 *  eviction, not an ending: the session is still live, so the store redials
 *  silently instead of stranding the user on "Disconnected". */
const CLOSE_SLOW_CONSUMER = 4008
/** The relay refused the ticket (CLOSE_UNAUTHORIZED). A retry mints the same
 *  "no" forever, so this close is terminal (the mobile viewers agree). Today
 *  the relay refuses a bad ticket at the HTTP upgrade instead (401, which a
 *  browser reports as 1006 — rightly retryable, every redial mints a fresh
 *  ticket), so this is protocol completeness rather than a hot path. */
const CLOSE_UNAUTHORIZED = 4003
/** A disposal grace once a store is neither kept by the dock strip's reaper
 *  nor subscribed — long enough to survive transient empty live-query results
 *  and route remounts, which must never kill a background socket. */
const RETAIN_GRACE_MS = 60_000
/** A store whose session ENDED and lost its last subscriber lingers briefly
 *  so a quick re-open still shows the tail, then self-disposes. */
const ENDED_GRACE_MS = 5_000
/** EXP-625: the mint is a network round-trip with no deadline of its own.
 *  A request issued as the tab suspended can hang forever, and the store
 *  would sit on "Connecting…" with nothing to click. Bound it. */
const MINT_TIMEOUT_MS = 20_000
/** EXP-625: the relay ALWAYS answers a join (`activity_reset` + replay, or
 *  `error no_such_session` then close 4001), so silence after the join means
 *  a dead socket: one that opened but never delivers a frame. Close it so
 *  the normal onclose path runs and the user gets a Reconnect affordance
 *  instead of an eternal "Connecting…". */
const JOIN_ACK_TIMEOUT_MS = 15_000
/** EXP-648: the relay sends every joined viewer a `keepalive` frame every
 *  15s (apps/steer-relay/src/hub.ts), so three of those missing on a
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
/** What the mint race resolves to when the deadline wins (EXP-625). */
const MINT_TIMED_OUT = Symbol(`mint-timed-out`)

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
   *  answer frame's `text`. Absent from older desktops. */
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
  // EXP-785/786: a call settled and/or an edit's per-call diff — a LOG row
  // on the wire that folds INTO the tool row with that `id`, never a row of
  // its own; an id this feed does not hold is dropped.
  | {
      kind: `tool_update`
      id: string
      status?: `completed` | `failed`
      diff?: string
      at?: number
    }
  | { kind: `diff`; diff: string; at?: number }
  // EXP-78 (member-only on the relay): a human turn from the transcript…
  // `subagentId` (EXP-773): a turn addressed to a subagent, shown in that
  // subagent's view only.
  | { kind: `user_message`; text: string; subagentId?: string; at?: number }
  // …and an interactive question (AskUserQuestion / plan approval).
  // `planMode` marks an ExitPlanMode plan-approval picker (EXP-97) — absent
  // on generic questions and on events from older desktops/relays.
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
      id?: string
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
      status: `started` | `completed`
      detail?: string
      // EXP-748: the tool calls the publisher attributed to the subagent,
      // stamped on the COMPLETED edge. Authoritative over the tool rows the
      // feed can see — a replay buffer evicts subagent tool rows first, so
      // the visible ones undercount. Absent on older publishers.
      toolCalls?: number
      at?: number
    }
  | { kind: `permission`; tool: string; detail?: string; at?: number }
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
  // EXP-772: `options` still rides the wire from older publishers; the fold
  // ignores it, so it is not declared here.
  | {
      kind: `config_state`
      currentMode?: string
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
  // EXP-784: the rate-limit window, the fourth latest-wins slot. An empty or
  // `ok` status CLEARS it.
  | {
      kind: `rate_limit`
      status: string
      resetsAt?: number
      message?: string
      at?: number
    }

type ServerFrame =
  // EXP-783: `seq` is the publisher's own monotonic index, echoed by the
  // relay; absent from a publisher older than EXP-783.
  | { t: `activity`; event: ActivityEvent; seq?: number }
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
      firstSeq?: number
      lastSeq?: number
      truncated?: boolean
    }
  // EXP-783: one page of OLDER transcript, answering this viewer's
  // `history_page`. PREPENDED, never appended.
  | {
      t: `history_chunk`
      requestId: string
      events: ActivityEvent[]
      seqs?: number[]
      done: boolean
    }
  // EXP-773: no live room, but the session's device is online — the relay
  // parked this viewer and asked the machine to republish the run's journal.
  // The transcript arrives as an ordinary replay, then a `bye`.
  | { t: `history_pending` }
  // EXP-648: the relay's liveness beat to joined viewers. Carries nothing;
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
  // EXP-773: the run is over and its transcript lives on the device — the
  // relay is fetching it. Rows stream in under this phase and the `bye`
  // that follows turns it into `ended`.
  | { kind: `history_pending`; detail?: string }
  // The session ended (relay `bye`, or the room was never live).
  | { kind: `ended`; detail?: string }
  // Unexpected socket loss — offer a manual Reconnect (fresh ticket).
  // `terminal` (EXP-648) marks a "no" a retry cannot turn into a "yes":
  // steering disabled on the instance, a mint refused for ownership or a
  // gone row, a ticket the relay rejected. The wakeup kicks leave those
  // alone instead of re-minting on every tab switch; the explicit Reconnect
  // button still tries.
  | { kind: `closed`; detail?: string; terminal?: boolean }

/** EXP-783: the publisher's monotonic index for the event behind a row, when
 *  it sent one. The only monotonic anchor on the wire — it is what lets a join
 *  replay be spliced onto a transcript prefix already on screen, and what an
 *  older-page request is addressed relative to. */
interface FeedSeq {
  seq?: number
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
      /** EXP-785: that status was `failed` (a later `completed` clears it).
       *  Group captions list these last. */
      failed?: boolean
      /** EXP-786: the per-call unified diff an `edit` published, already cut
       *  to the contract's caps by the publisher. */
      diff?: string
    }
  | { id: number; kind: `user_message`; text: string; subagentId?: string }
  | { id: number; kind: `permission`; tool: string; detail?: string }
  | {
      id: number
      kind: `subagent`
      subagentId: string
      agentType: string
      status: `started` | `completed`
      detail?: string
      /** EXP-748: the publisher's own tool-call count for the subagent (the
       *  completed edge carries it); absent on the started edge and on older
       *  publishers. `summarizeSubagentRow` prefers it over the visible rows. */
      toolCalls?: number
    }
  | {
      id: number
      kind: `question`
      text: string
      options: QuestionOption[]
      multiSelect: boolean
      planMode: boolean
      /** Wire identity (protocol v2) — absent on cards from a desktop too
       *  old to publish ids, which render read-only (EXP-672). */
      questionId?: string
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
 *  union to its common keys, losing the per-kind fields). */
type NewFeedItem = FeedItem extends infer T
  ? T extends FeedItem
    ? Omit<T, `id`>
    : never
  : never

/** A composer image pending upload — the draft survives disconnects and view
 *  unmounts, so `uploadedId` also persists a mid-batch upload across them and
 *  a retry only uploads the rest. */
export interface PendingSteerImage {
  file: File
  url: string
  uploadedId?: string
}

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
  /** EXP-784: the agent's rate-limit window, the fourth slot. Null = not
   *  limited (or cleared by an empty/`ok` status). */
  rateLimit: SessionRateLimitState | null
  answerStates: AnswerStates
  /** The socket is actually open. Distinct from the phase: a silent
   *  slow-consumer redial keeps `phase: live` while the socket is briefly
   *  down, and send affordances should dim honestly for that gap. */
  connected: boolean
  /** EXP-783: there is transcript BELOW the oldest row on screen, and this
   *  client can ask the device for it — what the transcript's "Load earlier"
   *  affordance is gated on. */
  canLoadEarlier: boolean
}

export interface SteerDraftSnapshot {
  text: string
  images: PendingSteerImage[]
}

export interface AddDraftImagesResult {
  /** Files refused for type/size (the "images up to 10 MB" toast). */
  rejected: number
  /** Accepted files dropped over MAX_STEER_IMAGES (the "up to N" toast). */
  overflow: number
  /** How many files actually joined the strip — the composer numbers its
   *  `[Image #N]` markers from the strip length it already knows (EXP-698). */
  added: number
}

interface SteerStoreDeps {
  mintTicket: (codingSessionId: string) => Promise<
    { disabled: true } | { ticket: string; url: string }
  >
  createSocket: (url: string) => WebSocket
}

const defaultDeps: SteerStoreDeps = {
  mintTicket: (codingSessionId) =>
    trpc.steer.mintTicket.mutate(
      { kind: `viewer`, sessionId: codingSessionId },
      { context: { skipErrorToast: true } }
    ) as Promise<{ disabled: true } | { ticket: string; url: string }>,
  createSocket: (url) => new WebSocket(url),
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
   *  relay's own signals plus the dock reaper. */
  noteSessionStatus(status: CodingSession[`status`]): void
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
  setDraftText(text: string): void
  addDraftImages(files: File[]): AddDraftImagesResult
  removeDraftImage(url: string): void
  setDraftImageUploaded(url: string, uploadedId: string): void
  clearDraftAfterSend(): void
  dispose(): void
}

interface InternalSteerSessionStore extends SteerSessionStore {
  /** Registry internals (reaper bookkeeping). */
  _subscriberCount(): number
  _scheduleReap(): void
  _cancelReap(): void
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
  let sessionStatus: CodingSession[`status`] | null = null
  /** EXP-773: the host machine's label, for the history phases' copy. The
   *  owning view feeds it off the synced devices row. */
  let deviceLabel: string | null = null
  // EXP-625: dial liveness, so a wakeup can tell a dial that is merely young
  // from one that is stuck. Both timers are generation-scoped.
  let mintTimer: ReturnType<typeof setTimeout> | null = null
  let joinAckTimer: ReturnType<typeof setTimeout> | null = null
  let dialStartedAt = 0
  let lastFrameAt = 0

  let phase: ViewerPhase = { kind: `idle` }
  let feed: FeedItem[] = []
  /** EXP-783: the running weight of `feed`, so the byte budget is an integer
   *  compare per append rather than a walk. */
  let feedBytes = 0
  /** EXP-783: the wire sequence of the event being folded in right now, so
   *  `append` can stamp it without threading it through every case. */
  let currentSeq: number | undefined
  let latestDiff: string | null = null
  let compacting: CompactionState | null = null
  let config: SessionConfigState | null = null
  let usage: SessionUsageState | null = null
  let rateLimit: SessionRateLimitState | null = null
  let compactionTimer: ReturnType<typeof setTimeout> | null = null
  let answerStates: AnswerStates = {}
  let connected = false
  let nextId = 0
  /** Locally-echoed sent messages awaiting their transcript-derived event. */
  const recentEchoes: EchoEntry[] = []
  /** Per-card `answer_ack` deadlines (see ANSWER_ACK_TIMEOUT_MS). */
  const ackTimers = new Map<string, ReturnType<typeof setTimeout>>()
  /** EXP-751: the replay being staged, or null when nothing is staging. Holds
   *  every `activity` event since the last `activity_reset`, in arrival
   *  order; the visible feed is folded from them in one go at commit. */
  let staged: { event: ActivityEvent; seq?: number }[] | null = null
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
    rateLimit,
    answerStates,
    connected,
    canLoadEarlier: false,
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
      rateLimit,
      answerStates,
      connected,
      // EXP-796: a page can only be asked for over an OPEN socket — once the
      // relay closed the (lingering) history room, the button goes.
      canLoadEarlier: historyTruncated && !historyExhausted && connected,
    }
    notify()
  }
  const commitDraft = () => {
    draftSnapshot = { text: draftText, images: draftImages }
    notify()
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

  /** EXP-625: drop both dial deadlines. Every path that abandons a dial
   *  (a new dial, reconnect, dispose, the socket closing) goes through here. */
  const clearDialTimers = () => {
    if (mintTimer) {
      clearTimeout(mintTimer)
      mintTimer = null
    }
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
    // EXP-773: a history republish streams the same frames a live room does,
    // but the run is over — the caption keeps saying so until the marker.
    if (phase.kind === `history_pending`) return false
    // EXP-796: an ended phase never lifts back to live on the same dial — a
    // lingering history room's keepalives and pages are not a run, and the
    // synced row stays the source of truth (a redial resets the phase).
    if (phase.kind === `ended`) return false
    phase = { kind: `live` }
    return true
  }

  /** EXP-796: the device's replay is fully in. The relay keeps the history
   *  room OPEN for a while (no `bye`, keepalives, "Load earlier" pages down
   *  the device's control socket), so the phase moves off "loading" here
   *  rather than at a close: an ENDED row is over — the transcript is on
   *  screen, read-only, the socket lingers only for pages — while a row the
   *  server still calls alive waits for its publisher, whose takeover of the
   *  lingering room lands on this same socket as an ordinary replay. */
  const markHistoryServed = (): boolean => {
    if (phase.kind !== `history_pending`) return false
    retries = 0
    phase = sessionStatus === `ended` ? { kind: `ended` } : { kind: `starting` }
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
      case `tool`: {
        const detail = event.detail?.trim() ? event.detail : undefined
        append({
          kind: `tool`,
          name: event.name,
          detail,
          subagentId: event.subagentId,
          callId: event.id?.trim() ? event.id : undefined,
          toolKind: parseToolKind(event.toolKind),
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
        }
        if (typeof event.diff === `string` && event.diff.trim()) next.diff = event.diff
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
        append({
          kind: `user_message`,
          text: event.text,
          subagentId: event.subagentId,
        })
        return
      }
      case `question`: {
        if (!event.text.trim() || !event.options?.length) return
        const item: Omit<QuestionItem, `id`> = {
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
        // desktop augments options as it learns them).
        const replaced = event.id ? upsertQuestion(feed, event.id, item) : null
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
          status: event.status === `completed` ? `completed` : `started`,
          detail: event.detail?.trim() ? event.detail : undefined,
          toolCalls:
            typeof event.toolCalls === `number` ? event.toolCalls : undefined,
        })
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
      case `rate_limit`: {
        // EXP-784: the fourth slot. Null clears — an empty/`ok` status says
        // the window lifted, and an unreadable payload must not leave a
        // stale "rate limited" banner beside a live run.
        rateLimit = parseRateLimit(event)
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
  const stageEvent = (event: ActivityEvent, seq?: number) => {
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
    // scrolled back to load, which the full swap used to throw away. Kept
    // only when the WHOLE prefix is numbered: an unnumbered row cannot be
    // proved older than the replay, so one of them makes this the full swap
    // it has always been.
    let retained: FeedItem[] = []
    if (firstSeq !== undefined) {
      let split = 0
      while (
        split < feed.length &&
        feed[split].seq !== undefined &&
        (feed[split].seq as number) < firstSeq
      ) {
        split++
      }
      retained = feed.slice(0, split)
    }
    const retainedNextId =
      retained.length > 0 ? retained[retained.length - 1].id + 1 : undefined
    feed = []
    feedBytes = 0
    latestDiff = null
    config = null
    usage = null
    rateLimit = null
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
      currentSeq = undefined
      foldingReplay = false
    }
    for (const text of echoes) {
      if (!tailCarriesEcho(text, echoes.length + 1)) {
        append({ kind: `user_message`, text })
      }
    }
    const liveKeys = new Set<string>()
    for (const item of feed) {
      if (item.kind === `question` && item.questionId !== undefined) {
        liveKeys.add(item.questionId)
      }
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
   *  request is asked relative to. `undefined` when nothing is numbered
   *  (every publisher older than EXP-783), which is also the signal that
   *  paging is unavailable for this run. */
  const oldestSeq = (): number | undefined => {
    for (const item of feed) if (item.seq !== undefined) return item.seq
    return undefined
  }

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
    feed = []
    feedBytes = 0
    nextId = 0
    foldingReplay = true
    try {
      events.forEach((event, ix) => {
        const seq = seqs[ix]
        if (oldest !== undefined && seq !== undefined && seq >= oldest) return
        currentSeq = seq
        handleActivity(event)
      })
    } finally {
      currentSeq = undefined
      foldingReplay = savedFolding
    }
    const page = feed
    feed = savedFeed
    feedBytes = savedBytes
    nextId = savedNextId
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
    | { t: `event`; event: ActivityEvent; seq?: number }
    | { t: `synced`; firstSeq?: number }
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
            currentSeq = undefined
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

  const dial = async (retrying: boolean) => {
    if (disposed) return
    const gen = ++generation
    clearDialTimers()
    // A superseded socket's HANDLERS are inert (the generation gate), but the
    // socket is not: left open it stays joined at the relay as a duplicate
    // viewer until the room closes. Every dial abandons its predecessor, so
    // the close belongs here — that makes every caller safe, `kick`'s
    // `starting` retry (which dials straight over an in-flight dial) included.
    if (ws) {
      ws.close()
      ws = null
      // A replay the abandoned socket never finished delivering is a partial
      // history of a room this dial is leaving — keep what the reader sees.
      discardStaging()
      // A page asked for on that socket is not coming either (EXP-795): the
      // relay drops the ask with the viewer, so a new one may go out.
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

    // `bye` / no_such_session must win over the generic close handler.
    let sawEnd = false
    let retryStarting = false
    // EXP-773: a history error a redial can only repeat — the close it
    // precedes is terminal, so the wakeup kicks leave it alone.
    let terminalError = false
    // EXP-773: this dial was parked on a device transcript. On a run the
    // synced row still calls live that answer is about the JOURNAL, never
    // about the run: the publisher just has not hello'd yet.
    let sawHistoryPending = false
    let detail: string | null = null

    try {
      // EXP-625: race the mint against its deadline. The attached noop catch
      // keeps a LATE rejection (after the deadline already won) from
      // surfacing as an unhandled rejection. The race still sees it while
      // it is the pending outcome.
      const minting = deps.mintTicket(sessionId)
      minting.catch(() => {})
      const minted = await Promise.race([
        minting,
        new Promise<typeof MINT_TIMED_OUT>((resolve) => {
          mintTimer = setTimeout(() => {
            mintTimer = null
            resolve(MINT_TIMED_OUT)
          }, MINT_TIMEOUT_MS)
        }),
      ])
      if (disposed || gen !== generation) return
      if (mintTimer) {
        clearTimeout(mintTimer)
        mintTimer = null
      }
      if (minted === MINT_TIMED_OUT) {
        phase = {
          kind: `closed`,
          detail: `Couldn't get a viewer ticket in time.`,
        }
        commit()
        return
      }
      if (`disabled` in minted && minted.disabled) {
        phase = {
          kind: `closed`,
          detail: `Live steering is unavailable on this instance.`,
          terminal: true,
        }
        commit()
        return
      }
      const { url } = minted as { ticket: string; url: string }

      const sock = deps.createSocket(url)
      ws = sock
      sock.onopen = () => {
        if (disposed || gen !== generation) return
        connected = true
        commit()
        // The feed is NEVER wiped here (protocol v2): the relay sends an
        // explicit `activity_reset` immediately before its join replay, so
        // a redial that never lands keeps showing what was already there.
        sock.send(JSON.stringify({ t: `join`, channel: `activity` }))
        // NOT live yet — the relay may answer the join with no_such_session
        // (desktop still starting). The phase flips to live on the first
        // confirming server frame instead (the relay sends activity_reset
        // immediately on a successful join).
        // EXP-625: the relay answers every join, so arm a deadline for that
        // answer. A socket that opened but stays mute (a stale connection a
        // suspended tab woke up with) is closed here, and the onclose path
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
        // Any frame at all proves the socket is alive (EXP-625).
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
            // the feed or re-render anything. The one exception: its 15s
            // cadence proves a staged replay burst is over, so while a
            // replay is staging (or a reset is still queued ahead of it) it
            // rides the queue to end it.
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
            // EXP-656: the relay's end-of-replay marker — the join succeeded
            // and the staged replay commits.
            // EXP-783: it names the span, so the commit keeps the pages this
            // client had already scrolled back to load.
            const f = frame as Extract<ServerFrame, { t: `activity_synced` }>
            historyTruncated = f.truncated === true
            activityQueue.enqueue({ t: `synced`, firstSeq: f.firstSeq })
            if (markHistoryServed()) {
              commit()
              if (phase.kind === `ended`) onEnded()
            } else if (markLive()) {
              commit()
            }
            return
          }
          // EXP-783: one page of older transcript. It goes in FRONT of
          // everything on screen, so it rides the same queue (order with the
          // live tail matters) and never touches the staging buffer.
          case `history_chunk`: {
            const f = frame as Extract<ServerFrame, { t: `history_chunk` }>
            if (f.requestId !== historyRequest) return
            if (f.done) historyRequest = null
            activityQueue.enqueue({
              t: `page`,
              events: f.events,
              seqs: f.seqs ?? [],
            })
            return
          }
          case `history_pending`: {
            // EXP-773: the relay parked this viewer and asked the device for
            // the run's journal. Never a redial state — the relay answers
            // with the transcript, `history_unavailable` or `device_offline`.
            sawHistoryPending = true
            phase = {
              kind: `history_pending`,
              detail: `Fetching the transcript from ${deviceLabel ?? `the device`}…`,
            }
            commit()
            return
          }
          case `bye`: {
            const f = frame as Extract<ServerFrame, { t: `bye` }>
            if (f.outcome === `publisher_lost`) {
              // The desktop's relay socket dropped but the session may still
              // be running — the synced row is the truth. Stay retryable.
              detail = `The desktop's connection to the relay dropped. Retry once it reconnects.`
            } else {
              sawEnd = true
              // EXP-773: `history` is the journal republish closing itself
              // out — the feed stays, with the plain ended caption. The two
              // history failures arrive as an `error` frame FIRST and then as
              // this outcome; the caption that arm wrote is the human one, so
              // it is never overwritten with the raw code.
              if (!terminalError) {
                detail =
                  f.outcome &&
                  f.outcome !== `ended` &&
                  f.outcome !== `history` &&
                  f.outcome !== `history_unavailable` &&
                  f.outcome !== `device_offline`
                    ? f.outcome
                    : null
              }
            }
            return
          }
          case `error`: {
            const f = frame as Extract<ServerFrame, { t: `error` }>
            if (f.code === `no_such_session`) {
              // Not live on the relay (yet) — auto-retry while the synced
              // row still says running.
              detail = `The live stream isn't up yet. The desktop may still be connecting.`
              retryStarting = true
              sock.close()
            } else if (f.code === `device_offline`) {
              // EXP-773: the transcript is a FILE on that machine — no
              // retry here can conjure it up, so this close is terminal.
              detail = `${deviceLabel ?? `The device`} is offline. The transcript lives on that machine.`
              terminalError = true
            } else if (f.code === `history_unavailable`) {
              detail = `No transcript on ${deviceLabel ?? `the device`}.`
              terminalError = true
            } else {
              detail = f.message ?? f.code
            }
            return
          }
          default:
            // Unknown frames from a newer relay are inert.
            return
        }
      }
      sock.onclose = (event) => {
        if (disposed || gen !== generation) return
        clearDialTimers()
        ws = null
        connected = false
        // A half-delivered replay is worth less than the last complete
        // picture — the reader keeps what they were reading (EXP-656).
        discardStaging()
        // Nor is an in-flight page ask answered on a closed socket (EXP-795).
        historyRequest = null
        // EXP-773: the relay opens a pending history room for ANY join it
        // has no live room for, so a run whose publisher is still connecting
        // gets a device answer instead of `no_such_session`: a partial
        // transcript, `history_unavailable` or `device_offline`, each closing
        // the socket. None of them is an ending while the synced row says the
        // run is alive — go back to `starting` and redial for the publisher.
        if (sawHistoryPending && sessionStatus !== `ended`) {
          phase = { kind: `starting` }
          commit()
          scheduleRedial()
          return
        }
        if (sawEnd) {
          phase = { kind: `ended`, detail: detail ?? undefined }
          // A run that ended mid-fold is not compacting any more (EXP-724).
          clearCompaction()
          commit()
          onEnded()
          return
        }
        if (retryStarting) {
          // An `in_review` terminal is still alive and steerable (EXP-194)
          // — only a truly ended session stops the redial.
          if (sessionStatus !== `ended`) {
            phase = { kind: `starting` }
            commit()
            scheduleRedial()
          } else {
            phase = { kind: `ended` }
            commit()
            onEnded()
          }
          return
        }
        // EXP-621: a slow-consumer eviction is not an ending — the session
        // is still live on the relay. Redial silently (the phase — usually
        // `live` — holds steady, so nothing flickers and the composer never
        // loses its footing; the commit still surfaces `connected: false` so
        // the send button dims honestly for the gap); the shared backoff
        // bounds a pathological evict-redial loop.
        //
        // EXP-781: an ENDED row gets the same treatment once we have seen
        // `history_pending`. That replay is the device pushing a whole
        // journal in a burst, which is exactly what trips the eviction, and
        // the staging was just discarded above — without a redial the phase
        // lands in `closed`, where `kick()` refuses to recover it and the
        // reader is stranded on an empty transcript. Retrying is safe: the
        // next join replays from scratch.
        if (
          event.code === CLOSE_SLOW_CONSUMER &&
          (sessionStatus !== `ended` || sawHistoryPending)
        ) {
          commit()
          scheduleRedial()
          return
        }
        phase = {
          kind: `closed`,
          detail: detail ?? undefined,
          terminal: terminalError || event.code === CLOSE_UNAUTHORIZED,
        }
        commit()
      }
    } catch (error) {
      if (disposed || gen !== generation) return
      // A mint refused for ownership (FORBIDDEN) or for a row that is gone
      // (NOT_FOUND) is terminal: it stays `closed` rather than `ended` — a
      // reaped row is disposed by the registry's retention sweep anyway.
      // Everything else (a network failure, a 5xx) is worth a retry.
      const code = trpcErrorCode(error)
      phase = {
        kind: `closed`,
        detail: trpcErrorMessage(error, `Couldn't get a viewer ticket`),
        terminal: code === `FORBIDDEN` || code === `NOT_FOUND`,
      }
      commit()
    }
  }

  // ── Steering (message-shaped; owner-only — the mint refuses anyone else) ──

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

  /** Protocol v2 answer: the relay forwards it verbatim to the desktop, which
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

  /** EXP-790: cancel the agent's current turn. The relay forwards the frame
   *  to the publisher verbatim; the engine's `session/cancel` dismisses every
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
    kick(_reason) {
      if (disposed) return
      const current = phase
      switch (current.kind) {
        case `closed`:
          // An unexpected close stays terminal BY DESIGN until something
          // says the world changed, and a wakeup is exactly that. A
          // session the synced row calls ended is left alone, and so is a
          // close no retry can fix (EXP-648, see ViewerPhase) — that used to
          // cost one mint per visibility/online event per retained store.
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
          // EXP-796: unless a socket is OPEN and talking — a served history
          // room lingers for the publisher's takeover, which arrives on it;
          // redialing would only throw the transcript away and re-ask.
          // An in-flight dial (open, nothing heard yet) is still retried:
          // only a socket that has answered THIS dial recently is left be.
          if (
            connected &&
            lastFrameAt >= dialStartedAt &&
            Date.now() - lastFrameAt <= LIVE_STALE_MS
          )
            return
          clearRetryTimer()
          void dial(true)
          return
        case `live`:
          // EXP-648: live on paper over a socket that has said nothing for
          // longer than three relay keepalives is dead — redial under the
          // `live` phase (the 4008 mechanics) so a socket that turns out
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
        default:
          // `idle` and `ended` need nothing.
          return
      }
    },
    noteSessionStatus(status) {
      sessionStatus = status
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
      pushEcho(recentEchoes, text, Date.now())
      // Sent mid-replay: the staged history predates it, so the commit has
      // to put it back (unless the replay turns out to carry it).
      if (staged !== null) stagedEchoes.push(text)
      const row = { id: nextId++, kind: `user_message` as const, text }
      setFeed([...feed, row], [row])
      commit()
      return true
    },
    /** Submit a card's answer and LOCK it immediately — a locked card never
     *  fires again. `answer_ack` confirms the lock; `question_resolved`
     *  finalizes it; ANSWER_ACK_TIMEOUT_MS without either re-enables the card
     *  with an inline note. EXP-672: a card carrying no wire id is a no-op —
     *  an old desktop's card renders read-only, it is never answered blind. */
    answerQuestion(item, keys, labels, text) {
      if (item.questionId === undefined) return
      const key = answerKey(item)
      if (isAnswerLocked(answerStates[key]) || item.resolved === true) return
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
    interrupt() {
      return sendInterruptFrame()
    },
    setDraftText(text) {
      draftText = text
      commitDraft()
    },
    addDraftImages(files) {
      const accepted = files.filter(
        (file) =>
          isAcceptedImageContentType(file.type) &&
          file.size <= maxImageUploadBytes
      )
      const room = Math.max(0, MAX_STEER_IMAGES - draftImages.length)
      const taking = accepted.slice(0, room)
      if (taking.length > 0) {
        draftImages = [
          ...draftImages,
          ...taking.map((file) => ({ file, url: URL.createObjectURL(file) })),
        ]
        commitDraft()
      }
      return {
        rejected: files.length - accepted.length,
        overflow: accepted.length - taking.length,
        added: taking.length,
      }
    },
    removeDraftImage(url) {
      URL.revokeObjectURL(url)
      draftImages = draftImages.filter((image) => image.url !== url)
      commitDraft()
    },
    setDraftImageUploaded(url, uploadedId) {
      draftImages = draftImages.map((image) =>
        image.url === url ? { ...image, uploadedId } : image
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
