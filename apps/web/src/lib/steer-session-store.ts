import { trpc } from "@/lib/trpc-client"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import {
  ackAnswer,
  answerKey,
  applyQuestionResolved,
  beginAnswer,
  clearAnswer,
  consumeEcho,
  createActivityCoalescer,
  failAnswer,
  isAnswerLocked,
  pushEcho,
  spliceBeforeQuestion,
  upsertQuestion,
  ANSWER_ACK_TIMEOUT_MS,
  FEED_CAP,
  type AnswerStates,
  type EchoEntry,
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
// composer draft all live here, so collapsing the dock, switching tabs or
// navigating around the app detaches the VIEW without dropping the
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
/** A disposal grace once a store is neither kept by the dock's reaper nor
 *  subscribed — long enough to survive transient empty live-query results
 *  and dock remounts, which must never kill a background socket. */
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
  /** Raw keystroke that selects this option in the desktop TUI picker — also
   *  the token the semantic `answer` frame carries back. */
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
  | { kind: `narration`; text: string; beforeQuestionId?: string; at?: number }
  // `subagentId` (protocol v2) nests the call under its subagent group.
  | { kind: `tool`; name: string; detail?: string; subagentId?: string; at?: number }
  | { kind: `diff`; diff: string; at?: number }
  // EXP-78 (member-only on the relay): a human turn from the transcript…
  | { kind: `user_message`; text: string; at?: number }
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
      at?: number
    }
  | { kind: `permission`; tool: string; detail?: string; at?: number }

type ServerFrame =
  | { t: `activity`; event: ActivityEvent }
  // Protocol v2: "clear your feed now" — sent before every join replay and
  // whenever the desktop re-publishes its full history.
  | { t: `activity_reset` }
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
  // The session ended (relay `bye`, or the room was never live).
  | { kind: `ended`; detail?: string }
  // Unexpected socket loss — offer a manual Reconnect (fresh ticket).
  // `terminal` (EXP-648) marks a "no" a retry cannot turn into a "yes":
  // steering disabled on the instance, a mint refused for ownership or a
  // gone row, a ticket the relay rejected. The wakeup kicks leave those
  // alone instead of re-minting on every tab switch; the explicit Reconnect
  // button still tries.
  | { kind: `closed`; detail?: string; terminal?: boolean }

