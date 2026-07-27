import { useEffect, useMemo, useRef, useState } from "react"
import { toast } from "sonner"
import { TRPCClientError } from "@trpc/client"
import {
  ArrowDown,
  ArrowUp,
  Bot,
  Check,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  ClipboardList,
  Loader2,
  Maximize2,
  Minimize2,
  OctagonX,
  RotateCw,
  ShieldQuestion,
  Sparkles,
  Wrench,
  X,
} from "lucide-react"
import type { CodingSession } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  ackAnswer,
  activeQuestionIds,
  answerKey,
  applyQuestionResolved,
  askStepperView,
  attachQuestionAnswer,
  beginAnswer,
  clearAnswer,
  consumeEcho,
  dismissPendingQuestions,
  failAnswer,
  groupFeedRows,
  hasSemanticQuestions,
  isAnswerLocked,
  pushEcho,
  upsertQuestion,
  ANSWER_ACK_TIMEOUT_MS,
  FEED_CAP,
  PLAN_RESOLVED_NARRATION,
  QUESTION_ANSWERED_PREFIX,
  QUESTION_DISMISSED_NARRATION,
  type AnswerState,
  type AnswerStates,
  type EchoEntry,
} from "@/lib/agent-feed"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { splitUnifiedDiff } from "@/lib/unified-diff"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { FileDiffList } from "@/components/diff-view"

// The custom-rendered agent-session viewer (EXP-63 — the web port of the
// mobile "Agent session" chat view, EXP-32). NO terminal rendering: the
// viewer joins the steer relay's scrubbed ACTIVITY channel
// ({"t":"join","channel":"activity"}, apps/steer-relay/src/protocol.ts) and
// renders structured events — narration bubbles + compact tool rows, a
// pinned "Latest changes" diff above the composer — never raw PTY bytes.
// Steering is message-shaped like mobile — chunked input + a SEPARATE `\r`
// frame, perm-gated by the relay (EXP-312: no operator claim, fully
// seamless); question answers ride the semantic `answer` frame (steer
// protocol v2, EXP-249) whenever the desktop publishes question ids.
// Since EXP-106 this view is mounted ONLY by the global agent dock
// (components/agent-dock) — one at a time — so it always auto-connects and
// delegates its chrome (title, collapse) to the dock; the "coding now" rows +
// remote-start affordances moved to issue-coding-rows.tsx.

// ── Wire protocol (activity-viewer side of apps/steer-relay/src/protocol.ts) ─

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
const INPUT_CHUNK_CHARS = 4096
/** Redial backoff while the desktop's publisher socket is still starting:
 *  3s doubling to 30s. Each redial mints a fresh ticket and opens a fresh
 *  relay socket, so a fixed cadence across many waiting viewers would eat the
 *  relay's per-IP connect budget in lockstep. */
const STARTING_RETRY_BASE_MS = 3_000
const STARTING_RETRY_MAX_MS = 30_000

/** Equal jitter (half fixed, half random) — desynchronizes viewers that
 *  started waiting together while keeping a floor on the delay. */
function startingRetryDelay(retries: number): number {
  const capped = Math.min(
    STARTING_RETRY_BASE_MS * 2 ** retries,
    STARTING_RETRY_MAX_MS
  )
  return capped / 2 + Math.random() * (capped / 2)
}

interface QuestionOption {
  label: string
  /** Raw keystroke that selects this option in the desktop TUI picker — also
   *  the token the semantic `answer` frame carries back. */
  key: string
  /** Claude's per-option blurb (protocol v2), rendered under the label. */
  description?: string
}

type ActivityEvent =
  | { kind: `narration`; text: string; at?: number }
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

// The ticket is `base64url(JSON claims).base64url(sig)` — the claims carry the
// caller's perm (view|steer), which decides whether steering/kill controls
// show. Decoding locally is display-only; the relay enforces perm server-side.
function decodeTicketPerm(ticket: string): `view` | `steer` {
  try {
    const payload = ticket.slice(0, ticket.indexOf(`.`))
    const b64 = payload.replace(/-/g, `+`).replace(/_/g, `/`)
    const claims = JSON.parse(atob(b64)) as { perm?: string }
    return claims.perm === `steer` ? `steer` : `view`
  } catch {
    return `view`
  }
}

function trpcErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof TRPCClientError) {
    const message = error.message?.trim()
    if (message && !message.startsWith(`[`) && !message.startsWith(`{`)) {
      return message
    }
  }
  return fallback
}

// ── steer.config, fetched once per app lifetime (env-derived, static) ─────────

interface SteerConfig {
  enabled: boolean
  relayUrl: string | null
}

let steerConfigPromise: Promise<SteerConfig> | null = null

function fetchSteerConfigOnce(): Promise<SteerConfig> {
  steerConfigPromise ??= trpc.steer.config.query().catch((error) => {
    steerConfigPromise = null
    throw error
  })
  return steerConfigPromise
}

// Exported for the team Agents page, which gates its Watch controls on
// the same relay availability signal.
export function useSteerConfig(): SteerConfig | null {
  const [config, setConfig] = useState<SteerConfig | null>(null)
  useEffect(() => {
    let active = true
    fetchSteerConfigOnce()
      .then((c) => active && setConfig(c))
      // Treat an unreachable config proc as "steer off" — the badge still shows.
      .catch(() => active && setConfig({ enabled: false, relayUrl: null }))
    return () => {
      active = false
    }
  }, [])
  return config
}

// ── The agent-session view: structured activity feed over the relay ─────────

type ViewerPhase =
  | { kind: `idle` }
  | { kind: `connecting` }
  // no_such_session while the synced row still says running — the desktop is
  // still dialing its publisher socket; the view auto-redials with jittered
  // backoff (3s → 30s).
  | { kind: `starting` }
  | { kind: `live` }
  // The session ended (relay `bye`, or the room was never live).
  | { kind: `ended`; detail?: string }
  // Unexpected socket loss — offer a manual Reconnect (fresh ticket).
  | { kind: `closed`; detail?: string }

type FeedItem =
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

type QuestionItem = Extract<FeedItem, { kind: `question` }>

/** `Omit` that distributes over the FeedItem union (plain `Omit` collapses a
 *  union to its common keys, losing the per-kind fields). */
type NewFeedItem = FeedItem extends infer T
  ? T extends FeedItem
    ? Omit<T, `id`>
    : never
  : never

