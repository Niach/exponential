import { useEffect, useMemo, useRef, useState } from "react"
import { toast } from "sonner"
import { TRPCClientError } from "@trpc/client"
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  ClipboardList,
  Eye,
  Loader2,
  Maximize2,
  Minimize2,
  OctagonX,
  RotateCw,
  Sparkles,
  Wrench,
} from "lucide-react"
import type { CodingSession } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  consumeEcho,
  groupToolRuns,
  pushEcho,
  activeQuestionIds,
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
// Steering is message-shaped like mobile: a steal-claim + chunked input + a
// SEPARATE `\r` frame. Since EXP-106 this view is mounted ONLY by the global
// agent dock (components/agent-dock) — one at a time — so it always
// auto-connects and delegates its chrome (title, collapse) to the dock; the
// "coding now" rows + remote-start affordances moved to issue-coding-rows.tsx.

// ── Wire protocol (activity-viewer side of apps/steer-relay/src/protocol.ts) ─

// Relay rejects input frames > 8 KiB; chunk pastes well under that.
const INPUT_CHUNK_CHARS = 4096
/** Client-side feed cap — old events fall off the top. */
const FEED_CAP = 500
/** Auto-release the steer claim after this long with no sends. */
const IDLE_RELEASE_MS = 60_000
/** Redial cadence while the desktop's publisher socket is still starting. */
const STARTING_RETRY_MS = 3_000

interface PresenceViewer {
  userId: string
  name: string
  perm: `view` | `steer`
}

interface QuestionOption {
  label: string
  /** Raw keystroke that selects this option in the desktop TUI picker. */
  key: string
}

type ActivityEvent =
  | { kind: `narration`; text: string; at?: number }
  | { kind: `tool`; name: string; detail?: string; at?: number }
  | { kind: `diff`; diff: string; at?: number }
  // EXP-78 (member-only on the relay): a human turn from the transcript…
  | { kind: `user_message`; text: string; at?: number }
  // …and an interactive question (AskUserQuestion / plan approval).
  // `planMode` marks an ExitPlanMode plan-approval picker (EXP-97) — absent
  // on generic questions and on events from older desktops/relays.
  | {
      kind: `question`
      text: string
      options: QuestionOption[]
      multiSelect?: boolean
      planMode?: boolean
      at?: number
    }

type ServerFrame =
  | { t: `presence`; viewers: PresenceViewer[]; steererId: string | null }
  | { t: `activity`; event: ActivityEvent }
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
  // still dialing its publisher socket; the view auto-redials every ~3s.
  | { kind: `starting` }
  | { kind: `live` }
  // The session ended (relay `bye`, or the room was never live).
  | { kind: `ended`; detail?: string }
  // Unexpected socket loss — offer a manual Reconnect (fresh ticket).
  | { kind: `closed`; detail?: string }