export type FeedItem =
  | { id: number; kind: `narration`; text: string }
  | { id: number; kind: `tool`; name: string; detail?: string; subagentId?: string }
  | { id: number; kind: `user_message`; text: string }
  | { id: number; kind: `permission`; tool: string; detail?: string }
  | {
      id: number
      kind: `subagent`
      subagentId: string
      agentType: string
      status: `started` | `completed`
      detail?: string
    }
  | {
      id: number
      kind: `question`
      text: string
      options: QuestionOption[]
      multiSelect: boolean
      planMode: boolean
      /** Wire identity (protocol v2) — absent on legacy cards. */
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

export type QuestionItem = Extract<FeedItem, { kind: `question` }>

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

export interface SteerSessionSnapshot {
  phase: ViewerPhase
  feed: FeedItem[]
  latestDiff: string | null
  answerStates: AnswerStates
  /** The socket is actually open. Distinct from the phase: a silent
   *  slow-consumer redial keeps `phase: live` while the socket is briefly
   *  down, and send affordances should dim honestly for that gap. */
  connected: boolean
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
      { kind: `viewer`, codingSessionId },
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
  /** The synced row is the truth for "still running" inside the redial
   *  loops — the OWNING view feeds it; an unwatched store falls back to the
   *  relay's own signals plus the dock reaper. */
  noteSessionStatus(status: CodingSession[`status`]): void
  sendMessage(text: string): boolean
  answerQuestion(
    item: QuestionItem,
    keys: string[],
    labels: string[],
    text?: string
  ): void
  toggleLegacyOption(key: string): void
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
  // EXP-625: dial liveness, so a wakeup can tell a dial that is merely young
  // from one that is stuck. Both timers are generation-scoped.
  let mintTimer: ReturnType<typeof setTimeout> | null = null
  let joinAckTimer: ReturnType<typeof setTimeout> | null = null
  let dialStartedAt = 0
  let lastFrameAt = 0

  let phase: ViewerPhase = { kind: `idle` }
  let feed: FeedItem[] = []
  let latestDiff: string | null = null
  let answerStates: AnswerStates = {}
  let connected = false
  let nextId = 0
  /** Locally-echoed sent messages awaiting their transcript-derived event. */
  const recentEchoes: EchoEntry[] = []
  /** Per-card `answer_ack` deadlines (see ANSWER_ACK_TIMEOUT_MS). */
  const ackTimers = new Map<string, ReturnType<typeof setTimeout>>()

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
    answerStates,
    connected,
  }
  let draftSnapshot: SteerDraftSnapshot = { text: draftText, images: draftImages }

  const notify = () => {
    for (const listener of listeners) listener()
  }
  const commit = () => {
    snapshot = { phase, feed, latestDiff, answerStates, connected }
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

  /** Returns whether the phase actually changed — a per-frame commit on an
   *  already-live store would notify (and re-render) once per replayed
   *  message, undoing the REV-33 coalescing. */
  const markLive = (): boolean => {
    retries = 0
    if (phase.kind === `live`) return false
    phase = { kind: `live` }
    return true
  }

  /** `activity_reset`: the relay/desktop is about to (re)publish the whole
   *  history — everything derived from the old feed goes with it. */
  const resetFeed = () => {
    for (const timer of ackTimers.values()) clearTimeout(timer)
    ackTimers.clear()
    feed = []
    latestDiff = null
    answerStates = {}
    // After a reset the replayed transcript event is the ONLY copy of a sent
    // message and must render.
    recentEchoes.length = 0
  }

  const append = (item: NewFeedItem) => {
    feed = [...feed, { ...item, id: nextId++ } as FeedItem].slice(-FEED_CAP)
  }

  const handleActivity = (event: ActivityEvent) => {
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
          }
          feed = (spliceBeforeQuestion(feed, anchor, item) ?? [...feed, item]).slice(
            -FEED_CAP
          )
          return
        }
        append({ kind: `narration`, text: event.text })
        return
      }
      case `tool`: {
        const detail = event.detail?.trim() ? event.detail : undefined
        append({
          kind: `tool`,
          name: event.name,
          detail,
          subagentId: event.subagentId,
        })
        return
      }
      case `user_message`: {
        if (!event.text.trim()) return
        // A message this client just sent was already echoed locally — skip
        // its transcript-derived twin.
        if (consumeEcho(recentEchoes, event.text, Date.now())) return
        append({ kind: `user_message`, text: event.text })
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
        feed =
          replaced ?? [...feed, { ...item, id: nextId++ }].slice(-FEED_CAP)
        return
      }
      case `question_resolved`: {
        feed = applyQuestionResolved(feed, event) ?? feed
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

  // REV-33: a join replay fans the relay's whole activity log (up to
  // FEED_CAP frames) out as individual ws messages. Handling each one
  // directly meant one notify per frame over the full non-virtualized feed
  // — O(n²) work that froze the tab on open/reconnect. Frames buffer here
  // and apply in one synchronous pass per window instead; `activity_reset`
  // rides the same queue so a reset can never overtake buffered frames.
  // The queue outlives redials (order is preserved across them) and only
  // dispose cancels it.
  const activityQueue = createActivityCoalescer<
    { t: `reset` } | { t: `event`; event: ActivityEvent }
  >((batch) => {
    if (disposed) return
    for (const op of batch) {
      if (op.t === `reset`) resetFeed()
      else handleActivity(op.event)
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
            activityQueue.enqueue({ t: `event`, event: f.event })
            if (markLive()) commit()
            return
          }
          case `keepalive`:
            // EXP-648: already counted by the `lastFrameAt` stamp above.
            // Never a phase change and never a commit — it must not touch
            // the feed or re-render anything.
            return
          case `activity_reset`: {
            activityQueue.enqueue({ t: `reset` })
            if (markLive()) commit()
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
              detail = f.outcome && f.outcome !== `ended` ? f.outcome : null
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
            } else {
              detail = f.message ?? f.code
            }
            return
          }
          default:
            return
        }
      }
      sock.onclose = (event) => {
        if (disposed || gen !== generation) return
        clearDialTimers()
        ws = null
        connected = false
        if (sawEnd) {
          phase = { kind: `ended`, detail: detail ?? undefined }
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
        if (event.code === CLOSE_SLOW_CONSUMER && sessionStatus !== `ended`) {
          commit()
          scheduleRedial()
          return
        }
        phase = {
          kind: `closed`,
          detail: detail ?? undefined,
          terminal: event.code === CLOSE_UNAUTHORIZED,
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

  /** Legacy answer path (a desktop that publishes no question ids): raw
   *  keystrokes — the desktop passes single-byte frames to the PTY unwrapped,
   *  so the TUI sees keypresses, not a paste. NO trailing `\r`: a digit
   *  already selects AND advances in claude's picker, so the extra return
   *  cascaded into the next question and auto-answered it (EXP-249).
   *  Multi-select taps toggle with the digit alone; Continue sends `\t`. */
  const sendKeystrokes = (keys: string[]): boolean => {
    if (ws?.readyState !== WebSocket.OPEN) return false
    for (const key of keys) ws.send(JSON.stringify({ t: `input`, data: key }))
    return true
  }

  /** Protocol v2 answer: the relay forwards it verbatim to the desktop, which
   *  drives its own picker and confirms with `answer_ack`. */
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
      feed = [
        ...feed,
        { id: nextId++, kind: `user_message` as const, text },
      ].slice(-FEED_CAP)
      commit()
      return true
    },
    /** Submit a card's answer and LOCK it immediately — a locked card never
     *  fires again. `answer_ack` confirms the lock; `question_resolved`
     *  finalizes it; ANSWER_ACK_TIMEOUT_MS without either re-enables the card
     *  with an inline note. */
    answerQuestion(item, keys, labels, text) {
      const key = answerKey(item)
      if (isAnswerLocked(answerStates[key]) || item.resolved === true) return
      const sent = item.questionId
        ? sendAnswerFrame(item.questionId, item.askId, keys, text)
        : sendKeystrokes(keys)
      if (!sent) return
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
    /** A legacy multi-select toggle — one keystroke, no lock: the selection is
     *  only submitted by Continue. */
    toggleLegacyOption(key) {
      sendKeystrokes([key])
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
      // Grace before reaping: transient empty live-query results and dock
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