// Mounted ONLY by the global agent dock (one at a time), keyed by session id.
// Always auto-connects; the caller owns the membership + config.enabled gating
// (the relay enforces both regardless) and supplies the `title` + `onCollapse`
// chrome. Session-scoped — the "coding now" rows live in issue-coding-rows.tsx.
export function AgentSessionView({
  session,
  currentUserId,
  title,
  onCollapse,
  isFullscreen,
  onToggleFullscreen,
}: {
  session: CodingSession
  currentUserId: string
  /** Header identity — an issue-identifier Link, or plain text (batch/syncing). */
  title: React.ReactNode
  /** Collapse the dock panel (the socket tears down on unmount). */
  onCollapse: () => void
  /** Fullscreen toggle chrome (EXP-184) — owned by the dock; absent = no button. */
  isFullscreen?: boolean
  onToggleFullscreen?: () => void
}) {
  // Bumping `attempt` (re)runs the whole connect lifecycle with a fresh ticket.
  // Always starts at 1 — the dock only mounts this while it should be live.
  const [attempt, setAttempt] = useState(1)
  const [phase, setPhase] = useState<ViewerPhase>({ kind: `idle` })
  const [perm, setPerm] = useState<`view` | `steer`>(`view`)
  const [feed, setFeed] = useState<FeedItem[]>([])
  /** The most recent worktree diff — each one replaces the previous. */
  const [latestDiff, setLatestDiff] = useState<string | null>(null)
  const [diffOpen, setDiffOpen] = useState(false)
  const [confirmKill, setConfirmKill] = useState(false)
  const [killing, setKilling] = useState(false)
  const [atBottom, setAtBottom] = useState(true)
  /** Per-card answer locks, keyed by `answerKey` — a card locks the instant
   *  its answer goes out and only re-enables when the ack times out. */
  const [answerStates, setAnswerStates] = useState<AnswerStates>({})

  const wsRef = useRef<WebSocket | null>(null)
  const nextIdRef = useRef(0)
  /** Locally-echoed sent messages awaiting their transcript-derived event. */
  const recentEchoesRef = useRef<EchoEntry[]>([])
  /** Per-card `answer_ack` deadlines (see ANSWER_ACK_TIMEOUT_MS). */
  const ackTimersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>())
  const scrollRef = useRef<HTMLDivElement | null>(null)
  // The synced row is the truth for "still running" inside the redial loop.
  const sessionStatusRef = useRef(session.status)
  sessionStatusRef.current = session.status

  const clearAckTimer = (key: string) => {
    const timer = ackTimersRef.current.get(key)
    if (timer) {
      clearTimeout(timer)
      ackTimersRef.current.delete(key)
    }
  }

  /** `activity_reset`: the relay/desktop is about to (re)publish the whole
   *  history — everything derived from the old feed goes with it. */
  const resetFeed = () => {
    for (const timer of ackTimersRef.current.values()) clearTimeout(timer)
    ackTimersRef.current.clear()
    setFeed([])
    setLatestDiff(null)
    setAnswerStates({})
    // After a reset the replayed transcript event is the ONLY copy of a sent
    // message and must render.
    recentEchoesRef.current = []
  }

  useEffect(() => {
    if (attempt === 0) return

    let disposed = false
    let retryTimer: ReturnType<typeof setTimeout> | null = null
    let ws: WebSocket | null = null
    // Consecutive `starting` redials — drives the backoff; a live connection
    // resets it so the next stall starts fast again.
    let startingRetries = 0

    const markLive = () => {
      startingRetries = 0
      setPhase((prev) => (prev.kind === `live` ? prev : { kind: `live` }))
    }

    const append = (item: NewFeedItem) => {
      setFeed((prev) =>
        [...prev, { ...item, id: nextIdRef.current++ } as FeedItem].slice(
          -FEED_CAP
        )
      )
    }

    const handleActivity = (event: ActivityEvent) => {
      switch (event.kind) {
        case `narration`: {
          const trimmed = event.text.trim()
          if (!trimmed) return
          // Resolution narrations are the LEGACY signal (EXP-197): a protocol
          // v2 desktop emits them beside `question_resolved` purely for old
          // clients, so they are dropped once the feed carries question ids.
          // On a legacy feed they fold into the pending card instead of
          // rendering as a narration row; with no card waiting the answer
          // still renders, so it is never lost.
          if (trimmed.startsWith(QUESTION_ANSWERED_PREFIX)) {
            const answer = trimmed.slice(QUESTION_ANSWERED_PREFIX.length)
            setFeed((prev) => {
              if (hasSemanticQuestions(prev)) return prev
              return (
                attachQuestionAnswer(prev, answer) ??
                [
                  ...prev,
                  {
                    id: nextIdRef.current++,
                    kind: `narration` as const,
                    text: event.text,
                  },
                ].slice(-FEED_CAP)
              )
            })
            return
          }
          if (trimmed === QUESTION_DISMISSED_NARRATION) {
            setFeed((prev) =>
              hasSemanticQuestions(prev)
                ? prev
                : (dismissPendingQuestions(prev) ?? prev)
            )
            return
          }
          if (trimmed === PLAN_RESOLVED_NARRATION) {
            // Legacy feeds need it IN the feed — `activeQuestionIds` reads it
            // as the plan card's only retirement signal.
            setFeed((prev) =>
              hasSemanticQuestions(prev)
                ? prev
                : [
                    ...prev,
                    {
                      id: nextIdRef.current++,
                      kind: `narration` as const,
                      text: event.text,
                    },
                  ].slice(-FEED_CAP)
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
          if (consumeEcho(recentEchoesRef.current, event.text, Date.now()))
            return
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
          setFeed((prev) => {
            // A re-emission of a known id replaces the card in place (the
            // desktop augments options as it learns them).
            const replaced = event.id
              ? upsertQuestion(prev, event.id, item)
              : null
            return (
              replaced ??
              [...prev, { ...item, id: nextIdRef.current++ }].slice(-FEED_CAP)
            )
          })
          return
        }
        case `question_resolved`: {
          setFeed((prev) => applyQuestionResolved(prev, event) ?? prev)
          return
        }
        case `answer_ack`: {
          if (!event.id) return
          clearAckTimer(event.id)
          setAnswerStates((prev) => ackAnswer(prev, event.id))
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
          // behind the pinned "Latest changes" strip.
          setLatestDiff(event.diff.trim() ? event.diff : null)
          return
        }
        default:
          // Future kinds from a newer desktop: ignore, never crash the socket.
          return
      }
    }

    const dial = async (retrying: boolean) => {
      if (disposed) return
      // Hold the `starting` phase steady across auto-retry redials — flipping
      // to `connecting` per attempt makes the header flicker on every redial.
      if (!retrying) setPhase({ kind: `connecting` })

      // `bye` / no_such_session must win over the generic close handler.
      let sawEnd = false
      let retryStarting = false
      let detail: string | null = null

      try {
        const minted = await trpc.steer.mintTicket.mutate(
          { kind: `viewer`, codingSessionId: session.id },
          { context: { skipErrorToast: true } }
        )
        if (disposed) return
        if (`disabled` in minted && minted.disabled) {
          setPhase({
            kind: `closed`,
            detail: `Live steering is unavailable on this instance.`,
          })
          return
        }
        const { ticket, url } = minted as { ticket: string; url: string }
        setPerm(decodeTicketPerm(ticket))

        ws = new WebSocket(url)
        wsRef.current = ws
        ws.onopen = () => {
          if (disposed) return
          // The feed is NEVER wiped here (protocol v2): the relay sends an
          // explicit `activity_reset` immediately before its join replay, so
          // a redial that never lands keeps showing what was already there.
          ws?.send(JSON.stringify({ t: `join`, channel: `activity` }))
          // NOT live yet — the relay may answer the join with no_such_session
          // (desktop still starting). The phase flips to live on the first
          // confirming server frame instead (the relay sends activity_reset
          // immediately on a successful join).
        }
        ws.onmessage = (event) => {
          if (disposed || typeof event.data !== `string`) return
          const frame = parseServerFrame(event.data)
          if (!frame) return
          switch (frame.t) {
            case `activity`: {
              const f = frame as Extract<ServerFrame, { t: `activity` }>
              handleActivity(f.event)
              markLive()
              return
            }
            case `activity_reset`: {
              resetFeed()
              markLive()
              return
            }
            case `bye`: {
              const f = frame as Extract<ServerFrame, { t: `bye` }>
              if (f.outcome === `publisher_lost`) {
                // The desktop's relay socket dropped but the session may still
                // be running — the synced row is the truth. Stay retryable.
                detail = `The desktop's connection to the relay dropped — retry once it reconnects.`
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
                detail = `The live stream isn't up yet — the desktop may still be connecting.`
                retryStarting = true
                ws?.close()
              } else {
                detail = f.message ?? f.code
              }
              return
            }
            default:
              return
          }
        }
        ws.onclose = () => {
          if (disposed) return
          wsRef.current = null
          if (sawEnd) {
            setPhase({ kind: `ended`, detail: detail ?? undefined })
            return
          }
          if (retryStarting) {
            // An `in_review` terminal is still alive and steerable (EXP-194)
            // — only a truly ended session stops the redial.
            if (sessionStatusRef.current !== `ended`) {
              setPhase({ kind: `starting` })
              retryTimer = setTimeout(
                () => void dial(true),
                startingRetryDelay(startingRetries++)
              )
            } else {
              setPhase({ kind: `ended` })
            }
            return
          }
          setPhase({ kind: `closed`, detail: detail ?? undefined })
        }
      } catch (error) {
        if (disposed) return
        setPhase({
          kind: `closed`,
          detail: trpcErrorMessage(error, `Couldn't get a viewer ticket`),
        })
      }
    }

    void dial(false)

    return () => {
      disposed = true
      if (retryTimer) clearTimeout(retryTimer)
      wsRef.current = null
      ws?.close()
    }
  }, [attempt, session.id])

  // ── Steering (message-shaped; relay gates on the ticket's steer perm) ─────

  /**
   * Forward raw input (chunked ≤4 KiB, never splitting a surrogate pair).
   */
  const sendInput = (data: string): boolean => {
    const sock = wsRef.current
    if (perm !== `steer` || sock?.readyState !== WebSocket.OPEN) return false
    for (let i = 0; i < data.length; ) {
      let end = Math.min(i + INPUT_CHUNK_CHARS, data.length)
      const last = end < data.length ? data.charCodeAt(end - 1) : 0
      if (last >= 0xd800 && last <= 0xdbff) end += 1
      sock.send(JSON.stringify({ t: `input`, data: data.slice(i, end) }))
      i = end
    }
    return true
  }

  /**
   * Send one message to the agent: the text, then a SEPARATE `\r` frame —
   * bundled into one write TUI apps treat the trailing return as a paste,
   * which inserts instead of submitting. The sent text is echoed into the
   * local feed immediately (EXP-78); its transcript-derived `user_message`
   * event is deduped against the echo FIFO when it arrives.
   */
  const sendMessage = (text: string) => {
    if (!text || !sendInput(text)) return
    wsRef.current?.send(JSON.stringify({ t: `input`, data: `\r` }))
    pushEcho(recentEchoesRef.current, text, Date.now())
    setFeed((prev) =>
      [
        ...prev,
        { id: nextIdRef.current++, kind: `user_message` as const, text },
      ].slice(-FEED_CAP)
    )
  }

  /** Legacy answer path (a desktop that publishes no question ids): raw
   *  keystrokes — the desktop passes single-byte frames to the PTY unwrapped,
   *  so the TUI sees keypresses, not a paste. NO trailing `\r`: a digit
   *  already selects AND advances in claude's picker, so the extra return
   *  cascaded into the next question and auto-answered it (EXP-249).
   *  Multi-select taps toggle with the digit alone; Continue sends `\t`. */
  const sendKeystrokes = (keys: string[]): boolean => {
    const sock = wsRef.current
    if (perm !== `steer` || sock?.readyState !== WebSocket.OPEN) return false
    for (const key of keys) sock.send(JSON.stringify({ t: `input`, data: key }))
    return true
  }

  /** Protocol v2 answer: the relay forwards it verbatim to the desktop, which
   *  drives its own picker and confirms with `answer_ack`. Same perm gating
   *  as raw input. */
  const sendAnswerFrame = (
    questionId: string,
    askId: string | undefined,
    keys: string[]
  ): boolean => {
    const sock = wsRef.current
    if (perm !== `steer` || sock?.readyState !== WebSocket.OPEN) return false
    sock.send(JSON.stringify({ t: `answer`, questionId, askId, keys }))
    return true
  }

  /** Submit a card's answer and LOCK it immediately — a locked card never
   *  fires again. `answer_ack` confirms the lock; `question_resolved`
   *  finalizes it; ANSWER_ACK_TIMEOUT_MS without either re-enables the card
   *  with an inline note. */
  const answerQuestion = (
    item: QuestionItem,
    keys: string[],
    labels: string[]
  ) => {
    const key = answerKey(item)
    if (isAnswerLocked(answerStates[key]) || item.resolved === true) return
    const sent = item.questionId
      ? sendAnswerFrame(item.questionId, item.askId, keys)
      : sendKeystrokes(keys)
    if (!sent) return
    setAnswerStates((prev) => beginAnswer(prev, key, keys, labels))
    clearAckTimer(key)
    ackTimersRef.current.set(
      key,
      setTimeout(() => {
        ackTimersRef.current.delete(key)
        setAnswerStates((prev) => failAnswer(prev, key))
      }, ANSWER_ACK_TIMEOUT_MS)
    )
  }

  /** A legacy multi-select toggle — one keystroke, no lock: the selection is
   *  only submitted by Continue. */
  const toggleLegacyOption = (key: string) => {
    sendKeystrokes([key])
  }

  /** Escape interrupts whatever the agent is currently doing. */
  const sendEscape = () => {
    sendInput(`\u001b`)
  }

  const kill = async () => {
    setKilling(true)
    try {
      await trpc.steer.killSession.mutate(
        { codingSessionId: session.id },
        { context: { skipErrorToast: true } }
      )
      setConfirmKill(false)
      // The synced row flips to ended — the dock keeps the panel mounted
      // until the user collapses it; the relay `bye` tears the socket down.
    } catch (error) {
      toast.error(`Couldn't kill the session`, {
        description: trpcErrorMessage(error, `The kill could not be delivered`),
      })
    } finally {
      setKilling(false)
    }
  }

  // ── Follow-scroll: pinned to the newest event until the user scrolls up ───

  const handleFeedScroll = () => {
    const el = scrollRef.current
    if (!el) return
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 32)
  }

  const jumpToBottom = () => {
    const el = scrollRef.current
    if (el) el.scrollTo({ top: el.scrollHeight })
    setAtBottom(true)
  }

  // A resolved card carries its own answer — drop its lock so a stale ack
  // deadline can't flip a finished card into the retry state.
  useEffect(() => {
    setAnswerStates((prev) => {
      let next = prev
      for (const item of feed) {
        if (item.kind !== `question` || item.resolved !== true) continue
        const key = answerKey(item)
        if (!(key in next)) continue
        clearAckTimer(key)
        next = clearAnswer(next, key)
      }
      return next
    })
  }, [feed])

  useEffect(
    () => () => {
      for (const timer of ackTimersRef.current.values()) clearTimeout(timer)
      ackTimersRef.current.clear()
    },
    []
  )

  useEffect(() => {
    if (!atBottom) return
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [feed, atBottom, phase.kind])

  const diffFiles = useMemo(
    () => (latestDiff ? splitUnifiedDiff(latestDiff) : []),
    [latestDiff]
  )
  const diffStats = useMemo(
    () =>
      diffFiles.reduce(
        (acc, f) => ({
          additions: acc.additions + f.additions,
          deletions: acc.deletions + f.deletions,
        }),
        { additions: 0, deletions: 0 }
      ),
    [diffFiles]
  )

  const live = phase.kind === `live`
  const sessionEnded = session.status === `ended`
  const composerVisible = live && perm === `steer` && !sessionEnded

  /** Identity-scoped questions stay answerable until they resolve; legacy
   *  cards fall back to the trailing-run heuristic, with a plan-approval card
   *  answerable until a real resolution signal — lagged transcript flushes
   *  don't retire a pending picker (EXP-174). */
  const questionIds = useMemo(() => activeQuestionIds(feed), [feed])
  const canAnswer = live && perm === `steer` && !sessionEnded
  /** Render rows: consecutive tool calls collapse into "N tool calls" runs
   *  (EXP-97), one ask's questions into a stepper and a subagent's work into
   *  its own group — a projection only, the flat feed stays the state. */
  const rows = useMemo(() => groupFeedRows(feed), [feed])
  /** A trailing question/plan means the session is blocked on a human — the
   *  header flips to "Needs your input" so it never looks silently stuck. */
  const awaitingInput = live && questionIds.size > 0

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Compact header line — the dock owns the panel frame, so this is just
          identity + controls. */}
      <div className="flex items-center gap-2 border-b border-border px-3 py-1.5">
        <PhaseIndicator
          phase={phase}
          deviceLabel={session.deviceLabel}
          awaitingInput={awaitingInput}
        />
        <div className="min-w-0 flex-1 truncate text-sm">{title}</div>
        {phase.kind === `closed` && (
          <Button
            variant="outline"
            size="sm"
            className="shrink-0"
            onClick={() => setAttempt((n) => n + 1)}
          >
            <RotateCw />
            Reconnect
          </Button>
        )}
        {/* steer.killSession also authorizes the session owner — a member who
            remote-started their own session can kill it without steer perm. */}
        {live && (perm === `steer` || session.userId === currentUserId) && (
          <Button
            variant="ghost"
            size="icon"
            className="shrink-0 text-destructive hover:text-destructive"
            aria-label="Kill session"
            title="Kill session"
            onClick={() => setConfirmKill(true)}
          >
            <OctagonX />
          </Button>
        )}
        {onToggleFullscreen && (
          <Button
            variant="ghost"
            size="icon"
            className="shrink-0"
            aria-label={isFullscreen ? `Exit fullscreen` : `Fullscreen`}
            onClick={onToggleFullscreen}
          >
            {isFullscreen ? <Minimize2 /> : <Maximize2 />}
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon"
          className="shrink-0"
          aria-label="Collapse session"
          onClick={onCollapse}
        >
          <ChevronDown />
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card/40">
          {/* The activity feed (bottom-anchored, follow-scroll) */}
          <div className="relative min-h-0 flex-1">
            <div
              ref={scrollRef}
              onScroll={handleFeedScroll}
              className="h-full overflow-y-auto"
            >
              {feed.length === 0 &&
              (phase.kind === `connecting` || phase.kind === `starting`) ? (
                <CenteredState>
                  <Loader2 className="size-4 animate-spin text-muted-foreground" />
                  <span className="text-xs text-muted-foreground">
                    {phase.kind === `starting`
                      ? `The agent is starting — waiting for the live stream…`
                      : `Connecting…`}
                  </span>
                </CenteredState>
              ) : feed.length === 0 && live && !latestDiff ? (
                <CenteredState>
                  <span className="text-sm text-muted-foreground">
                    Waiting for activity…
                  </span>
                  <span className="text-xs text-muted-foreground/70">
                    This session isn&apos;t publishing an activity feed — it may
                    be marked private on the desktop, or the desktop app needs
                    an update.
                  </span>
                </CenteredState>
              ) : (
                <div className="flex min-h-full flex-col justify-end gap-0.5 px-3 py-2">
                  {rows.map((row, index) => {
                    if (row.kind === `toolRun`) {
                      return (
                        <ToolGroupRow
                          key={row.id}
                          items={
                            row.items as Extract<FeedItem, { kind: `tool` }>[]
                          }
                          liveTail={live && index === rows.length - 1}
                        />
                      )
                    }
                    if (row.kind === `subagent`) {
                      return <SubagentGroupRow key={row.id} items={row.items} />
                    }
                    if (row.kind === `ask`) {
                      return (
                        <AskStepperCard
                          key={row.id}
                          items={row.items as QuestionItem[]}
                          activeIds={questionIds}
                          canAnswer={canAnswer}
                          answerStates={answerStates}
                          onAnswer={answerQuestion}
                          onToggleLegacy={toggleLegacyOption}
                        />
                      )
                    }
                    const item = row.item
                    switch (item.kind) {
                      case `narration`:
                        return <NarrationBubble key={item.id} text={item.text} />
                      case `tool`:
                        return (
                          <ToolRow
                            key={item.id}
                            name={item.name}
                            detail={item.detail}
                          />
                        )
                      case `user_message`:
                        return (
                          <UserMessageBubble key={item.id} text={item.text} />
                        )
                      case `permission`:
                        return (
                          <PermissionRow
                            key={item.id}
                            tool={item.tool}
                            detail={item.detail}
                          />
                        )
                      case `subagent`:
                        return <SubagentGroupRow key={item.id} items={[item]} />
                      case `question`:
                        return (
                          <QuestionCard
                            key={item.id}
                            item={item}
                            active={questionIds.has(item.id)}
                            canAnswer={canAnswer}
                            answerState={answerStates[answerKey(item)]}
                            onAnswer={answerQuestion}
                            onToggleLegacy={toggleLegacyOption}
                          />
                        )
                    }
                  })}
                </div>
              )}
            </div>
            {!atBottom && feed.length > 0 && (
              <Button
                variant="secondary"
                size="sm"
                className="absolute bottom-2 left-1/2 h-7 -translate-x-1/2 rounded-full border border-border shadow-md"
                onClick={jumpToBottom}
              >
                Jump to bottom
                <ArrowDown />
              </Button>
            )}
          </div>

          {/* Status banners (feed retained above) */}
          {phase.kind === `ended` && (
            <div className="border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              {phase.detail ?? `The session has ended.`}
            </div>
          )}
          {phase.kind === `closed` && (
            <div className="border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              {phase.detail ?? `Connection lost.`}
            </div>
          )}
          {phase.kind === `starting` && feed.length > 0 && (
            <div className="flex items-center gap-1.5 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              The agent is starting — waiting for the live stream…
            </div>
          )}

          {/* Pinned "Latest changes" (directly above the composer) */}
          {latestDiff && (
            <Collapsible
              open={diffOpen}
              onOpenChange={setDiffOpen}
              className="border-t border-border"
            >
              <CollapsibleTrigger className="flex w-full items-center gap-2 bg-muted/30 px-3 py-2 text-left text-xs transition-colors hover:bg-muted/50">
                <ChevronRight
                  className={cn(
                    `size-3.5 shrink-0 text-muted-foreground transition-transform`,
                    diffOpen && `rotate-90`
                  )}
                />
                <span className="font-medium">Latest changes</span>
                <span className="ml-auto" />
                <span className="shrink-0 font-mono">
                  <span className="text-emerald-400">+{diffStats.additions}</span>
                  {` `}
                  <span className="text-rose-400">-{diffStats.deletions}</span>
                </span>
              </CollapsibleTrigger>
              <CollapsibleContent>
                <div className="max-h-72 overflow-y-auto border-t border-border/60">
                  <FileDiffList files={diffFiles} />
                </div>
              </CollapsibleContent>
            </Collapsible>
          )}

          {/* Steering composer (perm-gated). Steering is fully seamless
              (EXP-312) — no captions, no operator state. */}
          {composerVisible && (
            <div className="border-t border-border p-2">
              <MessageComposer onSend={sendMessage} onEscape={sendEscape} />
            </div>
          )}
      </div>

      <Dialog open={confirmKill} onOpenChange={setConfirmKill}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Kill this coding session?</DialogTitle>
            <DialogDescription>
              This force-terminates the terminal
              {session.deviceLabel ? ` on ${session.deviceLabel}` : ``} and
              ends the session. Uncommitted work in the worktree is kept, but
              Claude stops immediately.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setConfirmKill(false)}
              disabled={killing}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => void kill()}
              disabled={killing}
            >
              {killing && <Loader2 className="animate-spin" />}
              Kill session
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

// ── Pieces ───────────────────────────────────────────────────────────────────

function PhaseIndicator({
  phase,
  deviceLabel,
  awaitingInput = false,
}: {
  phase: ViewerPhase
  deviceLabel: string | null
  /** Live but blocked on a trailing question/plan — the session is waiting
   *  for a human, not stuck (EXP-97). */
  awaitingInput?: boolean
}) {
  const connecting = phase.kind === `connecting` || phase.kind === `starting`
  const awaiting = phase.kind === `live` && awaitingInput
  const label =
    phase.kind === `live`
      ? awaiting
        ? deviceLabel
          ? `Needs your input · ${deviceLabel}`
          : `Needs your input`
        : deviceLabel
          ? `Live · ${deviceLabel}`
          : `Live`
      : phase.kind === `starting`
        ? `Agent starting…`
        : phase.kind === `connecting` || phase.kind === `idle`
          ? `Connecting…`
          : phase.kind === `ended`
            ? `Session ended`
            : `Disconnected`
  return (
    <span className="inline-flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <span
        className={cn(
          `size-2 shrink-0 rounded-full`,
          phase.kind === `live` && (awaiting ? `bg-amber-400` : `bg-emerald-500`),
          connecting && `animate-pulse bg-amber-400`,
          !connecting && phase.kind !== `live` && `bg-muted-foreground/40`
        )}
      />
      <span className={cn(`truncate`, awaiting && `text-amber-400`)}>
        {label}
      </span>
    </span>
  )
}

function CenteredState({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      {children}
    </div>
  )
}

/** Assistant prose — a chat bubble with a small glyph, selectable text. */
function NarrationBubble({ text }: { text: string }) {
  return (
    <div className="flex items-start gap-2 py-1">
      <Sparkles className="mt-2 size-3 shrink-0 text-muted-foreground/60" />
      <div className="min-w-0 flex-1 whitespace-pre-wrap rounded-md border border-border/60 bg-muted/30 px-3 py-1.5 text-sm text-foreground/90">
        {text}
      </div>
    </div>
  )
}

/** How much user/question text shows before the "Show more" fold (the initial
 *  prompt can be 16 KiB). Line-based clamp via CSS; the toggle appears on any
 *  plausibly-clamped text. */
const CLAMP_LINES = 6
const CLAMP_CHARS = 600

function useClampToggle(text: string) {
  const [expanded, setExpanded] = useState(false)
  const clampable =
    text.length > CLAMP_CHARS || text.split(`\n`).length > CLAMP_LINES
  return { expanded, setExpanded, clampable }
}

function ShowMoreButton({
  expanded,
  onToggle,
}: {
  expanded: boolean
  onToggle: () => void
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="mt-1 text-[0.6875rem] font-medium text-muted-foreground hover:text-foreground"
    >
      {expanded ? `Show less` : `Show more`}
    </button>
  )
}

/** A human turn (EXP-78): the initial prompt or a steered message — rendered
 *  right-aligned like the sender's own chat bubble, long text folded. */
function UserMessageBubble({ text }: { text: string }) {
  const { expanded, setExpanded, clampable } = useClampToggle(text)
  return (
    <div className="flex justify-end py-1 pl-8">
      <div className="min-w-0 rounded-md border border-primary/25 bg-primary/10 px-3 py-1.5 text-sm text-foreground/90">
        <div
          className={cn(
            `whitespace-pre-wrap break-words`,
            clampable && !expanded && `line-clamp-6`
          )}
        >
          {text}
        </div>
        {clampable && (
          <ShowMoreButton
            expanded={expanded}
            onToggle={() => setExpanded((v) => !v)}
          />
        )}
      </div>
    </div>
  )
}

type AnswerHandler = (
  item: QuestionItem,
  keys: string[],
  labels: string[]
) => void

/** The interactive half of a question card: the options, the immediate lock
 *  once an answer goes out, and the resolved answer. Two answer paths:
 *  protocol v2 cards (a wire id) send the semantic `answer` frame and wait for
 *  `answer_ack`; legacy cards send raw TUI keystrokes — a single-select tap is
 *  the digit ALONE (the digit selects AND advances, so a trailing return used
 *  to auto-answer the NEXT question), multi-select taps toggle with the digit
 *  and Continue advances with `\t` (Enter would toggle the highlighted row,
 *  verified against claude v2.1.215). */
function QuestionPrompt({
  item,
  active,
  canAnswer,
  answerState,
  onAnswer,
  onToggleLegacy,
  variant = `default`,
}: {
  item: QuestionItem
  /** Still answerable per the feed — the session is blocked on this card. */
  active: boolean
  /** Live + steer perm — whether this client may answer at all. */
  canAnswer: boolean
  answerState?: AnswerState
  onAnswer: AnswerHandler
  onToggleLegacy: (key: string) => void
  /** `plan`/`submit` promote the first option to the primary action. */
  variant?: `default` | `plan` | `submit`
}) {
  const [picked, setPicked] = useState<string[]>([])
  const locked = isAnswerLocked(answerState)
  const semantic = item.questionId !== undefined
  const answerable = active && canAnswer && !locked && item.resolved !== true

  if (item.resolved === true) {
    return (
      <AnsweredLine answer={item.answer} dismissed={item.dismissed === true} />
    )
  }
  if (locked) {
    return (
      <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
        <Loader2 className="size-3 shrink-0 animate-spin" />
        <span className="shrink-0">Answering…</span>
        {answerState && answerState.labels.length > 0 && (
          <span className="truncate font-medium text-foreground/80">
            {answerState.labels.join(`, `)}
          </span>
        )}
      </div>
    )
  }

  const labelsFor = (keys: string[]) =>
    item.options.filter((o) => keys.includes(o.key)).map((o) => o.label)

  const choose = (option: QuestionOption) => {
    if (!answerable) return
    if (item.multiSelect) {
      // Legacy toggles land in the TUI right away; semantic ones stay local
      // until the answer frame goes out.
      if (!semantic) onToggleLegacy(option.key)
      setPicked((prev) =>
        prev.includes(option.key)
          ? prev.filter((k) => k !== option.key)
          : [...prev, option.key]
      )
      return
    }
    onAnswer(item, [option.key], [option.label])
  }

  const submitPicked = () => {
    if (!answerable) return
    onAnswer(item, semantic ? picked : [`\t`], labelsFor(picked))
  }

  return (
    <>
      <div
        className={cn(
          `mt-2 flex items-start gap-1`,
          variant === `default`
            ? `flex-col`
            : `flex-row flex-wrap items-center gap-1.5`
        )}
      >
        {item.options.map((option, index) =>
          answerable ? (
            <Button
              key={option.key}
              variant={variant !== `default` && index === 0 ? `default` : `outline`}
              size="sm"
              className={cn(
                `h-auto min-h-7 justify-start whitespace-normal py-1 text-left text-xs`,
                picked.includes(option.key) &&
                  (variant === `default`
                    ? `border-amber-500/60 bg-amber-500/15`
                    : `border-primary/60 bg-primary/15`)
              )}
              onClick={() => choose(option)}
            >
              {variant === `default` && (
                <span className="font-mono text-muted-foreground">
                  {option.key}
                </span>
              )}
              <span className="flex min-w-0 flex-col items-start gap-0.5">
                <span>
                  {variant === `submit` && index === 0
                    ? `Submit answers`
                    : option.label}
                </span>
                {option.description && (
                  <span className="font-normal text-[0.6875rem] text-muted-foreground">
                    {option.description}
                  </span>
                )}
              </span>
            </Button>
          ) : (
            <span key={option.key} className="text-xs text-muted-foreground">
              <span className="font-mono">{option.key}</span>
              {` · ${option.label}`}
            </span>
          )
        )}
      </div>
      {answerable && item.multiSelect && (
        <Button
          variant="secondary"
          size="sm"
          className="mt-2 h-7 text-xs"
          disabled={semantic && picked.length === 0}
          onClick={submitPicked}
        >
          {semantic ? `Answer` : `Continue`}
        </Button>
      )}
      {/* Only the semantic path is acknowledged — a legacy card just unlocks
          again, with nothing to report. */}
      {answerState?.status === `error` && semantic && (
        <div className="mt-1.5 text-[0.6875rem] text-amber-400">
          No confirmation from the desktop — pick again to retry.
        </div>
      )}
      {active && !canAnswer && (
        <div className="mt-2 text-xs text-muted-foreground">
          {item.planMode
            ? `Waiting for approval — you're viewing read-only.`
            : `Waiting for an answer — you're viewing read-only.`}
        </div>
      )}
    </>
  )
}

/** The resolution of a card — the chosen answer, or a dismissal (Esc). */
function AnsweredLine({
  answer,
  dismissed,
}: {
  answer?: string
  dismissed: boolean
}) {
  return (
    <div className="mt-2 flex items-start gap-1.5 text-xs">
      {dismissed ? (
        <X className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      ) : (
        <Check className="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
      )}
      <span className="whitespace-pre-wrap break-words font-medium text-foreground/90">
        {dismissed ? `Dismissed` : (answer ?? `Answered`)}
      </span>
    </div>
  )
}

/** A standalone question (EXP-78): a plan approval, or an AskUserQuestion from
 *  a desktop that publishes no ask grouping. `planMode` cards (EXP-97) get a
 *  "Plan ready" presentation with the first option as the primary approve
 *  action and the plan ALWAYS rendered as markdown (folded behind a height
 *  clamp while long, EXP-249) — labels/keys always come from the wire
 *  `options`, the desktop owns the TUI key mapping. */
function QuestionCard({
  item,
  active,
  canAnswer,
  answerState,
  onAnswer,
  onToggleLegacy,
}: {
  item: QuestionItem
  active: boolean
  canAnswer: boolean
  answerState?: AnswerState
  onAnswer: AnswerHandler
  onToggleLegacy: (key: string) => void
}) {
  const { expanded, setExpanded, clampable } = useClampToggle(item.text)
  const plan = item.planMode
  return (
    <div
      className={cn(
        `my-1 rounded-md border px-3 py-2`,
        plan
          ? `border-primary/40 bg-primary/5`
          : `border-amber-500/40 bg-amber-500/5`
      )}
    >
      <div className="flex items-start gap-2">
        {plan ? (
          <ClipboardList className="mt-0.5 size-3.5 shrink-0 text-primary" />
        ) : (
          <CircleHelp className="mt-0.5 size-3.5 shrink-0 text-amber-400" />
        )}
        <div className="min-w-0 flex-1">
          {plan ? (
            <div className="mb-1 text-xs font-medium text-primary">
              Plan ready
            </div>
          ) : (
            item.header && (
              <div className="mb-1 text-xs font-medium text-amber-400">
                {item.header}
              </div>
            )
          )}
          {plan ? (
            // The plan is GFM markdown — always rendered as markdown; a long
            // plan folds behind a height clamp instead of dropping to raw text.
            <div
              className={cn(
                `text-sm`,
                clampable && !expanded && `max-h-56 overflow-hidden`
              )}
            >
              <MarkdownEditor
                markdown={item.text}
                editable={false}
                onChange={() => {}}
              />
            </div>
          ) : (
            <div
              className={cn(
                `whitespace-pre-wrap break-words text-sm text-foreground/90`,
                clampable && !expanded && `line-clamp-6`
              )}
            >
              {item.text}
            </div>
          )}
          {clampable && (
            <ShowMoreButton
              expanded={expanded}
              onToggle={() => setExpanded((v) => !v)}
            />
          )}
          <QuestionPrompt
            item={item}
            active={active}
            canAnswer={canAnswer}
            answerState={answerState}
            onAnswer={onAnswer}
            onToggleLegacy={onToggleLegacy}
            variant={plan ? `plan` : `default`}
          />
        </div>
      </div>
    </div>
  )
}

/** One multi-question ask (protocol v2 `askId`), claude-style: one question at
 *  a time with "k of n" progression, answered steps collapsed behind their
 *  chosen answer, and the ask's final review step rendered with an explicit
 *  "Submit answers" button once the desktop publishes it. The stepper advances
 *  the moment a step locks — `answer_ack` then confirms it. */
function AskStepperCard({
  items,
  activeIds,
  canAnswer,
  answerStates,
  onAnswer,
  onToggleLegacy,
}: {
  items: QuestionItem[]
  activeIds: Set<number>
  canAnswer: boolean
  answerStates: AnswerStates
  onAnswer: AnswerHandler
  onToggleLegacy: (key: string) => void
}) {
  const view = askStepperView(items, answerStates)
  const current =
    view.steps.find((s) => s.phase === `current`) ??
    (view.submit?.phase === `current` ? view.submit : null)
  const answered = view.steps.filter((s) => s.phase === `answered`)
  const header = current?.item.header ?? items[0]?.header
  const submitStep = current !== null && current.item.index === undefined

  return (
    <div className="my-1 rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2">
      <div className="flex items-start gap-2">
        <CircleHelp className="mt-0.5 size-3.5 shrink-0 text-amber-400" />
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center gap-2">
            <span className="truncate text-xs font-medium text-amber-400">
              {header ?? (submitStep ? `Review answers` : `Question`)}
            </span>
            {view.total > 1 && (
              <span className="shrink-0 text-[0.6875rem] text-muted-foreground">
                {current && !submitStep
                  ? `${view.position} of ${view.total}`
                  : `${view.total} questions`}
              </span>
            )}
          </div>
          {answered.map((step) => (
            <AnsweredStepRow
              key={step.item.id}
              text={step.item.text}
              answer={step.answer}
              dismissed={step.item.dismissed === true}
            />
          ))}
          {current ? (
            <div className="mt-1.5">
              <div className="whitespace-pre-wrap break-words text-sm text-foreground/90">
                {current.item.text}
              </div>
              <QuestionPrompt
                // Per-step multi-select state must not survive the step
                // advancing — the prompt sits at a fixed tree position.
                key={current.item.id}
                item={current.item}
                active={activeIds.has(current.item.id)}
                canAnswer={canAnswer}
                answerState={answerStates[answerKey(current.item)]}
                onAnswer={onAnswer}
                onToggleLegacy={onToggleLegacy}
                variant={submitStep ? `submit` : `default`}
              />
            </div>
          ) : (
            view.waiting && (
              <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
                <Loader2 className="size-3 animate-spin" />
                Waiting for the next question…
              </div>
            )
          )}
        </div>
      </div>
    </div>
  )
}

/** An answered step inside the stepper — the question, folded to one line,
 *  next to what was chosen. */
function AnsweredStepRow({
  text,
  answer,
  dismissed,
}: {
  text: string
  answer?: string
  dismissed: boolean
}) {
  return (
    <div className="flex items-center gap-1.5 border-b border-border/40 py-1 text-xs last:border-b-0">
      {dismissed ? (
        <X className="size-3 shrink-0 text-muted-foreground" />
      ) : (
        <Check className="size-3 shrink-0 text-emerald-500" />
      )}
      <span className="min-w-0 flex-1 truncate text-muted-foreground" title={text}>
        {text}
      </span>
      <span className="max-w-[50%] shrink-0 truncate font-medium text-foreground/90">
        {dismissed ? `Dismissed` : (answer ?? `Answered`)}
      </span>
    </div>
  )
}

/** A permission prompt the agent raised (protocol v2) — informational only:
 *  the decision lives in the desktop TUI, there is nothing to answer here. */
function PermissionRow({ tool, detail }: { tool: string; detail?: string }) {
  return (
    <div className="flex min-w-0 items-center gap-2 py-0.5 pl-0.5">
      <ShieldQuestion className="size-3 shrink-0 text-amber-400/70" />
      <span className="shrink-0 text-xs font-medium text-amber-400/90">
        Permission · {tool}
      </span>
      {detail && (
        <span
          className="truncate font-mono text-[0.6875rem] text-muted-foreground"
          title={detail}
        >
          {detail}
        </span>
      )}
    </div>
  )
}

/** A subagent's work (protocol v2): its lifecycle events plus every tool call
 *  it made, collapsed into one expandable row like a tool run. */
function SubagentGroupRow({ items }: { items: FeedItem[] }) {
  const [expanded, setExpanded] = useState(false)
  const agents = items.filter(
    (i): i is Extract<FeedItem, { kind: `subagent` }> => i.kind === `subagent`
  )
  const tools = items.filter(
    (i): i is Extract<FeedItem, { kind: `tool` }> => i.kind === `tool`
  )
  const latest = agents[agents.length - 1]
  const completed = agents.some((a) => a.status === `completed`)
  const detail = [...agents].reverse().find((a) => a.detail)?.detail
  return (
    <div className="min-w-0">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full min-w-0 items-center gap-2 py-0.5 pl-0.5 text-muted-foreground hover:text-foreground"
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0" />
        ) : (
          <ChevronRight className="size-3 shrink-0" />
        )}
        <Bot className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="shrink-0 text-xs font-medium">
          {latest?.agentType ?? `subagent`}
        </span>
        {!completed && <Loader2 className="size-3 shrink-0 animate-spin" />}
        <span className="shrink-0 text-[0.6875rem]">
          {completed ? `done` : `running`}
          {tools.length > 0 &&
            ` · ${tools.length} tool call${tools.length === 1 ? `` : `s`}`}
        </span>
        {detail && (
          <span className="truncate text-[0.6875rem]" title={detail}>
            {detail}
          </span>
        )}
      </button>
      {expanded && tools.length > 0 && (
        <div className="ml-5">
          {tools.map((tool) => (
            <ToolRow key={tool.id} name={tool.name} detail={tool.detail} />
          ))}
        </div>
      )}
    </div>
  )
}