type FeedItem =
  | { id: number; kind: `narration`; text: string }
  | { id: number; kind: `tool`; name: string; detail?: string }
  | { id: number; kind: `user_message`; text: string }
  | {
      id: number
      kind: `question`
      text: string
      options: QuestionOption[]
      multiSelect: boolean
      planMode: boolean
    }

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
  const [viewers, setViewers] = useState<PresenceViewer[]>([])
  const [steererId, setSteererId] = useState<string | null>(null)
  const [feed, setFeed] = useState<FeedItem[]>([])
  /** The most recent worktree diff — each one replaces the previous. */
  const [latestDiff, setLatestDiff] = useState<string | null>(null)
  const [diffOpen, setDiffOpen] = useState(false)
  const [confirmKill, setConfirmKill] = useState(false)
  const [killing, setKilling] = useState(false)
  const [atBottom, setAtBottom] = useState(true)

  const wsRef = useRef<WebSocket | null>(null)
  const steeringRef = useRef(false)
  const nextIdRef = useRef(0)
  /** Locally-echoed sent messages awaiting their transcript-derived event. */
  const recentEchoesRef = useRef<EchoEntry[]>([])
  const idleReleaseRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const scrollRef = useRef<HTMLDivElement | null>(null)
  // The synced row is the truth for "still running" inside the redial loop.
  const sessionStatusRef = useRef(session.status)
  sessionStatusRef.current = session.status

  const steering = steererId === currentUserId
  steeringRef.current = steering

  useEffect(() => {
    if (attempt === 0) return

    let disposed = false
    let retryTimer: ReturnType<typeof setTimeout> | null = null
    let ws: WebSocket | null = null

    const clearIdleRelease = () => {
      if (idleReleaseRef.current) {
        clearTimeout(idleReleaseRef.current)
        idleReleaseRef.current = null
      }
    }

    const markLive = () =>
      setPhase((prev) => (prev.kind === `live` ? prev : { kind: `live` }))

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
          if (!event.text.trim()) return
          append({ kind: `narration`, text: event.text })
          return
        }
        case `tool`: {
          const detail = event.detail?.trim() ? event.detail : undefined
          append({ kind: `tool`, name: event.name, detail })
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
          append({
            kind: `question`,
            text: event.text,
            options: event.options,
            multiSelect: event.multiSelect === true,
            planMode: event.planMode === true,
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
      // to `connecting` per attempt makes the header flicker every ~3s.
      if (!retrying) setPhase({ kind: `connecting` })
      setViewers([])
      setSteererId(null)

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
          // The relay replays the room's whole activity log (+ last diff) to
          // every joining socket — start from a clean slate or each reconnect
          // would append the full history a second time. The echo FIFO clears
          // too: after a reconnect the replayed transcript event is the ONLY
          // copy of a sent message and must render.
          setFeed([])
          setLatestDiff(null)
          nextIdRef.current = 0
          recentEchoesRef.current = []
          ws?.send(JSON.stringify({ t: `join`, channel: `activity` }))
          // NOT live yet — the relay may answer the join with no_such_session
          // (desktop still starting). The phase flips to live on the first
          // confirming server frame instead (the relay sends presence
          // immediately on a successful join).
        }
        ws.onmessage = (event) => {
          if (disposed || typeof event.data !== `string`) return
          const frame = parseServerFrame(event.data)
          if (!frame) return
          switch (frame.t) {
            case `presence`: {
              const f = frame as Extract<ServerFrame, { t: `presence` }>
              setViewers(f.viewers)
              setSteererId(f.steererId)
              markLive()
              return
            }
            case `activity`: {
              const f = frame as Extract<ServerFrame, { t: `activity` }>
              handleActivity(f.event)
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
          clearIdleRelease()
          setViewers([])
          setSteererId(null)
          if (sawEnd) {
            setPhase({ kind: `ended`, detail: detail ?? undefined })
            return
          }
          if (retryStarting) {
            if (sessionStatusRef.current === `running`) {
              setPhase({ kind: `starting` })
              retryTimer = setTimeout(() => void dial(true), STARTING_RETRY_MS)
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
      clearIdleRelease()
      // Best-effort prompt claim release — closing the socket releases it
      // relay-side anyway.
      const sock = wsRef.current
      if (sock && steeringRef.current && sock.readyState === WebSocket.OPEN) {
        try {
          sock.send(JSON.stringify({ t: `release` }))
        } catch {
          // closing anyway
        }
      }
      wsRef.current = null
      ws?.close()
    }
  }, [attempt, session.id])

  // ── Steering (message-shaped; relay enforces the single claim) ────────────

  /** Auto-release the claim after 60s of no sends (timer resets per send). */
  const scheduleIdleRelease = () => {
    if (idleReleaseRef.current) clearTimeout(idleReleaseRef.current)
    idleReleaseRef.current = setTimeout(() => {
      const sock = wsRef.current
      if (steeringRef.current && sock?.readyState === WebSocket.OPEN) {
        sock.send(JSON.stringify({ t: `release` }))
      }
    }, IDLE_RELEASE_MS)
  }

  /**
   * Steal the claim and forward raw input (chunked ≤4 KiB, never splitting a
   * surrogate pair). The claim is ALWAYS sent: the relay tracks the steerer
   * per CONNECTION while presence only carries a user id, so `steering` can't
   * tell this socket from the same user's second-device claim — skipping the
   * claim there would make the relay silently drop every input frame.
   */
  const sendInput = (data: string): boolean => {
    const sock = wsRef.current
    if (perm !== `steer` || sock?.readyState !== WebSocket.OPEN) return false
    sock.send(JSON.stringify({ t: `claim`, steal: true }))
    for (let i = 0; i < data.length; ) {
      let end = Math.min(i + INPUT_CHUNK_CHARS, data.length)
      const last = end < data.length ? data.charCodeAt(end - 1) : 0
      if (last >= 0xd800 && last <= 0xdbff) end += 1
      sock.send(JSON.stringify({ t: `input`, data: data.slice(i, end) }))
      i = end
    }
    scheduleIdleRelease()
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

  /** Answer an interactive question: raw keystrokes — the desktop passes
   *  single-byte frames to the PTY unwrapped, so the TUI sees keypresses, not
   *  a paste. Verified against the real picker: a digit SELECTS but does not
   *  submit, so single-select answers send the digit + a separate `\r`
   *  (multi-select taps toggle with the digit alone; Submit sends `\r`). */
  const sendAnswer = (key: string, submit = false) => {
    if (!sendInput(key)) return
    if (submit && key !== `\r`) {
      wsRef.current?.send(JSON.stringify({ t: `input`, data: `\r` }))
    }
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

  const jumpToLatest = () => {
    const el = scrollRef.current
    if (el) el.scrollTo({ top: el.scrollHeight })
    setAtBottom(true)
  }

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

  const otherSteerer =
    steererId && steererId !== currentUserId
      ? (viewers.find((v) => v.userId === steererId)?.name ?? `Someone`)
      : null
  const live = phase.kind === `live`
  const sessionEnded = session.status === `ended`
  const composerVisible = live && perm === `steer` && !sessionEnded

  /** The trailing consecutive run of questions is answerable (EXP-78), and a
   *  plan-approval card stays answerable until a real resolution signal —
   *  lagged transcript flushes don't retire a pending picker (EXP-174). */
  const questionIds = useMemo(() => activeQuestionIds(feed), [feed])
  const canAnswer = live && perm === `steer` && !sessionEnded
  /** Render rows: consecutive tool calls collapse into "N tool calls" runs
   *  (EXP-97) — a projection only, the flat feed stays the state. */
  const rows = useMemo(() => groupToolRuns(feed), [feed])
  /** A trailing question/plan means the session is blocked on a human — the
   *  header flips to "Needs your input" so it never looks silently stuck. */
  const awaitingInput = live && questionIds.size > 0

  /** Presence tooltip — every current viewer, the steerer marked. */
  const presenceTitle = viewers
    .map((v) => (v.userId === steererId ? `${v.name} (steering)` : v.name))
    .join(`, `)

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Compact header line — the dock owns the panel frame, so this is just
          identity + presence + controls. */}
      <div className="flex items-center gap-2 border-b border-border px-3 py-1.5">
        <PhaseIndicator
          phase={phase}
          deviceLabel={session.deviceLabel}
          awaitingInput={awaitingInput}
        />
        <div className="min-w-0 flex-1 truncate text-sm">{title}</div>
        {live && viewers.length > 0 && (
          <span
            className="inline-flex shrink-0 items-center gap-1 text-xs text-muted-foreground"
            title={presenceTitle}
          >
            <Eye className="size-3.5" />
            {viewers.length}
          </span>
        )}
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
            size="sm"
            className="shrink-0 text-destructive hover:text-destructive"
            onClick={() => setConfirmKill(true)}
          >
            <OctagonX />
            <span className="hidden md:inline">Kill session</span>
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
                      case `question`:
                        return (
                          <QuestionCard
                            key={item.id}
                            text={item.text}
                            options={item.options}
                            multiSelect={item.multiSelect}
                            planMode={item.planMode}
                            active={questionIds.has(item.id)}
                            canAnswer={canAnswer}
                            onAnswer={sendAnswer}
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
                onClick={jumpToLatest}
              >
                Jump to latest
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

          {/* Steering composer (perm-gated; sending steals the claim) */}
          {composerVisible ? (
            <div className="border-t border-border p-2">
              {steering ? (
                <div className="px-1 pb-1 text-[0.6875rem] text-muted-foreground">
                  You&rsquo;re steering
                </div>
              ) : otherSteerer ? (
                <div className="px-1 pb-1 text-[0.6875rem] text-muted-foreground">
                  {otherSteerer} is steering — sending takes over
                </div>
              ) : null}
              <MessageComposer
                steering={steering}
                onSend={sendMessage}
                onEscape={sendEscape}
              />
            </div>
          ) : live && perm === `view` ? (
            <div className="border-t border-border px-3 py-2 text-xs text-muted-foreground">
              Watching — only team owners or the session owner can steer.
            </div>
          ) : null}
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

/** An interactive question (EXP-78): AskUserQuestion / plan approval. Option
 *  buttons send the option's raw TUI keystroke while the question is still
 *  active (per `activeQuestionIds` — trailing run, or an unresolved plan card,
 *  EXP-174); stale/view-only cards render the options as plain rows.
 *  `planMode` cards (EXP-97) get a dedicated "Plan ready" presentation with
 *  the first option as the primary approve action and the plan rendered as
 *  markdown on expand — labels/keys always come from the wire `options`, the
 *  desktop owns the TUI key mapping. Best-effort by design — the desktop TUI
 *  remains the source of truth. */
function QuestionCard({
  text,
  options,
  multiSelect,
  planMode,
  active,
  canAnswer,
  onAnswer,
}: {
  text: string
  options: QuestionOption[]
  multiSelect: boolean
  planMode: boolean
  /** Still answerable per the feed — the session is blocked on this card. */
  active: boolean
  /** Live + steer perm — whether this client may answer at all. */
  canAnswer: boolean
  onAnswer: (key: string, submit?: boolean) => void
}) {
  const { expanded, setExpanded, clampable } = useClampToggle(text)
  const [picked, setPicked] = useState<Set<string>>(new Set())
  const answerable = active && canAnswer

  const pick = (option: QuestionOption) => {
    // Single-select: digit + Enter submits. Multi-select: digit toggles; the
    // Submit button sends the Enter.
    onAnswer(option.key, !multiSelect)
    setPicked((prev) => {
      const next = new Set(prev)
      if (multiSelect && next.has(option.key)) next.delete(option.key)
      else if (multiSelect) next.add(option.key)
      else {
        next.clear()
        next.add(option.key)
      }
      return next
    })
  }

  return (
    <div
      className={cn(
        `my-1 rounded-md border px-3 py-2`,
        planMode
          ? `border-primary/40 bg-primary/5`
          : `border-amber-500/40 bg-amber-500/5`
      )}
    >
      <div className="flex items-start gap-2">
        {planMode ? (
          <ClipboardList className="mt-0.5 size-3.5 shrink-0 text-primary" />
        ) : (
          <CircleHelp className="mt-0.5 size-3.5 shrink-0 text-amber-400" />
        )}
        <div className="min-w-0 flex-1">
          {planMode && (
            <div className="mb-1 text-xs font-medium text-primary">
              Plan ready
            </div>
          )}
          {planMode && expanded ? (
            // The plan is GFM markdown — render it properly once unfolded
            // (TipTap mounts only on expand; plans can be 16 KiB).
            <div className="text-sm">
              <MarkdownEditor
                markdown={text}
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
              {text}
            </div>
          )}
          {clampable && (
            <ShowMoreButton
              expanded={expanded}
              onToggle={() => setExpanded((v) => !v)}
            />
          )}
          <div
            className={cn(
              `mt-2 flex items-start gap-1`,
              planMode && answerable
                ? `flex-row flex-wrap items-center gap-1.5`
                : `flex-col`
            )}
          >
            {options.map((option, index) =>
              answerable ? (
                <Button
                  key={option.key}
                  // The wire's first option is the plan's primary approve
                  // action ("Approve — auto-accept edits") — promote it.
                  variant={planMode && index === 0 ? `default` : `outline`}
                  size="sm"
                  className={cn(
                    `h-7 justify-start text-xs`,
                    !planMode &&
                      picked.has(option.key) &&
                      `border-amber-500/60 bg-amber-500/15`,
                    planMode &&
                      picked.has(option.key) &&
                      index !== 0 &&
                      `border-primary/60 bg-primary/15`
                  )}
                  onClick={() => pick(option)}
                >
                  {!planMode && (
                    <span className="font-mono text-muted-foreground">
                      {option.key}
                    </span>
                  )}
                  {option.label}
                </Button>
              ) : (
                <span
                  key={option.key}
                  className="text-xs text-muted-foreground"
                >
                  <span className="font-mono">{option.key}</span>
                  {` · ${option.label}`}
                </span>
              )
            )}
          </div>
          {answerable && multiSelect && (
            <Button
              variant="secondary"
              size="sm"
              className="mt-2 h-7 text-xs"
              onClick={() => onAnswer(`\r`)}
            >
              Submit selection
            </Button>
          )}
          {active && !canAnswer && (
            <div className="mt-2 text-xs text-muted-foreground">
              {planMode
                ? `Waiting for approval — you're viewing read-only.`
                : `Waiting for an answer — you're viewing read-only.`}
            </div>
          )}
        </div>
      </div>
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
  steering,
  onSend,
  onEscape,
}: {
  steering: boolean
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
          // Subtle active tint while we hold the steer claim (dark: variant
          // included so it beats the base dark:bg-input/30).
          steering ? `bg-muted/70 dark:bg-muted/70` : `bg-muted/40 dark:bg-muted/40`
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
