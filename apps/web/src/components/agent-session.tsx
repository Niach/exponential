import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react"
import { toast } from "sonner"
import { linkSegments } from "@/lib/linkify"
import { ArrowDown, Check, ChevronDown, ChevronRight, X } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { CodingSession, Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { SessionMergeButton } from "@/components/session-merge-button"
import { useSessionDevice } from "@/hooks/use-session-device"
import { useNow } from "@/hooks/use-now"
import { useSessionAgentUsage } from "@/hooks/use-session-agent-usage"
import { useKillSession } from "@/hooks/use-kill-session"
import { useIsMobile } from "@/hooks/use-mobile"
import { AgentUsageCards } from "@/components/agent-usage-bar"
import { accountCaption } from "@/lib/agent-usage"
import type { SessionDevice } from "@/lib/session-device"
import {
  activeQuestionIds,
  answerKey,
  askStepperView,
  collectSubagents,
  groupFeedRows,
  isAnswerLocked,
  looksLikeMarkdown,
  summarizeSubagentRow,
  visibleSubagentTabs,
  type AnswerState,
  type AnswerStates,
  type SubagentSummary,
} from "@/lib/agent-feed"
import {
  acquireSteerSession,
  type FeedItem,
  type QuestionItem,
  type QuestionOption,
  type SteerSessionStore,
  type ViewerPhase,
} from "@/lib/steer-session-store"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { uploadSessionImageFile } from "@/lib/storage/issue-image-upload"
import {
  buildSteerImageMessage,
  MAX_STEER_IMAGES,
} from "@/lib/steer-image-message"
import { splitUnifiedDiff } from "@/lib/unified-diff"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { FileDiffList } from "@/components/diff-view"

// EXP-317: the session glyphs the native clients also draw resolve through
// the shared registry (packages/icons/icons.json).
const CodingAssistantIcon = conceptIcon(`coding-assistant`)
const CodingPlanIcon = conceptIcon(`coding-plan`)
const CodingStopIcon = conceptIcon(`coding-stop`)
const CodingSubagentIcon = conceptIcon(`coding-subagent`)
const CodingToolIcon = conceptIcon(`coding-tool`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiDeviceOfflineIcon = conceptIcon(`ui-device-offline`)
const UiFullscreenIcon = conceptIcon(`ui-fullscreen`)
const UiFullscreenExitIcon = conceptIcon(`ui-fullscreen-exit`)
const UiHelpIcon = conceptIcon(`ui-help`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiPermissionIcon = conceptIcon(`ui-permission`)
const UiRefreshIcon = conceptIcon(`ui-refresh`)
const UiSendIcon = conceptIcon(`ui-send`)
const UiUsageIcon = conceptIcon(`ui-usage`)
// EXP-529: multi-select options carry an explicit checkbox state (Android
// parity) — the amber tint alone read as "nothing selected".
const UiSelectedIcon = conceptIcon(`ui-selected`)
const UiUnselectedIcon = conceptIcon(`ui-unselected`)

// The custom-rendered agent-session viewer (EXP-63 — the web port of the
// mobile "Agent session" chat view, EXP-32). NO terminal rendering: the
// viewer joins the steer relay's scrubbed ACTIVITY channel
// ({"t":"join","channel":"activity"}, apps/steer-relay/src/protocol.ts) and
// renders structured events — narration bubbles + compact tool rows, a
// pinned "Latest changes" diff above the composer — never raw PTY bytes.
// Steering is message-shaped like mobile — chunked input + a SEPARATE `\r`
// frame (EXP-312: no operator claim, no view/steer perm split — the mint is
// owner-only, so a live connection just steers); question answers ride the
// semantic `answer` frame (steer protocol v2, EXP-249) whenever the desktop
// publishes question ids.
// Since EXP-106 this view is mounted ONLY by the global agent dock
// (components/agent-dock) — one at a time — so it always auto-connects and
// delegates its chrome (title, collapse) to the dock; the "coding now" rows +
// remote-start affordances moved to issue-coding-rows.tsx.

// ── Wire protocol ────────────────────────────────────────────────────────────
// EXP-621: the relay protocol handling, the connection lifecycle and the feed
// reducer all moved to lib/steer-session-store.ts — a module-level per-session
// store that OUTLIVES this view, so collapsing the dock or navigating away
// keeps the socket, the feed and the composer draft. This file only renders.


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

/** EXP-688: one run's identity, resolved once by the dock and rendered by
 * both the dock tab and the mobile session header so they cannot drift. */
export interface SessionIdentity {
  /** `EXP-688` — null for action, batch and not-yet-synced issue runs. */
  identifier: string | null
  /** The issue title, an action's name snapshot, or `Batch run`. */
  subject: string
}

// Mounted ONLY by the global agent dock (one at a time), keyed by session id.
// Always auto-connects; the caller owns the membership + config.enabled gating
// (the relay enforces both regardless) and supplies the `title` + `onCollapse`
// chrome. Session-scoped — the "coding now" rows live in issue-coding-rows.tsx.
export function AgentSessionView({
  session,
  currentUserId,
  identity,
  prIssue,
  onCollapse,
  isFullscreen,
  onToggleFullscreen,
}: {
  session: CodingSession
  currentUserId: string
  /** EXP-688: what this run IS — the mono identifier (absent for action and
   *  batch runs) and its human subject. The MOBILE header names it; the
   *  desktop header carries no identity at all, because the dock tab under
   *  the panel already does. */
  identity: SessionIdentity
  /** EXP-678: the issue whose PR this session would merge — the linked issue,
   *  or a batch run's resolved representative (EXP-535). Absent (action runs,
   *  still syncing) = no Merge pill. */
  prIssue?: Issue
  /** Collapse the dock panel (the socket tears down on unmount). */
  onCollapse: () => void
  /** Fullscreen toggle chrome (EXP-184) — owned by the dock; absent = no button. */
  isFullscreen?: boolean
  onToggleFullscreen?: () => void
}) {
  // EXP-621: the connection lives in a module-level per-session store that
  // outlives this view — mounting subscribes to the retained state (feed,
  // phase, answers) and dials only when nothing is connected yet, so
  // reopening a session renders instantly with no reconnect phase.
  const store = useMemo(() => acquireSteerSession(session.id), [session.id])
  const { phase, feed, latestDiff, answerStates, connected } =
    useSyncExternalStore(store.subscribe, store.getSnapshot)
  useEffect(() => store.connect(), [store])
  // The synced row is the truth for "still running" inside the redial loops.
  useEffect(
    () => store.noteSessionStatus(session.status),
    [store, session.status]
  )

  const [diffOpen, setDiffOpen] = useState(false)
  const [usageOpen, setUsageOpen] = useState(false)
  const [atBottom, setAtBottom] = useState(true)
  /** EXP-356: the selected conversation tab — `null` is the main agent; a
   *  subagent id focuses that agent's stream. Falls back to Main whenever the
   *  id vanishes from the feed (an `activity_reset` replay). */
  const [agentTab, setAgentTab] = useState<string | null>(null)

  const scrollRef = useRef<HTMLDivElement | null>(null)

  const sendMessage = (text: string): boolean => store.sendMessage(text)
  const answerQuestion = (
    item: QuestionItem,
    keys: string[],
    labels: string[],
    text?: string
  ) => store.answerQuestion(item, keys, labels, text)
  const toggleLegacyOption = (key: string) => store.toggleLegacyOption(key)

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

  // EXP-440: rows now finish laying out AFTER the pin effect below has run —
  // a markdown bubble mounts its editor deferred (immediatelyRender: false)
  // and its images size only once decoded, so the feed grows under a scroll
  // position that was already at the bottom. Watch the content column and
  // re-pin on every such growth. Ref-mirrored `atBottom` because the observer
  // is installed once per mounted column, not per render.
  const atBottomRef = useRef(atBottom)
  atBottomRef.current = atBottom
  const contentObserverRef = useRef<ResizeObserver | null>(null)
  const setContentRef = useCallback((node: HTMLDivElement | null) => {
    contentObserverRef.current?.disconnect()
    contentObserverRef.current = null
    if (!node || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => {
      if (!atBottomRef.current) return
      const el = scrollRef.current
      if (el) el.scrollTop = el.scrollHeight
    })
    observer.observe(node)
    contentObserverRef.current = observer
  }, [])

  useEffect(() => () => contentObserverRef.current?.disconnect(), [])

  useEffect(() => {
    if (!atBottom) return
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
    // needsInput re-pins when the EXP-389 "Working…" footer toggles (its
    // other inputs — phase, the feed-derived question set — are covered).
  }, [feed, atBottom, phase.kind, session.needsInput])

  // Switching conversation tabs re-pins to the newest event (EXP-356).
  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
    setAtBottom(true)
  }, [agentTab])

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
  // EXP-312: live implies ownership — the mint refuses everyone else.
  // EXP-621: the composer stays MOUNTED through connection flaps (only send
  // is disabled) — unmounting it on a phase change was how a slow-consumer
  // eviction ate a typed draft. It only leaves with the session itself.
  const composerVisible = !sessionEnded && phase.kind !== `ended`
  // EXP-678: an open PR on a still-live run is mergeable right here. The
  // server ends the session on merge (EXP-498) and the prState echo hides
  // the pill again — no local state to unwind.
  const canMerge = composerVisible && prIssue?.prState === `open`
  // EXP-706: a conflicted merge swaps the pill for the "Fix conflicts" run,
  // which needs the relay. The dock only mounts this view for a member with
  // steering on, but the config is the honest gate.
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(steerConfig?.enabled)

  /** Identity-scoped questions stay answerable until they resolve; legacy
   *  cards fall back to the trailing-run heuristic, with a plan-approval card
   *  answerable until a real resolution signal — lagged transcript flushes
   *  don't retire a pending picker (EXP-174). */
  const questionIds = useMemo(() => activeQuestionIds(feed), [feed])
  const canAnswer = live && !sessionEnded
  /** Render rows: consecutive tool calls collapse into "N tool calls" runs
   *  (EXP-97), one ask's questions into a stepper and a subagent's work into
   *  its own group — a projection only, the flat feed stays the state. */
  const rows = useMemo(() => groupFeedRows(feed), [feed])
  /** EXP-356: the subagents seen so far — one conversation tab each. EXP-387:
   *  the strip only shows the still-running ones (plus the focused tab). */
  const agents = useMemo(() => collectSubagents(feed), [feed])
  const visibleTabs = useMemo(
    () => visibleSubagentTabs(agents, agentTab),
    [agents, agentTab]
  )
  const activeAgent =
    agentTab !== null && visibleTabs.some((a) => a.subagentId === agentTab)
      ? agentTab
      : null
  /** The focused agent's stream (its lifecycle markers + tool calls). */
  const agentItems = useMemo(
    () =>
      activeAgent === null
        ? []
        : feed.filter(
            (item) =>
              (item.kind === `tool` || item.kind === `subagent`) &&
              item.subagentId === activeAgent
          ),
    [feed, activeAgent]
  )
  /** A trailing question/plan means the session is blocked on a human — the
   *  header flips to "Needs your input" so it never looks silently stuck. */
  const awaitingInput = live && questionIds.size > 0
  /** An active plan-approval card: the composer's free text IS the "tell
   *  Claude what to change" path (the desktop Escs the picker and types the
   *  message), so the placeholder says so instead of a dead option button. */
  const planPending = useMemo(
    () =>
      live &&
      feed.some(
        (item) =>
          item.kind === `question` &&
          item.planMode === true &&
          questionIds.has(item.id)
      ),
    [live, feed, questionIds]
  )
  /** EXP-389: the agent is actively working — live and nothing waiting on
   *  the user (no active question card, synced needs_input clear; all three
   *  agents drive the flag). Mobile parity. */
  const working = live && !sessionEnded && !awaitingInput && !session.needsInput
  /** EXP-549/550: the host machine per the synced devices row — its RENAMED
   *  label, and whether it is offline right now. */
  const device = useSessionDevice(session)
  /** EXP-484: the host machine's fresh rate-limit report for THIS run's
   *  agent, or null (finished run, other agent, stale or absent numbers). */
  const agentUsage = useSessionAgentUsage(session)
  const usageNow = useNow(30_000)
  const isMobile = useIsMobile()
  /** EXP-550: no live stream AND the host machine is offline (lid closed,
   *  usage-limit pause…) — the agent is PAUSED on that machine, not starting
   *  and not gone. The synced row stays `running`, so it resumes when the
   *  device returns; the redial loop below keeps trying and picks the stream
   *  back up on its own. Renders grey with honest copy instead of an endless
   *  "Agent starting…" spinner. */
  const paused =
    device.online === false &&
    !sessionEnded &&
    (phase.kind === `starting` ||
      phase.kind === `connecting` ||
      phase.kind === `idle` ||
      phase.kind === `closed`)
  /** EXP-688: the kill confirmation is shared with the dock tab's X. Live
   *  implies ownership (EXP-312), and only a live stream can be killed. */
  const {
    canKill: ownsLiveRow,
    requestKill,
    dialog: killDialog,
  } = useKillSession(session, currentUserId, device.label, paused)
  const canKill = live && ownsLiveRow
  const pausedTitle = `${device.label ?? `The device`} is offline`
  const pausedBody = `The agent is paused on that machine and continues when it comes back online.`
  // The `closed` phase (relay `bye publisher_lost`) does not redial on its
  // own — a viewer that watched the lid close would sit on "Disconnected"
  // after the machine woke. Nudge the store once the device flips back
  // online so the stream resumes without a click. EXP-625: the nudge is
  // `kick`, which decides for itself whether this store is actually stuck
  // (it also shortcuts a `starting` backoff step); the phase test that used
  // to live here moved inside it.
  const deviceOnline = device.online
  const wasOfflineRef = useRef(false)
  useEffect(() => {
    if (deviceOnline === false) {
      wasOfflineRef.current = true
      return
    }
    if (deviceOnline === true && wasOfflineRef.current) {
      wasOfflineRef.current = false
      store.kick(`device-online`)
    }
  }, [deviceOnline, store])

  /** Pinned "Latest changes". EXP-678: once the PR is open the strip shares
   *  its row with a glass Merge pill — the trigger shrinks, the pill sits on
   *  the right at the same height, and the expanded diff still spans the full
   *  width. The pill alone holds the row when no diff has arrived yet.
   *  EXP-688: on mobile it is a floating glass row over the feed. */
  const changesBar =
    latestDiff || canMerge ? (
      <Collapsible
        open={diffOpen && Boolean(latestDiff)}
        onOpenChange={setDiffOpen}
        className={
          isMobile
            ? `overflow-hidden rounded-xl border border-glass-stroke-card bg-glass-card shadow-lg backdrop-blur-md`
            : `border-t border-border`
        }
      >
        <div className={cn(`flex items-stretch`, !isMobile && `bg-muted/30`)}>
          {latestDiff ? (
            <CollapsibleTrigger className="flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-xs transition-colors hover:bg-muted/50">
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
          ) : (
            <span className="flex-1" />
          )}
          {canMerge && prIssue && (
            <div className="flex shrink-0 items-center py-1 pr-2 pl-1">
              <SessionMergeButton
                variant="glass"
                size="sm"
                label="Merge"
                prState={prIssue.prState}
                prNumber={prIssue.prNumber}
                issueId={prIssue.id}
                branch={prIssue.branch}
                teamId={prIssue.teamId}
                currentUserId={currentUserId}
                steerEnabled={steerEnabled}
              />
            </div>
          )}
        </div>
        {latestDiff && (
          <CollapsibleContent>
            <div className="max-h-72 overflow-y-auto overscroll-contain border-t border-border/60">
              <FileDiffList files={diffFiles} />
            </div>
          </CollapsibleContent>
        )}
      </Collapsible>
    ) : null
  /** The floating bar's footprint, so the newest message still scrolls clear
   *  of it (and "Jump to bottom" lands above it). */
  const floatingBar = isMobile && changesBar !== null

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* EXP-688: two headers, because the two surfaces already say different
          things. On DESKTOP the dock tab under the panel carries the identity,
          the phase and the kill, so the header keeps only the panel controls.
          On MOBILE the takeover IS the whole screen: it names the run over the
          phase caption and hides usage + kill behind a "…" menu, exactly like
          the native session screens. */}
      {isMobile ? (
        <div className="flex items-center gap-1 border-b border-border px-1 py-1.5">
          <Button
            variant="ghost"
            size="icon"
            className="shrink-0"
            aria-label="Collapse session"
            onClick={onCollapse}
          >
            <ChevronDown />
          </Button>
          <div className="flex min-w-0 flex-1 flex-col items-center">
            <div className="flex w-full min-w-0 items-center justify-center gap-1.5">
              <PhaseDot
                phase={phase}
                awaitingInput={awaitingInput}
                paused={paused}
              />
              {identity.identifier && (
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {identity.identifier}
                </span>
              )}
              <span className="min-w-0 truncate text-sm font-medium">
                {identity.subject}
              </span>
            </div>
            <span className="max-w-full truncate text-[11px] text-muted-foreground">
              {phaseLabel(phase, device, awaitingInput, paused)}
            </span>
          </div>
          {/* A dropped stream redials from here too — a phone has no desktop
              header to fall back on. */}
          {phase.kind === `closed` && !paused && (
            <Button
              variant="outline"
              size="sm"
              className="shrink-0"
              onClick={() => store.reconnect()}
            >
              <UiRefreshIcon />
              Reconnect
            </Button>
          )}
          {/* A finished run with no fresh numbers has nothing to offer, so the
              trigger goes away rather than opening an empty menu (its width
              stays, so the title does not jump). */}
          {!agentUsage && !canKill && <span className="size-8 shrink-0" />}
          {(agentUsage || canKill) && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  className="h-8 w-8 shrink-0 p-0"
                  aria-label="Session actions"
                >
                  <UiMoreIcon />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {agentUsage && (
                  <DropdownMenuItem onSelect={() => setUsageOpen(true)}>
                    <UiUsageIcon className="size-4" />
                    Usage
                  </DropdownMenuItem>
                )}
                {canKill && (
                  <DropdownMenuItem variant="destructive" onSelect={requestKill}>
                    <CodingStopIcon className="size-4" />
                    Kill session
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>
      ) : (
        <div className="flex items-center gap-2 border-b border-border px-3 py-1.5">
          <span className="flex-1" />
          {phase.kind === `closed` && !paused && (
            <Button
              variant="outline"
              size="sm"
              className="shrink-0"
              onClick={() => store.reconnect()}
            >
              <UiRefreshIcon />
              Reconnect
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
              {isFullscreen ? <UiFullscreenExitIcon /> : <UiFullscreenIcon />}
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
      )}

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card/40">
          {/* EXP-356: conversation tabs — Main plus one per RUNNING subagent
              (ended tabs are dropped, EXP-387). */}
          {visibleTabs.length > 0 && (
            <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-border/60 px-2 py-1">
              <AgentTab
                label="Main"
                active={activeAgent === null}
                onClick={() => setAgentTab(null)}
              />
              {visibleTabs.map((agent) => (
                <AgentTab
                  key={agent.subagentId}
                  label={agent.agentType}
                  running={!agent.done}
                  active={activeAgent === agent.subagentId}
                  onClick={() => setAgentTab(agent.subagentId)}
                />
              ))}
            </div>
          )}
          {/* The activity feed (bottom-anchored, follow-scroll) */}
          <div className="relative min-h-0 flex-1">
            <div
              ref={scrollRef}
              onScroll={handleFeedScroll}
              // EXP-619: the feed rides its own bottom edge, so without
              // containment every downward wheel tick over the terminal
              // scrolled the page behind the dock instead.
              className="h-full overflow-y-auto overscroll-contain"
            >
              {feed.length === 0 && paused ? (
                <CenteredState>
                  <UiDeviceOfflineIcon className="size-5 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">
                    {pausedTitle}
                  </span>
                  <span className="max-w-xs text-xs text-muted-foreground/70">
                    {pausedBody}
                  </span>
                </CenteredState>
              ) : feed.length === 0 &&
                (phase.kind === `connecting` || phase.kind === `starting`) ? (
                <CenteredState>
                  <UiLoadingIcon className="size-4 animate-spin text-muted-foreground" />
                  <span className="text-xs text-muted-foreground">
                    {phase.kind === `starting`
                      ? `The agent is starting. Waiting for the live stream…`
                      : `Connecting…`}
                  </span>
                </CenteredState>
              ) : feed.length === 0 && live && !latestDiff ? (
                <CenteredState>
                  <span className="text-sm text-muted-foreground">
                    Waiting for activity…
                  </span>
                  <span className="text-xs text-muted-foreground/70">
                    This session isn&apos;t publishing an activity feed. It may
                    be marked private on the desktop, or the desktop app needs
                    an update.
                  </span>
                </CenteredState>
              ) : activeAgent !== null ? (
                <div
                  ref={setContentRef}
                  className={cn(
                    `flex min-h-full flex-col justify-end gap-0.5 px-3 py-2`,
                    // Room for the floating changes bar, so the newest row
                    // still scrolls fully clear of it (EXP-688).
                    floatingBar && `pb-14`
                  )}
                >
                  <AgentConversation
                    summary={agents.find((a) => a.subagentId === activeAgent)}
                    items={agentItems}
                  />
                </div>
              ) : (
                <div
                  ref={setContentRef}
                  className={cn(
                    `flex min-h-full flex-col justify-end gap-0.5 px-3 py-2`,
                    // Room for the floating changes bar, so the newest row
                    // still scrolls fully clear of it (EXP-688).
                    floatingBar && `pb-14`
                  )}
                >
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
                            active={
                              live && item.id === feed[feed.length - 1]?.id
                            }
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
                  {/* EXP-389: the agent-is-busy footer under the newest
                      event (mobile parity) — main conversation only. */}
                  {working && <WorkingIndicatorRow />}
                </div>
              )}
            </div>
            {!atBottom && feed.length > 0 && (
              <Button
                variant="secondary"
                size="sm"
                className={cn(
                  `absolute left-1/2 h-7 -translate-x-1/2 rounded-full border border-border shadow-md`,
                  floatingBar ? `bottom-16` : `bottom-2`
                )}
                onClick={jumpToBottom}
              >
                Jump to bottom
                <ArrowDown />
              </Button>
            )}
            {/* EXP-688: the mobile changes bar floats over the feed's bottom
                edge instead of taking a slice of it. */}
            {floatingBar && (
              <div className="absolute inset-x-0 bottom-0 px-3 pb-2">
                {changesBar}
              </div>
            )}
          </div>

          {/* Status banners (feed retained above) */}
          {phase.kind === `ended` && (
            <div className="border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              {phase.detail ?? `The session has ended.`}
            </div>
          )}
          {paused && feed.length > 0 && (
            <div className="flex items-center gap-1.5 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              <UiDeviceOfflineIcon className="size-3 shrink-0" />
              <span>
                {`Paused — ${pausedTitle}. ${pausedBody}`}
              </span>
            </div>
          )}
          {phase.kind === `closed` && !paused && (
            <div className="border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              {phase.detail ?? `Connection lost.`}
            </div>
          )}
          {phase.kind === `starting` && !paused && feed.length > 0 && (
            <div className="flex items-center gap-1.5 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              <UiLoadingIcon className="size-3 animate-spin" />
              The agent is starting. Waiting for the live stream…
            </div>
          )}

          {/* Desktop keeps the row pinned between the feed and the composer;
              on mobile it FLOATS over the feed (above, inside the scroll
              wrapper) so it costs the conversation no height. */}
          {!isMobile && changesBar}

          {/* Steering composer. Steering is fully seamless (EXP-312) — no
              captions, no operator state; live implies ownership. */}
          {composerVisible && (
            <div className="border-t border-border p-2">
              <MessageComposer
                store={store}
                // `connected` matters beyond the phase: a silent slow-consumer
                // redial keeps `live` while the socket is briefly down, and
                // the send button should dim honestly for that gap.
                live={live && connected}
                onSend={sendMessage}
                sessionId={session.id}
                placeholder={
                  planPending ? `Tell Claude what to change…` : undefined
                }
              />
            </div>
          )}
      </div>

      {killDialog}

      {/* EXP-688: usage is a SHEET on mobile, not a hairline under the header
          — every window the machine reports, grouped the way the agent's own
          app groups them. */}
      {agentUsage && (
        <Dialog open={usageOpen} onOpenChange={setUsageOpen}>
          <DialogContent
            className="sm:max-w-sm"
            aria-describedby={undefined}
          >
            <DialogHeader>
              <DialogTitle>Usage</DialogTitle>
            </DialogHeader>
            <div className="space-y-3">
              {/* Above the cards, without the agent prefix — the natives'
                  Usage sheets do the same (hand-mirrored strings, EXP-484). */}
              {agentUsage.account && (
                <p className="text-[11px] text-muted-foreground">
                  {accountCaption(agentUsage.account)}
                </p>
              )}
              <AgentUsageCards usage={agentUsage.usage} now={usageNow} />
            </div>
          </DialogContent>
        </Dialog>
      )}
    </div>
  )
}

// ── Pieces ───────────────────────────────────────────────────────────────────

/** What the viewer's connection reads as: `Live · macbook`, `Needs your
 * input · macbook`, `Paused · macbook is offline`, `Session ended`. EXP-688:
 * the mobile header demotes it to a caption under the run's name.
 *
 * `awaitingInput`: live but blocked on a trailing question/plan — waiting for
 * a human, not stuck (EXP-97). `paused` (EXP-550): no stream and the host
 * machine is offline, which is neither starting nor gone. */
function phaseLabel(
  phase: ViewerPhase,
  /** EXP-549: the host machine per the synced devices row (renamed label). */
  device: SessionDevice,
  awaitingInput: boolean,
  paused: boolean
): string {
  const deviceLabel = device.label
  if (paused) return `Paused · ${deviceLabel ?? `device`} is offline`
  if (phase.kind === `live`) {
    if (awaitingInput) {
      return deviceLabel ? `Needs your input · ${deviceLabel}` : `Needs your input`
    }
    return deviceLabel ? `Live · ${deviceLabel}` : `Live`
  }
  if (phase.kind === `starting`) return `Agent starting…`
  if (phase.kind === `connecting` || phase.kind === `idle`) return `Connecting…`
  if (phase.kind === `ended`) return `Session ended`
  return `Disconnected`
}

/** The status dot that leads the phase: green live, amber pulsing while it
 * connects, amber steady while it waits on a human, grey otherwise. */
function PhaseDot({
  phase,
  awaitingInput = false,
  paused = false,
}: {
  phase: ViewerPhase
  awaitingInput?: boolean
  paused?: boolean
}) {
  const connecting =
    !paused && (phase.kind === `connecting` || phase.kind === `starting`)
  const awaiting = phase.kind === `live` && awaitingInput
  return (
    <span
      className={cn(
        `size-2 shrink-0 rounded-full`,
        phase.kind === `live` && (awaiting ? `bg-amber-400` : `bg-emerald-500`),
        connecting && `animate-pulse bg-amber-400`,
        !connecting && phase.kind !== `live` && `bg-muted-foreground/40`
      )}
    />
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
/** The trailing "agent is busy" row (EXP-389): a gently pulsing "Working…"
 *  under the newest event whenever the session is live and nothing waits on
 *  the user — without it a feed that ends in tool rows gives no cue whether
 *  the agent is still going. Static under reduced motion. */
function WorkingIndicatorRow() {
  return (
    <div className="flex items-center gap-2 py-1.5 motion-safe:animate-pulse">
      <CodingAssistantIcon className="size-3 shrink-0 text-muted-foreground/60" />
      <span className="text-xs text-muted-foreground">Working…</span>
    </div>
  )
}

/** Read-only editors never emit — one shared handler keeps every feed bubble
 *  out of the "new function identity per render" trap. */
const noop = () => {}

/** The shared read-only markdown renderer — the same TipTap pipeline the issue
 *  and comment bodies use, in its compact `chat` presentation, so a feed bubble
 *  draws headings, lists, code and IMAGES exactly like the rest of the product
 *  (EXP-440). `linkify` is on because agent text carries bare URLs. */
function FeedMarkdown({
  text,
  ariaLabel,
  hardBreaks,
}: {
  text: string
  ariaLabel: string
  /** Chat text is line-broken by hand — a single newline is a real break. */
  hardBreaks?: boolean
}) {
  return (
    <MarkdownEditor
      markdown={text}
      editable={false}
      onChange={noop}
      appearance="chat"
      linkify
      hardBreaks={hardBreaks}
      ariaLabel={ariaLabel}
    />
  )
}

/** Feed text: markdown when it carries any (EXP-440), else the plain
 *  linkified rendering — most narration is one prose line, and spinning up a
 *  TipTap instance per line would be pure overhead. */
function FeedText({
  text,
  ariaLabel,
  hardBreaks,
}: {
  text: string
  ariaLabel: string
  hardBreaks?: boolean
}) {
  if (looksLikeMarkdown(text)) {
    return (
      <FeedMarkdown text={text} ariaLabel={ariaLabel} hardBreaks={hardBreaks} />
    )
  }
  return (
    <div className="whitespace-pre-wrap break-words">
      {linkSegments(text).map((segment, i) =>
        segment.href ? (
          // break-all: the EXP-430 sign-in URL has no break points.
          <a
            key={i}
            href={segment.href}
            target="_blank"
            rel="noreferrer"
            className="break-all text-primary underline underline-offset-2 hover:opacity-80"
          >
            {segment.text}
          </a>
        ) : (
          <Fragment key={i}>{segment.text}</Fragment>
        ),
      )}
    </div>
  )
}

// memo: a live feed re-renders on every incoming frame, and a bubble that
// rendered markdown owns a TipTap editor — re-running that for unchanged text
// is the one cost worth avoiding here.
const NarrationBubble = memo(function NarrationBubble({
  text,
}: {
  text: string
}) {
  return (
    // EXP-696: no bubble, matching the natives (EXP-274) — a small assistant
    // glyph and the agent's prose running the full width of the feed.
    <div className="flex items-start gap-2 py-1">
      <CodingAssistantIcon className="mt-1.5 size-3 shrink-0 text-muted-foreground/60" />
      <div className="min-w-0 flex-1 text-sm text-foreground/90">
        <FeedText text={text} ariaLabel="Agent message" hardBreaks />
      </div>
    </div>
  )
})

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
const UserMessageBubble = memo(function UserMessageBubble({
  text,
}: {
  text: string
}) {
  const { expanded, setExpanded, clampable } = useClampToggle(text)
  return (
    <div className="flex justify-end py-1 pl-8">
      {/* EXP-696: the natives' neutral glass bubble, not a primary tint —
          slightly brighter than the assistant's glass sections so the
          sender's own turn reads apart from the feed. */}
      <div className="min-w-0 rounded-xl border border-glass-stroke-strong bg-glass-active px-3 py-2 text-sm text-foreground/90">
        {/* A height clamp, not `line-clamp`: line clamping needs a plain text
            flow, and a markdown body is a stack of blocks. */}
        <div
          className={cn(clampable && !expanded && `max-h-40 overflow-hidden`)}
        >
          <FeedText text={text} ariaLabel="Your message" hardBreaks />
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
})

type AnswerHandler = (
  item: QuestionItem,
  keys: string[],
  labels: string[],
  /** EXP-513: the typed reply for a `freeText` option. */
  text?: string
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
  /** Live (and not ended) — whether this client may answer at all. */
  canAnswer: boolean
  answerState?: AnswerState
  onAnswer: AnswerHandler
  onToggleLegacy: (key: string) => void
  /** `plan`/`submit` promote the first option to the primary action. */
  variant?: `default` | `plan` | `submit`
}) {
  const [picked, setPicked] = useState<string[]>([])
  /** EXP-513: the `freeText` option whose inline input is open (its key). */
  const [freeTextKey, setFreeTextKey] = useState<string | null>(null)
  const [freeTextValue, setFreeTextValue] = useState(``)
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
        <UiLoadingIcon className="size-3 shrink-0 animate-spin" />
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
    // EXP-513: a free-text row collects the reply first — nothing is sent
    // until the input submits (the desktop types it into the TUI row).
    if (option.freeText === true && semantic) {
      setFreeTextKey((prev) => (prev === option.key ? null : option.key))
      return
    }
    onAnswer(item, [option.key], [option.label])
  }

  const submitPicked = () => {
    if (!answerable) return
    onAnswer(item, semantic ? picked : [`\t`], labelsFor(picked))
  }

  const submitFreeText = () => {
    if (!answerable || freeTextKey === null) return
    const text = freeTextValue.trim()
    if (text.length === 0) return
    onAnswer(item, [freeTextKey], [text], text)
    setFreeTextKey(null)
    setFreeTextValue(``)
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
                (picked.includes(option.key) || freeTextKey === option.key) &&
                  (variant === `default`
                    ? `border-amber-500/60 bg-amber-500/15`
                    : `border-primary/60 bg-primary/15`)
              )}
              onClick={() => choose(option)}
            >
              {variant === `default` &&
                (item.multiSelect ? (
                  picked.includes(option.key) ? (
                    <UiSelectedIcon className="size-3.5 shrink-0 text-foreground" />
                  ) : (
                    <UiUnselectedIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  )
                ) : (
                  <span className="font-mono text-muted-foreground">
                    {option.key}
                  </span>
                ))}
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
      {answerable && freeTextKey !== null && (
        <div className="mt-2 flex w-full items-center gap-1.5">
          <Input
            autoFocus
            value={freeTextValue}
            maxLength={4000}
            placeholder="Type your answer…"
            className="h-7 flex-1 text-xs"
            onChange={(e) => setFreeTextValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                submitFreeText()
              } else if (e.key === `Escape`) {
                e.preventDefault()
                setFreeTextKey(null)
              }
            }}
          />
          <Button
            variant="secondary"
            size="sm"
            className="h-7 text-xs"
            disabled={freeTextValue.trim().length === 0}
            onClick={submitFreeText}
          >
            Answer
          </Button>
        </div>
      )}
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
          No confirmation from the desktop. Pick again to retry.
        </div>
      )}
      {active && !canAnswer && (
        <div className="mt-2 text-xs text-muted-foreground">
          {item.planMode
            ? `Waiting for approval. You're viewing read-only.`
            : `Waiting for an answer. You're viewing read-only.`}
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
          <CodingPlanIcon className="mt-0.5 size-3.5 shrink-0 text-primary" />
        ) : (
          <UiHelpIcon className="mt-0.5 size-3.5 shrink-0 text-amber-400" />
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
                onChange={noop}
                linkify
              />
            </div>
          ) : (
            <div
              className={cn(
                `text-sm text-foreground/90`,
                clampable && !expanded && `max-h-56 overflow-hidden`
              )}
            >
              <FeedText text={item.text} ariaLabel="Question" />
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
        <UiHelpIcon className="mt-0.5 size-3.5 shrink-0 text-amber-400" />
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
              <div className="text-sm text-foreground/90">
                <FeedText text={current.item.text} ariaLabel="Question" />
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
                <UiLoadingIcon className="size-3 animate-spin" />
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

/** A permission prompt the agent raised (protocol v2) — informational: the
 *  decision lives in the desktop TUI (a grid-confirmed dialog publishes an
 *  answerable question card instead, EXP-455/529). While it is the live
 *  trailing event, point at the working escape hatch — a composer message
 *  reaches the paused TUI — instead of dead-ending the viewer. */
function PermissionRow({
  tool,
  detail,
  active = false,
}: {
  tool: string
  detail?: string
  active?: boolean
}) {
  return (
    <div className="min-w-0 py-0.5 pl-0.5">
      <div className="flex min-w-0 items-center gap-2">
        <UiPermissionIcon className="size-3 shrink-0 text-amber-400/70" />
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
      {active && (
        <div className="pl-5 text-[0.6875rem] text-muted-foreground">
          Approve on the desktop, or reply below to continue.
        </div>
      )}
    </div>
  )
}

/** A subagent's work (protocol v2): its lifecycle events plus every tool call
 *  it made, collapsed into one expandable row like a tool run. Expandable only
 *  when there ARE tool calls — the detail is always visible collapsed, so a
 *  chevron on an empty group would expand to nothing (EXP-350). */
function SubagentGroupRow({ items }: { items: FeedItem[] }) {
  const [expanded, setExpanded] = useState(false)
  const tools = items.filter(
    (i): i is Extract<FeedItem, { kind: `tool` }> => i.kind === `tool`
  )
  const { agentType, done, detail } = summarizeSubagentRow(items)
  const expandable = tools.length > 0
  const header = (
    <>
      <CodingSubagentIcon className="size-3 shrink-0 text-muted-foreground/60" />
      <span className="shrink-0 text-xs font-medium">{agentType}</span>
      {!done && <UiLoadingIcon className="size-3 shrink-0 animate-spin" />}
      <span className="shrink-0 text-[0.6875rem]">
        {done ? `done` : `running`}
        {tools.length > 0 &&
          ` · ${tools.length} tool call${tools.length === 1 ? `` : `s`}`}
      </span>
      {detail && (
        <span className="truncate text-[0.6875rem]" title={detail}>
          {detail}
        </span>
      )}
    </>
  )
  if (!expandable) {
    return (
      <div className="flex min-w-0 items-center gap-2 py-0.5 pl-0.5 text-muted-foreground">
        {header}
      </div>
    )
  }
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
        {header}
      </button>
      {expanded && (
        <div className="ml-5">
          {tools.map((tool) => (
            <ToolRow key={tool.id} name={tool.name} detail={tool.detail} />
          ))}
        </div>
      )}
    </div>
  )
}

/** One conversation tab chip (EXP-356) — Main or a subagent. */
function AgentTab({
  label,
  active,
  running,
  onClick,
}: {
  label: string
  active: boolean
  running?: boolean
  onClick: () => void
}) {
  return (
    <Button
      variant={active ? `secondary` : `ghost`}
      size="sm"
      className="h-6 shrink-0 gap-1.5 px-2 text-xs"
      onClick={onClick}
    >
      {label}
      {running && <UiLoadingIcon className="size-3 shrink-0 animate-spin" />}
    </Button>
  )
}

/** A focused subagent conversation (EXP-356): its delegation summary on top,
 *  then every tool call as a full row — the flat feed stays the state, this
 *  is a per-agent projection like the grouped main view. */
function AgentConversation({
  summary,
  items,
}: {
  summary?: SubagentSummary
  items: FeedItem[]
}) {
  const tools = items.filter(
    (i): i is Extract<FeedItem, { kind: `tool` }> => i.kind === `tool`
  )
  return (
    <>
      {summary && (
        <div className="flex min-w-0 items-center gap-2 py-0.5 pl-0.5 text-muted-foreground">
          <CodingSubagentIcon className="size-3 shrink-0 text-muted-foreground/60" />
          <span className="shrink-0 text-xs font-medium">
            {summary.agentType}
          </span>
          {!summary.done && (
            <UiLoadingIcon className="size-3 shrink-0 animate-spin" />
          )}
          <span className="shrink-0 text-[0.6875rem]">
            {summary.done ? `done` : `running`}
          </span>
          {summary.detail && (
            <span className="truncate text-[0.6875rem]" title={summary.detail}>
              {summary.detail}
            </span>
          )}
        </div>
      )}
      {tools.length === 0 ? (
        <div className="py-1 pl-0.5 text-xs text-muted-foreground">
          No tool calls yet.
        </div>
      ) : (
        tools.map((tool) => (
          <ToolRow key={tool.id} name={tool.name} detail={tool.detail} />
        ))
      )}
    </>
  )
}

/** Tool-call headline — compact single line, consecutive rows visually tight. */
function ToolRow({ name, detail }: { name: string; detail?: string }) {
  return (
    <div className="flex min-w-0 items-center gap-2 py-0.5 pl-0.5">
      <CodingToolIcon className="size-3 shrink-0 text-muted-foreground/60" />
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
        <CodingToolIcon className="size-3 shrink-0 text-muted-foreground/60" />
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
  store,
  live,
  onSend,
  sessionId,
  placeholder,
}: {
  store: SteerSessionStore
  /** Sending is possible — the composer itself stays mounted regardless
   *  (EXP-621), so a connection flap never eats the draft. */
  live: boolean
  onSend: (text: string) => boolean
  /** Every steer image uploads to the session's own server-only store
   *  (EXP-702) — issue runs included, so steering screenshots never clutter
   *  the issue's Files section. */
  sessionId: string
  /** Context-aware hint (e.g. the plan-approval "Tell Claude what to
   *  change…"); the default stays the generic prompt. */
  placeholder?: string
}) {
  // EXP-621: the draft lives in the per-session store, so it survives
  // reconnects, dock collapse/reopen and navigation. Blob URLs are the
  // store's to revoke — no unmount cleanup here.
  const { text, images: pending } = useSyncExternalStore(
    store.subscribe,
    store.getDraftSnapshot
  )
  const [sending, setSending] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)
  // A file chooser steals focus without moving it anywhere in the document
  // AND can stall the tab long enough for the relay to evict the viewer —
  // latched on the click that opens one, released when it resolves either
  // way (the comment composer's pattern).
  const filePickerOpenRef = useRef(false)

  useEffect(() => {
    const release = () => {
      filePickerOpenRef.current = false
    }
    window.addEventListener(`focus`, release)
    return () => window.removeEventListener(`focus`, release)
  }, [])

  const addFiles = (files: File[]) => {
    const { rejected, overflow } = store.addDraftImages(files)
    if (rejected > 0) {
      toast.error(`Only images up to 10 MB can be attached`)
    }
    if (overflow > 0) {
      toast.error(`Up to ${MAX_STEER_IMAGES} images per message`)
    }
  }

  const send = async () => {
    if (sending || !live) return
    if (!text.trim() && pending.length === 0) return
    if (pending.length === 0) {
      if (onSend(text)) store.clearDraftAfterSend()
      return
    }
    setSending(true)
    try {
      // Upload sequentially, persisting each id as it lands — a mid-batch
      // failure keeps the composer intact and a retry only uploads the rest.
      const ids: string[] = []
      for (const image of pending) {
        let uploadedId = image.uploadedId
        if (!uploadedId) {
          const uploaded = await uploadSessionImageFile(sessionId, image.file)
          uploadedId = uploaded.id
          store.setDraftImageUploaded(image.url, uploadedId)
        }
        ids.push(uploadedId)
      }
      if (!onSend(buildSteerImageMessage(text, ids))) {
        toast.error(`The session is no longer connected`)
        return
      }
      store.clearDraftAfterSend()
    } catch (error) {
      toast.error(`Couldn't upload image`, {
        description: error instanceof Error ? error.message : undefined,
      })
    } finally {
      setSending(false)
    }
  }

  // EXP-696: ONE rounded card laid out as a COLUMN — the pending strip, a
  // borderless full-width field, then the `[+]`·spacer·send row (the natives'
  // composerCard). Behavior and wire format are unchanged.
  return (
    <div
      className="rounded-2xl border border-border bg-muted/40"
      onDrop={(event) => {
        if (event.dataTransfer.files.length === 0) return
        event.preventDefault()
        addFiles(Array.from(event.dataTransfer.files))
      }}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes(`Files`)) event.preventDefault()
      }}
    >
      {pending.length > 0 && (
        <div className="flex flex-wrap gap-2 px-3 pt-3">
          {pending.map((image) => (
            <div key={image.url} className="relative">
              <img
                src={image.url}
                alt=""
                className="size-16 rounded-md border border-border/60 object-cover"
              />
              <button
                type="button"
                aria-label="Remove image"
                disabled={sending}
                onClick={() => store.removeDraftImage(image.url)}
                className="absolute -right-1.5 -top-1.5 rounded-full border border-border bg-background p-0.5 text-muted-foreground hover:text-foreground"
              >
                <X className="size-3" />
              </button>
            </div>
          ))}
        </div>
      )}
      <Textarea
        value={text}
        onChange={(e) => store.setDraftText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            void send()
          }
        }}
        onPaste={(e) => {
          if (e.clipboardData.files.length === 0) return
          e.preventDefault()
          addFiles(Array.from(e.clipboardData.files))
        }}
        placeholder={placeholder ?? `Message the agent…`}
        rows={1}
        className={cn(
          `max-h-32 min-h-9 w-full resize-none border-none px-3 pb-1 pt-3 shadow-none focus-visible:ring-0`,
          // The composer sits on the session's own surface, so it drops the
          // stock Textarea's glass fill (EXP-616).
          `bg-transparent`
        )}
      />
      <div className="flex items-center gap-1 px-2 pb-2">
        <input
          ref={fileInputRef}
          type="file"
          multiple
          accept={acceptedImageContentTypes.join(`,`)}
          className="hidden"
          onChange={(e) => {
            filePickerOpenRef.current = false
            if (e.target.files) addFiles(Array.from(e.target.files))
            e.target.value = ``
          }}
        />
        <Button
          variant="ghost"
          size="icon"
          className="shrink-0 text-muted-foreground"
          aria-label="Attach image"
          title="Attach image"
          disabled={sending}
          onClick={() => {
            filePickerOpenRef.current = true
            fileInputRef.current?.click()
          }}
        >
          <UiAddIcon />
        </Button>
        <div className="flex-1" />
        <Button
          variant="ghost"
          size="icon"
          className="shrink-0 text-foreground"
          aria-label="Send"
          title="Send"
          disabled={sending || !live || (!text.trim() && pending.length === 0)}
          onClick={() => void send()}
        >
          <UiSendIcon />
        </Button>
      </div>
    </div>
  )
}