/** Tool-call headline — compact single line, consecutive rows visually tight. */
function ToolRow({ name, detail }: { name: string; detail?: string }) {
  return (
    <div className="flex min-w-0 items-center gap-2 py-0.5 pl-0.5">
      <Wrench className="size-3 shrink-0 text-muted-foreground/60" />
      <span className="shrink-0 text-xs font-medium">{name}</span>
      {detail && (
        <span
          className="truncate font-mono text-[0.6875rem] text-muted-foreground"
          title={detail}
        >
          {detail}
        </span>
      )}
    </div>
  )
}

/** A run of ≥2 consecutive tool calls collapsed into one "N tool calls" row
 *  (EXP-97), expandable to the individual rows. While the run is the trailing
 *  row of a live session, the latest call stays visible under the count so
 *  the viewer still sees live progress. */
function ToolGroupRow({
  items,
  liveTail,
}: {
  items: Extract<FeedItem, { kind: `tool` }>[]
  liveTail: boolean
}) {
  const [expanded, setExpanded] = useState(false)
  const latest = items[items.length - 1]
  return (
    <div className="min-w-0">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex min-w-0 items-center gap-2 py-0.5 pl-0.5 text-muted-foreground hover:text-foreground"
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0" />
        ) : (
          <ChevronRight className="size-3 shrink-0" />
        )}
        <Wrench className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="shrink-0 text-xs font-medium">
          {items.length} tool calls
        </span>
      </button>
      {expanded ? (
        <div className="ml-5">
          {items.map((item) => (
            <ToolRow key={item.id} name={item.name} detail={item.detail} />
          ))}
        </div>
      ) : (
        liveTail && (
          <div className="ml-5">
            <ToolRow name={latest.name} detail={latest.detail} />
          </div>
        )
      )}
    </div>
  )
}

function MessageComposer({
  onSend,
  onEscape,
}: {
  onSend: (text: string) => void
  onEscape: () => void
}) {
  const [text, setText] = useState(``)

  const send = () => {
    if (!text.trim()) return
    onSend(text)
    setText(``)
  }

  return (
    <div className="flex items-end gap-1.5">
      <Textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            send()
          }
        }}
        placeholder="Message the agent…"
        rows={1}
        className={cn(
          `max-h-32 min-h-9 flex-1 resize-none border-none shadow-none focus-visible:ring-0`,
          // dark: variant included so it beats the base dark:bg-input/30.
          `bg-muted/40 dark:bg-muted/40`
        )}
      />
      <Button
        variant="outline"
        size="sm"
        className="h-9 shrink-0 px-2.5 font-mono text-xs text-muted-foreground"
        title="Send Escape — interrupts what the agent is doing"
        onClick={onEscape}
      >
        Esc
      </Button>
      <Button
        size="icon"
        className="shrink-0"
        aria-label="Send"
        disabled={!text.trim()}
        onClick={send}
      >
        <ArrowUp />
      </Button>
    </div>
  )
}
