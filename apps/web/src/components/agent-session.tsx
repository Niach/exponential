import type { ReactNode } from "react"
import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react"
import { parseDiff, totals, type DiffFile } from "@exp/domain-contract/diff"
import { editCard } from "@exp/domain-contract/edit-card"
import { linkSegments } from "@/lib/linkify"
import { splitIssueRefs } from "@/lib/issue-refs"
import { ArrowDown, Check, ChevronDown, ChevronRight, X } from "lucide-react"
import type { PastRunRow } from "@/hooks/use-agents-data"
import { MobileFaceSwitcher } from "@/components/mobile-face-switcher"
import { IssueRunSwitcher } from "@/components/issue-run-switcher"
import {
  MOBILE_WORK_BAR_CLEARANCE,
  MOBILE_WORK_CIRCLE_CLASS,
  MobileWorkBar,
  MobileWorkCapsule,
} from "@/components/mobile-work-bar"
import { MergeCapsule } from "@/components/issue-changes-face"
import { PrGithubButton } from "@/components/pr-github-button"
import { ChangesFileSheet } from "@/components/changes-file-sheet"
import { ChangesView } from "@/components/changes-view"
import { TitleStateDot } from "@/components/issue-mobile-header"
import { COMPOSER_PLACEHOLDER } from "@/components/steer-composer"
import {
  conceptIcon,
  DiffCounts,
  EditedFilesCard,
  FAB_CHROME_CLASS,
  GlassCard,
  useIsMobile,
  type SessionDotTone,
  Button,
  Pill,
  Textarea,
  Progress,
  Meter,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  IssueChip as IssueChipView,
} from "@exp/ui"
import { availableFaces, phaseDotTone } from "@/lib/work-faces"
import type { CodingSession } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  mergeTargetProps,
  type SessionMergeTarget,
} from "@/hooks/use-agents-data"
import { useSessionDevice } from "@/hooks/use-session-device"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useNow } from "@/hooks/use-now"
import { useSessionAgentUsage } from "@/hooks/use-session-agent-usage"
import { useSessionUsageRefreshOnOpen } from "@/hooks/use-session-usage-refresh"
import { useKillSession } from "@/hooks/use-kill-session"
import { UsageWindows } from "@/components/agent-usage-bar"
import { AgentMark } from "@/components/agent-picker"
import {
  ACCOUNTS_SECTION_TITLE,
  activeAccountIndex,
  globalSwitchBlocker,
  SessionAccountRows,
  SWITCH_COST_NOTE,
  useSessionAccountSwitch,
  WALL_SWITCH_LABEL,
  type SessionAccountSwitch,
} from "@/components/session-account-switch"
import {
  accountCaption,
  agentHealth,
  blockedBadgeLabel,
  contextPercent,
  formatContextCompact,
  formatContextUsage,
  formatUsageCost,
  healthBadgeLabel,
  severity,
  usageState,
  CONTEXT_SECTION_TITLE,
} from "@/lib/agent-usage"
import type { SessionIdentity } from "@/lib/session-identity"
import {
  staleActivityMinutes,
} from "@/lib/stale-activity"
import {
  activeQuestionIds,
  backgroundStripLines,
  hasPendingCard,
  answerKey,
  askStepperView,
  collectSubagents,
  expToolCaption,
  expToolDisplay,
  freeAnswerFor,
  BACK_TO_CURRENT_STEP,
  FREE_TEXT_PLACEHOLDER,
  PLAN_FEEDBACK_PLACEHOLDER,
  groupFeedRows,
  groupLaneRows,
  FEED_WINDOW,
  FEED_WINDOW_STEP,
  isAnswerLocked,
  LIVE_TOOL_OUTPUT_TAIL_LINES,
  liveToolOutputTail,
  liveToolRowId,
  looksLikeMarkdown,
  nestedWorkflowId,
  optionForHotkey,
  orphanWorkflowIds,
  optionHotkey,
  pendingAnswerable,
  opensInlineField,
  QUEUE_REMOVE_LABEL,
  QUEUE_STRIP_TITLE,
  type QueuedMessage,
  rateLimitBanner,
  rowClass,
  sessionIsWorking,
  subagentLabel,
  subagentIdOf,
  summarizeSubagentRow,
  toolGroupCaption,
  transcriptGapToken,
  visibleSubagentTabs,
  workflowSubagentIds,
  type AnswerState,
  type AnswerStates,
  type ExpToolDisplay,
  type ExpToolPreview,
  type RowClass,
  type SessionRateLimitState,
  type SessionUsageState,
  type BackgroundStripLine,
  type SubagentSummary,
  type TranscriptGapToken,
  type WorkflowState,
} from "@/lib/agent-feed"
import { workingCaption } from "@/lib/working-caption"
import { AgentBrandMark } from "@/components/agent-brand-mark"
import { SteerComposer } from "@/components/steer-composer"
import { ContextRing } from "@/components/context-ring"
import {
  MergePrPill,
  ResumeRunPill,
  StopRunPill,
} from "@/components/run-action-pills"
import {
  ISSUE_FACE_LABEL,
  RESULTS_FACE_LABEL,
  runFaceLabel,
  WorkFaceToggle,
  type WorkFace,
  type WorkFaceItem,
} from "@/components/team/work-face-toggle"
import { SessionResultsView } from "@/components/session-results-view"
import { parseSessionResults } from "@/lib/session-results"
import {
  RUN_TITLE_CLASS,
  WORK_COLUMN_CLASS,
  WorkHeader,
} from "@/components/work-header"
import { useCanResumeOn } from "@/hooks/use-resume-run"
import { DuplicateWarningRow, WorkflowCard } from "@/components/workflow-card"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import {
  mergeAgentCommands,
  parseSteerCommand,
  steerAgentId,
  steerCommandsFor,
  COMPACTED_LABEL,
  COMPACTING_LABEL,
} from "@/lib/steer-commands"
import {
  acquireSteerSession,
  type FeedItem,
  type QuestionItem,
  type QuestionOption,
  type ToolItem,
} from "@/lib/steer-session-store"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { IssueChip } from "@/components/issue-chip"
import { parseSteerMessage } from "@/lib/steer-image-message"
import { cn } from "@/lib/utils"
import { Composer, ComposerSubmit } from "@/components/composer"
import { ExponentialLogo } from "@/components/exponential-logo"
import { ImagePreviewDialog } from "@/components/image-preview-dialog"

// EXP-317: the session glyphs the native clients also draw resolve through
// the shared registry (packages/icons/icons.json).
const CodingAssistantIcon = conceptIcon(`coding-assistant`)
const CodingCompactIcon = conceptIcon(`coding-compact`)
const CodingCommandIcon = conceptIcon(`coding-command`)
const CodingPlanIcon = conceptIcon(`coding-plan`)
const CodingSubagentIcon = conceptIcon(`coding-subagent`)
const CodingToolIcon = conceptIcon(`coding-tool`)
const EditorImageIcon = conceptIcon(`editor-image`)
const UiDeviceOfflineIcon = conceptIcon(`ui-device-offline`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiHelpIcon = conceptIcon(`ui-help`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiPermissionIcon = conceptIcon(`ui-permission`)
const UiRefreshIcon = conceptIcon(`ui-refresh`)
const UiUsageIcon = conceptIcon(`ui-usage`)
const UiRepeatIcon = conceptIcon(`ui-repeat`)
const UiSwapIcon = conceptIcon(`ui-swap`)
const UiQueuedIcon = conceptIcon(`ui-queued`)
const UiCloseIcon = conceptIcon(`ui-close`)
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
// semantic `answer` frame (steer protocol v2, EXP-249) — the ONLY answer
// path since EXP-672; a card from a desktop that publishes no question id
// renders read-only.
// EXP-740: this view is mounted by the two SESSION ROUTES —
// `/t/$teamSlug/sessions/$sessionId` and the team's `/t/$teamSlug/agent` —
// filling the content panel on every breakpoint. It always auto-connects; the
// route owns the membership + config.enabled gating (the relay enforces both
// regardless) and supplies `identity` + `onBack`. The "coding now" rows +
// remote-start affordances live in issue-coding-rows.tsx.

// ── Transcript rhythm (EXP-787) ──────────────────────────────────────────────
// The measure and the gap ladder are shared tokens (packages/design-tokens
// tokens.json `transcript`, mirrored as --transcript-* in styles.css and
// derived ONCE in lib/agent-feed.ts). The renderer only maps a ladder token
// onto its custom property, so no number is restated here.

/** The centred reading column every transcript row sits in: the measure plus
 *  one gutter either side, with the gutter as padding from `sm:` up (a phone
 *  cannot afford 48px of it, so it keeps the old 12). */
const TRANSCRIPT_COLUMN = `${WORK_COLUMN_CLASS} px-3 sm:px-[var(--transcript-gutter)]`

/** Prose scale — agent narration and the sender's own bubbles. Markdown
 *  bodies get the same size/leading from the `.agent-feed .tiptap-content`
 *  rule in styles.css. */
const TRANSCRIPT_BODY_TEXT = `text-[length:var(--transcript-body-size)] leading-[var(--transcript-body-line-height)]`

/** Tool scale — tool rows, group captions, permission and subagent rows, the
 *  compaction hairline and the "Working…" indicator. */
const TRANSCRIPT_TOOL_TEXT = `text-[length:var(--transcript-tool-size)] leading-[var(--transcript-tool-line-height)]`

const TRANSCRIPT_GAP_CLASS = {
  gapTurn: `pt-[var(--transcript-gap-turn)]`,
  gapBlock: `pt-[var(--transcript-gap-block)]`,
  gapTool: `pt-[var(--transcript-gap-tool)]`,
  gapDefault: `pt-[var(--transcript-gap-default)]`,
} as const satisfies Record<TranscriptGapToken, string>

/** The space above a row, as a class — `transcriptGapToken` picks it, the
 *  first rendered row gets none. */
function transcriptGapClass(prev: RowClass | null, cur: RowClass): string {
  const token = transcriptGapToken(prev, cur)
  return token === null ? `` : TRANSCRIPT_GAP_CLASS[token]
}

// ── Wire protocol ────────────────────────────────────────────────────────────
// EXP-621: the relay protocol handling, the connection lifecycle and the feed
// reducer all moved to lib/steer-session-store.ts — a module-level per-session
// store that OUTLIVES this view, so leaving the session page or navigating
// away keeps the socket, the feed and the composer draft. This file only
// renders.


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

// Mounted by the session routes (EXP-740), keyed by session id. Always
// auto-connects; the caller owns the membership + config.enabled gating (the
// relay enforces both regardless) and supplies `identity` + `onBack`.
// Session-scoped — the "coding now" rows live in issue-coding-rows.tsx.
export function AgentSessionView({
  session,
  currentUserId,
  identity,
  mergeTarget,
  banner,
  face,
  onFace,
  onIssueFace,
  issueHeader,
  issueRuns,
  onOpenRun,
  onStart,
  prFiles,
  prUrl,
  graphBadge,
  renderMobileHeader,
  onBack,
}: {
  session: CodingSession
  currentUserId: string
  /** EXP-688: what this run IS — the mono identifier (absent for action,
   *  chat and batch runs) and its human subject. The ONE header names it on
   *  every breakpoint (EXP-740: the dock tab no longer sits under a panel). */
  identity: SessionIdentity
  /** EXP-678: what this session's Merge pill acts on — the linked issue, a
   *  batch run's resolved representative (EXP-535), or the run's own chore PR
   *  row (EXP-734). Absent (no open PR, still syncing) = no Merge pill. */
  mergeTarget?: SessionMergeTarget
  /** EXP-773: a strip between the header and the feed — the session route's
   *  ended-run close-out (byline, Resume). */
  banner?: React.ReactNode
  /** EXP-877: which face of the work tab this page shows — `run` (the
   *  transcript) or `diff` (the run's changes, full column). `issue` is a
   *  navigation the route performs (`onIssueFace`). */
  face: WorkFace
  onFace: (face: WorkFace) => void
  /** EXP-870: the run's linked issue is the same work tab's other face — its
   *  canonical URL. Absent on issue-less runs (no `Issue` face). */
  onIssueFace?: () => void
  /** EXP-877: an issue-bound run wears the ISSUE's header — the editable
   *  title, the pin + `…` cluster and the properties tray (with the ONE
   *  coding action inside it). Absent = a run header: the run title, Merge
   *  and Stop/Resume on the right. */
  issueHeader?: {
    title: ReactNode
    trailing?: ReactNode
    tray?: ReactNode
  }
  /** EXP-886: the issue's runs of mine (`useIssueRuns`), switcher order —
   *  the Run/Runs label, the md+ `IssueRunSwitcher` and the phone switcher's
   *  run rows all read it. */
  issueRuns?: readonly PastRunRow[]
  /** EXP-886: open another of the issue's runs (the view swaps in place). */
  onOpenRun?: (session: CodingSession) => void
  /** EXP-893: start a NEW run on the issue — the phone switcher's `Start
   *  coding` row once this run ended for good. */
  onStart?: () => void
  /** EXP-893: the issue's PR files, the phone's Changes face when the run
   *  published no live diff (`useReviewFiles`). */
  prFiles?: DiffFile[] | null
  /** EXP-893: the PR page, the Changes face's GitHub circle. */
  prUrl?: string | null
  /** EXP-897: the stack/batch pill (`PrGraphBadge`) — the route builds it so
   *  this file stays free of routing. It rides the ONE work header, and its
   *  overlay's sections follow the face showing. */
  graphBadge?: ReactNode
  /** EXP-893: an issue subject's phone header (`IssueMobileHeader`) — the
   *  route wraps it so the same bar shows on every face; `showingRun` says
   *  whether to put Stop / Resume in its trailing slot. */
  renderMobileHeader?: (input: {
    dot: { tone: SessionDotTone; connecting: boolean }
    showingRun: boolean
  }) => ReactNode
  /** Leave the session page (the socket outlives the unmount, EXP-621). */
  onBack: () => void
}) {
  /** EXP-886: more than one run of mine on the issue — "Runs". */
  const multipleRuns = (issueRuns?.length ?? 0) > 1
  // EXP-621: the connection lives in a module-level per-session store that
  // outlives this view — mounting subscribes to the retained state (feed,
  // phase, answers) and dials only when nothing is connected yet, so
  // reopening a session renders instantly with no reconnect phase.
  const store = useMemo(() => acquireSteerSession(session.id), [session.id])
  // EXP-698: the steer composer is the mention field, so it needs the run's
  // team roster for `@` autocomplete.
  const { users: teamUsers } = useTeamUsers(session.teamId)
  const {
    phase,
    feed,
    latestDiff,
    compacting,
    config,
    usage: sessionUsage,
    rateLimit,
    turnState,
    turnStartedAt,
    turnTokens,
    backgroundTasks,
    queue,
    workflows,
    runningWorkflow,
    answerStates,
    connected,
    canLoadEarlier: snapshotCanLoadEarlier,
  } = useSyncExternalStore(store.subscribe, store.getSnapshot)
  /** EXP-724: the agent is folding its context — the strip above the composer
   *  says so, and the generic "Working…" footer stands down while it does. */
  const compactingNow = compacting !== null
  useEffect(() => store.connect(), [store])
  // The synced row is the truth for "still running" inside the redial loops.
  useEffect(
    () => store.noteSessionStatus(session.status),
    [store, session.status]
  )

  /** EXP-893: the phone's composer is a capsule until tapped. */
  const [composerOpen, setComposerOpen] = useState(false)
  /** EXP-877: the file the diff FACE is scrolled to (null = the top). */
  const [diffFile, setDiffFile] = useState<string | null>(null)
  const [usageOpen, setUsageOpen] = useState(false)
  /** EXP-866: an account switch has been requested for THIS run — the live
   *  rate-limit notice stands down for good (the slot is cleared too, but a
   *  late frame from the ending run must not bring the wall back over a run
   *  that is being left). Reset when the view moves to another session. */
  const [switchRequested, setSwitchRequested] = useState(false)
  useEffect(() => setSwitchRequested(false), [session.id])
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
    // other inputs — phase, the feed-derived question set — are covered);
    // the compaction strip (EXP-724) takes height the same way.
  }, [feed, atBottom, phase.kind, session.needsInput, compactingNow])

  // Switching conversation tabs re-pins to the newest event (EXP-356).
  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
    setAtBottom(true)
  }, [agentTab])

  /** EXP-895: the run's live `git diff` through the ONE parser — the same
   *  `DiffFile[]` the review page and the tool cards render. */
  const diffFiles = useMemo(
    () => (latestDiff ? parseDiff(latestDiff).files : []),
    [latestDiff]
  )
  const diffStats = useMemo(() => totals(diffFiles), [diffFiles])

  const live = phase.kind === `live`
  const sessionEnded = session.status === `ended`
  // EXP-312: live implies ownership — the mint refuses everyone else.
  // EXP-621: the composer stays MOUNTED through connection flaps (only send
  // is disabled) — unmounting it on a phase change was how a slow-consumer
  // eviction ate a typed draft. It only leaves with the session itself.
  // EXP-820: while a plan or question waits on THIS viewer, the card is the
  // input — its free-text row opens an inline field — so the composer steps
  // aside instead of doubling as the answer path (EXP-788 is retired). The
  // draft survives in the store for when it comes back. (`composerVisible`
  // is derived below, once the feed's active set is.)
  const sessionOpen = !sessionEnded && phase.kind !== `ended`
  // EXP-678: an open PR on a still-live run is mergeable right here. The
  // server ends the session on merge (EXP-498) and the prState echo hides
  // the pill again — no local state to unwind. A pending card never hides
  // the Merge (only the composer steps aside for it).
  const mergeProps = mergeTarget ? mergeTargetProps(mergeTarget) : null
  const canMerge = sessionOpen && mergeProps?.prState === `open`
  // EXP-706: a conflicted merge swaps the pill for the "Fix conflicts" run,
  // which needs the relay. The session routes only mount this view for a
  // member with steering on, but the config is the honest gate.
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(steerConfig?.enabled)

  /** Identity-scoped questions stay answerable until they resolve. A card
   *  from a desktop too old to publish ids is never in here — it renders
   *  read-only (EXP-672). */
  const questionIds = useMemo(() => activeQuestionIds(feed), [feed])
  const canAnswer = live && !sessionEnded
  // EXP-783: the transcript keeps the WHOLE run, and this view paints a
  // WINDOW over it. `windowFrom` is the id of the oldest rendered row —
  // `null` means the newest FEED_WINDOW rows, WINDOW_FROM_START the feed's
  // own first row whatever it is. An id rather than an index, so a
  // byte-budget eviction or a replay swap cannot slide the window somewhere
  // else under the reader.
  const [windowFrom, setWindowFrom] = useState<number | null>(null)
  const windowStart = useMemo(() => {
    const tail = Math.max(0, feed.length - FEED_WINDOW)
    if (windowFrom === null) return tail
    const at = feed.findIndex((item) => item.id >= windowFrom)
    return Math.min(at === -1 ? tail : at, tail)
  }, [feed, windowFrom])
  /** Render rows: consecutive tool calls collapse into "N tool calls" runs
   *  (EXP-97), one ask's questions into a stepper and a subagent's work into
   *  its own group — a projection only, the flat feed stays the state. */
  const rows = useMemo(
    () => groupFeedRows(feed, windowStart),
    [feed, windowStart]
  )
  /** EXP-895: the ONE row the transcript keeps EXPANDED — the last item while
   *  it is an unsettled tool call, and only in a live run (an ended run's
   *  trailing unsettled row is history, not a tail). Every other row is
   *  compact; the shared rule is `liveToolRowId` ×4. */
  const liveRowId = useMemo(
    () => (live ? liveToolRowId(feed) : undefined),
    [live, feed]
  )
  /** There is more of this run above the window: either rows the feed already
   *  holds, or (EXP-783) a page only the device has. */
  const canLoadEarlier = windowStart > 0 || snapshotCanLoadEarlier
  /** Pull the next page in. Anchored by capturing the distance from the
   *  BOTTOM of the scroll content before the rows change and restoring it
   *  after: a front insertion otherwise moves the reader by exactly the
   *  height of what was inserted. Past the feed's own first row the window
   *  is pinned to the FRONT before the device is asked, so the page lands
   *  inside it when it arrives (EXP-795). */
  const anchorRef = useRef<number | null>(null)
  const loadEarlier = useCallback(() => {
    const el = scrollRef.current
    anchorRef.current = el ? el.scrollHeight - el.scrollTop : null
    if (windowStart > 0) {
      setWindowFrom(feed[Math.max(0, windowStart - FEED_WINDOW_STEP)].id)
      return
    }
    setWindowFrom(WINDOW_FROM_START)
    store.loadEarlier()
  }, [feed, windowStart, store])
  useLayoutEffect(() => {
    const anchor = anchorRef.current
    if (anchor === null) return
    anchorRef.current = null
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight - anchor
  }, [rows])
  // While the reader is at the newest rows the window may SLIDE with the
  // stream (invisible, and it keeps the projection bounded). The moment they
  // scroll up it is PINNED: rows vanishing above a reader is exactly the jump
  // this change exists to avoid.
  useEffect(() => {
    if (atBottom) setWindowFrom(null)
  }, [atBottom])
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
  /** The focused agent's stream, in feed order: its lifecycle markers, tool
   *  calls and — since EXP-773 — the prose and user turns the mapper scoped
   *  to it (they are hidden from Main for exactly that reason). */
  const agentItems = useMemo(
    () =>
      activeAgent === null
        ? []
        : feed.filter((item) => subagentIdOf(item) === activeAgent),
    [feed, activeAgent]
  )
  /** A trailing question/plan means the session is blocked on a human — the
   *  header flips to "Needs your input" so it never looks silently stuck. */
  const awaitingInput = live && questionIds.size > 0
  /** The composer steps aside only for a card this viewer can answer on the
   *  tab it is looking at, and never past a timed-out answer
   *  (`hasPendingCard`) — an id-less, off-tab or timed-out card must not
   *  hide the only other input for good. */
  const cardPending =
    canAnswer && hasPendingCard(feed, questionIds, answerStates, activeAgent)
  const composerVisible = sessionOpen && !cardPending
  /** EXP-788/820: the card the keyboard answers — the newest active plan or
   *  question whose answer is not in flight. Its number chips are live on
   *  that card alone. */
  const pendingCard = useMemo(
    () =>
      live && canAnswer
        ? pendingAnswerable(feed, questionIds, answerStates)
        : null,
    [live, canAnswer, feed, questionIds, answerStates]
  )
  /** EXP-724: the slash commands THIS session's agent can run (an agent-less
   *  row is a claude run). Empty catalog = no menu, no hint, no "…" entry.
   *  EXP-746: an ACP run's agent advertises commands of its own, so the
   *  catalog is the same MERGE the composer's `/` menu offers — otherwise a
   *  steered `/<agent command>` came back as a prose bubble here while the
   *  menu that sent it listed the row (Android and iOS merge on both sides
   *  too). The "…" Compact entry follows: a run whose agent advertises
   *  `/compact` can run it. An agent-less run that published a `config_state`
   *  is an EXTERNAL agent, not a claude one (`steerAgentId`). */
  const catalogAgent = steerAgentId(session.agent, config !== null)
  const agentCommands = useMemo(
    () =>
      mergeAgentCommands(
        steerCommandsFor(catalogAgent),
        config?.commands ?? [],
        catalogAgent
      ),
    [catalogAgent, config?.commands]
  )
  /** EXP-389/EXP-848: the agent is actively EXECUTING a turn — the shared
   *  predicate (`sessionIsWorking`), keyed on the turn slot rather than on
   *  "live", so an idle run between turns no longer pulses. Mobile parity. */
  const working = sessionIsWorking({
    live,
    sessionEnded,
    turnState,
    awaitingInput,
    needsInput: session.needsInput,
    blocked: session.blocked != null,
    compacting: compactingNow,
  })
  /** EXP-549/550: the host machine per the synced devices row — its RENAMED
   *  label, and whether it is offline right now. */
  const device = useSessionDevice(session)
  /** EXP-484: the host machine's fresh rate-limit report for THIS run's
   *  agent, or null (finished run, other agent, stale or absent numbers). */
  const agentUsage = useSessionAgentUsage(session)
  /** EXP-849 (phase 3): the accounts this run's machine holds, and the
   *  between-turns switch onto one of them (claude only, own machine, idle).
   *  The Usage sheet and the rate-limit notice both open these rows. */
  const accountSwitch = useSessionAccountSwitch(session, currentUserId, {
    turnEnded: turnState === `ended`,
    // EXP-866: the wall this switch is the way out of goes NOW, not when the
    // continuation's page replaces this one.
    onBeforeSwitch: () => {
      store.clearRateLimit()
      setSwitchRequested(true)
    },
  })
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
  /** FEED-26: when the feed last changed while live — the viewer's own clock
   *  for "this run has gone quiet". A compaction edge counts as activity too.
   *  State (not a ref) so the render that appends the item already sees the
   *  reset instead of one stale 30 s tick. */
  const [lastActivityAt, setLastActivityAt] = useState(() => Date.now())
  useEffect(() => {
    setLastActivityAt(Date.now())
  }, [feed, compacting, live])
  /** Minutes of silence past the threshold, or null while the agent is
   *  working, waiting on a human, compacting or paused — each of those has
   *  its own caption. */
  const staleMinutes =
    live && !paused && !awaitingInput && !compactingNow
      ? staleActivityMinutes(usageNow.getTime(), lastActivityAt)
      : null
  // EXP-804: the agent's usage wall as the ROW records it — null unless the
  // run's device reported one. `usageNow` already ticks for the stale check,
  // so the countdown rides it rather than opening a second timer.
  const blockedLabel = blockedBadgeLabel(session.blocked, usageNow)
  /** EXP-688: the kill confirmation is shared with the dock tab's X. Live
   *  implies ownership (EXP-312), and only a live stream can be killed. */
  const {
    canKill: ownsLiveRow,
    requestKill,
    dialog: killDialog,
  } = useKillSession(session, currentUserId, device.label, paused)
  const canKill = live && ownsLiveRow
  /** EXP-877: Resume in the run header (issue-less runs) — an issue-bound
   *  run's tray decides for itself (`issue-coding-action.tsx`). */
  const canResumeAny = useCanResumeOn(sessionEnded ? session : null)
  const canResumeRun = canResumeAny && !issueHeader
  /** EXP-849: the Usage sheet is a CONTROL now — it opens the account rows
   *  (with their bars) and switches between them — so it exists whenever the
   *  machine reported an account for this run, not only when numbers are
   *  fresh. */
  const hasAccountRows = accountSwitch.options.length > 0
  // EXP-850 §10: the `…` overflow is GONE from this header. Usage moved into
  // the Context pill and "Compact context" is no longer a menu entry —
  // `/compact` stays a slash command in the composer, which is where every
  // other command is typed.
  const pausedTitle = `${device.label ?? `The device`} is offline`
  const pausedBody = `The agent is paused on that machine and continues when it comes back online.`
  // The `closed` phase (relay `bye publisher_lost`) does not redial on its
  // own — a viewer that watched the lid close would sit on "Disconnected"
  // after the machine woke. Nudge the store once the device flips back
  // online so the stream resumes without a click. EXP-625: the nudge is
  // `kick`, which decides for itself whether this store is actually stuck
  // (it also shortcuts a `starting` backoff step); the phase test that used
  // to live here moved inside it.
  // EXP-773: the history phases name the machine the transcript lives on.
  useEffect(
    () => store.noteDeviceLabel(device.label),
    [store, device.label]
  )
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

  /** EXP-850 §10 / EXP-877: the usage overlay opens off the context ring —
   *  in the composer footer on md+, the phone bar's left circle on a phone.
   *  Hidden once the run is over (a finished run's context is not a live
   *  number), unless the machine reported accounts or rate limits worth
   *  opening. */
  const contextLabel = formatContextCompact(sessionUsage)
  const usageAvailable =
    !sessionEnded && Boolean(contextLabel || agentUsage || hasAccountRows)

  /** EXP-850 §3/§4: the subagents that belong to a workflow card — their rows
   *  nest inside it instead of standing in the transcript. */
  const workflowAgents = useMemo(() => workflowSubagentIds(feed), [feed])

  /** §1/§2: what the CLI is running in the background, plus every OPEN wait
   *  row — the compact strip directly above the composer. */
  const stripLines = useMemo(
    () => backgroundStripLines({ backgroundTasks, feed }),
    [backgroundTasks, feed]
  )

  /** §4: the duplicate warnings a workflow card carries, by workflow id. A
   *  duplicate edge WITHOUT one renders inline in its subagent group row. */
  const workflowDuplicates = useMemo(() => {
    const byWorkflow = new Map<string, string[]>()
    for (const item of feed) {
      if (item.kind !== `subagent` || item.status !== `duplicate`) continue
      if (!item.workflowId || !item.detail) continue
      const held = byWorkflow.get(item.workflowId) ?? []
      if (!held.includes(item.detail)) held.push(item.detail)
      byWorkflow.set(item.workflowId, held)
    }
    return byWorkflow
  }, [feed])

  /** §3: a workflow agent's own rows, by agent id — the card folds them away
   *  behind its agent row, and they never stand in the transcript. */
  const workflowAgentEvents = useMemo(() => {
    const byWorkflow = new Map<string, Map<string, ReactNode>>()
    for (const [subagentId, workflowId] of workflowAgents) {
      const items = feed.filter(
        (item) =>
          subagentIdOf(item) === subagentId &&
          (item.kind === `tool` || item.kind === `narration`)
      )
      if (items.length === 0) continue
      const agents = byWorkflow.get(workflowId) ?? new Map<string, ReactNode>()
      agents.set(subagentId, <NestedAgentEvents items={items} />)
      byWorkflow.set(workflowId, agents)
    }
    return byWorkflow
  }, [feed, workflowAgents])

  /** EXP-850 §3: cards whose `Workflow` tool row this window does not hold —
   *  evicted, or above the rendered rows. They land at the TAIL of the
   *  transcript, because a card is the ONLY place a running workflow's agents
   *  and its duplicate warnings are shown (Android parity). */
  const orphanWorkflows = useMemo(
    () =>
      orphanWorkflowIds(rows, workflows)
        .map((id) => workflows.get(id))
        .filter((workflow): workflow is WorkflowState => workflow !== undefined),
    [rows, workflows]
  )

  /** EXP-879: the screenshots this run published (`coding_sessions.results`,
   *  a synced jsonb blob). Empty = no Results face. */
  const results = useMemo(
    () => parseSessionResults(session.results),
    [session.results]
  )

  /** EXP-877: the faces this work tab offers — `Issue` when the run links
   *  one, `Run` always (this IS the run), the diff once the run has changes,
   *  and `Results` once it published screenshots (EXP-879). Under two faces
   *  the toggle renders nothing. Selecting the diff from here drops a turn
   *  scope the reader left behind (EXP-862). */
  const faceItems: WorkFaceItem[] = [
    ...(onIssueFace
      ? [{ face: `issue` as const, label: ISSUE_FACE_LABEL, onSelect: onIssueFace }]
      : []),
    {
      face: `run` as const,
      label: runFaceLabel(multipleRuns),
      onSelect: () => onFace(`run`),
    },
    ...(diffFiles.length > 0
      ? [
          {
            face: `diff` as const,
            label: (
              <DiffCounts
                additions={diffStats.additions}
                deletions={diffStats.deletions}
              />
            ),
            onSelect: () => onFace(`diff`),
          },
        ]
      : []),
    ...(results.length > 0
      ? [
          {
            face: `results` as const,
            label: RESULTS_FACE_LABEL,
            onSelect: () => onFace(`results`),
          },
        ]
      : []),
  ]
  /** EXP-893: what the Changes face draws — the run's live diff, else the
   *  issue's PR files the route fetched for a phone. */
  const changesFiles = diffFiles.length > 0 ? diffFiles : (prFiles ?? [])
  /** EXP-877: the diff face stands only while there is something to draw —
   *  with no files it falls back to the run face. */
  const showDiffFace = face === `diff` && changesFiles.length > 0
  /** EXP-879: the results face stands only while the run published something
   *  — a stale `?view=results` falls back to the run face, like the diff. */
  const showResultsFace = face === `results` && results.length > 0
  /** EXP-893: the subject HAS changes — a live diff, PR files, or an open PR
   *  whose files are one fetch away. The phone's Changes face exists then. */
  const hasChanges =
    diffFiles.length > 0 ||
    (prFiles?.length ?? 0) > 0 ||
    mergeProps?.prState === `open`

  /** The run header's own right cluster (issue-less runs): Merge, then
   *  Stop while live or Resume once ended and resumable on its machine. An
   *  issue-bound run's cluster is the issue's (pin + `…`), and its coding
   *  action sits in the tray. */
  const runTrailing = issueHeader ? (
    issueHeader.trailing
  ) : (
    <>
      {/* EXP-916: the Changes face has no bar of its own any more, so the
          header carries the merge control on every face — exactly ONE. */}
      {canMerge && mergeProps && (
        <MergePrPill {...mergeProps} steerEnabled={steerEnabled} />
      )}
      {prUrl && <PrGithubButton prUrl={prUrl} />}
      {canKill ? (
        <StopRunPill onStop={requestKill} />
      ) : sessionEnded && canResumeRun ? (
        <ResumeRunPill session={session} />
      ) : null}
    </>
  )

  /** EXP-877: the context meter, INSIDE the composer's tool row now (the
   *  `usageSlot`), anchoring the same usage popover the header pill used to.
   *  Gone once the run is over — a finished run's context is not a live
   *  number any more. */
  const showEmptyRing = Boolean(agentUsage) || hasAccountRows
  /** EXP-909 §1: the login this run SPENDS — the same resolution the overlay's
   *  header draws (`activeAccountIndex`), hoisted so the open-refresh can name
   *  it in its command. */
  const runAccount =
    accountSwitch.options[
      activeAccountIndex(accountSwitch.options, agentUsage?.account?.email)
    ] ?? null
  // EXP-881: opening the overlay asks that login's machine for fresh numbers,
  // once per open, inside the device's own floor.
  useSessionUsageRefreshOnOpen(
    usageOpen,
    session,
    runAccount?.row ?? null,
    currentUserId
  )
  /* EXP-909: ONE overlay, not two — `MobilePopover` IS the popover on md+ and
     the bottom sheet on a phone, so the sections cannot drift between the two
     shells the way the hand-rolled `Popover` + `Sheet` pair did. The composer's
     ring anchors it here; the phone's work bar anchors the same overlay around
     its own circle (the two rings never render at once). */
  const usageOverlay = (trigger: React.ReactElement) => (
    <MobilePopover open={usageOpen} onOpenChange={setUsageOpen}>
      <MobilePopoverTrigger asChild>{trigger}</MobilePopoverTrigger>
      <MobilePopoverContent
        align="end"
        collisionPadding={8}
        className="w-80 p-0"
        aria-label="Usage"
        mobileTitle="Usage"
        data-testid="session-usage-popover"
      >
        <SessionUsageSections
          sessionUsage={sessionUsage}
          agentUsage={agentUsage}
          agent={session.agent}
          accountSwitch={accountSwitch}
          now={usageNow}
        />
      </MobilePopoverContent>
    </MobilePopover>
  )
  const usageSlot = !usageAvailable
    ? null
    : usageOverlay(<ContextRing usage={sessionUsage} showEmpty={showEmptyRing} />)

  /** EXP-893: the phone's state dot — the header title's, and the switcher
   *  badge's off the Run face. */
  const dot = phaseDotTone({
    live,
    connecting:
      phase.kind === `connecting` ||
      phase.kind === `starting` ||
      phase.kind === `history_pending`,
    awaitingInput,
    paused,
    stale: staleMinutes !== null,
  })
  const showingRun = !showDiffFace && !showResultsFace

  /** EXP-893: the phone's face switcher — the bottom-right circle. Faces:
   *  Issue when the run links one, Run (this IS the run), Changes once there
   *  is a diff or an open PR. `Start coding` joins the menu once this run
   *  ended and no machine can resume it. */
  const mobileSwitcher = isMobile ? (
    <MobileFaceSwitcher
      faces={availableFaces({
        hasIssue: Boolean(onIssueFace),
        hasRun: true,
        hasChanges,
        hasResults: results.length > 0,
      })}
      face={showDiffFace ? `changes` : showResultsFace ? `results` : `run`}
      runs={issueRuns}
      viewedRunId={session.id}
      diffStats={diffFiles.length > 0 ? diffStats : null}
      hasChanges={hasChanges}
      sessionTone={dot.tone}
      offerStart={Boolean(onStart) && sessionEnded && !canResumeAny}
      onFace={(next) => {
        if (next === `issue`) {
          onIssueFace?.()
          return
        }
        onFace(
          next === `changes` ? `diff` : next === `results` ? `results` : `run`
        )
      }}
      onOpenRun={onOpenRun}
      onStart={onStart}
    />
  ) : null

  /** EXP-893: the phone bar by face. Run + open session: the usage ring, the
   *  composer capsule (expanding into the composer), the switcher. Run over:
   *  the switcher alone. Changes: GitHub, Merge PR while mergeable, the
   *  switcher. EXP-879 Results: the switcher ALONE — only the Run face owns
   *  Stop / Resume, only Changes the merge bar. */
  const mobileBar = !isMobile ? null : showResultsFace ? (
    <MobileWorkBar trailing={mobileSwitcher} />
  ) : showDiffFace ? (
    <MobileWorkBar
      /* EXP-895: the file LIST is the leading slot on a phone; GitHub rides the
         issue header's action slot (an issue-less run keeps the circle). */
      leading={
        changesFiles.length > 0 ? (
          <ChangesFileSheet
            files={changesFiles}
            selected={diffFile}
            onSelect={setDiffFile}
          />
        ) : prUrl ? (
          <PrGithubButton prUrl={prUrl} variant="circle" />
        ) : undefined
      }
      capsule={
        canMerge && mergeProps ? (
          <MergeCapsule {...mergeProps} steerEnabled={steerEnabled} />
        ) : undefined
      }
      trailing={mobileSwitcher}
    />
  ) : (
    <MobileWorkBar
      leading={
        sessionOpen && usageAvailable
          ? usageOverlay(
              <ContextRing
                usage={sessionUsage}
                showEmpty={showEmptyRing}
                className={cn(
                  MOBILE_WORK_CIRCLE_CLASS,
                  `[&>svg]:size-6 [&>svg]:shrink-0`
                )}
              />
            )
          : undefined
      }
      capsule={
        composerVisible ? (
          <MobileWorkCapsule
            onClick={() => setComposerOpen(true)}
            data-testid="steer-composer-capsule"
          >
            <span className="truncate text-muted-foreground">
              {COMPOSER_PLACEHOLDER}
            </span>
          </MobileWorkCapsule>
        ) : undefined
      }
      expanded={
        composerVisible && composerOpen ? (
          <div className={cn(`rounded-2xl p-1.5`, FAB_CHROME_CLASS)}>
            <SteerComposer
              store={store}
              live={live && connected}
              onSend={sendMessage}
              working={working}
              sessionId={session.id}
              users={teamUsers}
              agent={session.agent}
              config={config}
              usageSlot={usageSlot}
              autoFocus
              onEmptyBlur={() => setComposerOpen(false)}
            />
          </div>
        ) : null
      }
      trailing={mobileSwitcher}
    />
  )

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* EXP-851/850 §10: on a phone the header IS `MobileDetailHeader` —
          byte-identical to the issue, review, support-thread and session-issue
          screens — with the Context pill as its one trailing control; the
          run's own controls follow in a compact second row, because a native
          bar carries exactly one. On md+ the single header row names the run
          over its phase caption and carries every control on the right. */}
      {isMobile ? (
        /* EXP-893: the phone header never jumps between faces — an issue
           subject wears the issue's header (the route renders it, with Stop
           / Resume in its trailing slot on the Run face only); a run subject
           the shared detail header with the state dot + its title, and the
           same Stop / Resume on the right. No caption row, no plan chip:
           the strips under the transcript say what the run is doing. */
        renderMobileHeader ? (
          renderMobileHeader({ dot, showingRun })
        ) : (
          <MobileDetailHeader
            title={
              <>
                <TitleStateDot tone={dot.tone} connecting={dot.connecting} />
                {identity.subject}
              </>
            }
            onBack={onBack}
            menu={
              /* The cluster keeps the back button's width whether or not it
                 holds anything, so the title stays optically centred. */
              <div className="flex min-w-9 shrink-0 items-center justify-end gap-1">
                {/* EXP-897: an issue-less run — a BATCH run above all — says
                    what it is part of here, the same pill, the same sheet. */}
                {graphBadge}
                {showingRun &&
                  (canKill ? (
                    <StopRunPill onStop={requestKill} />
                  ) : sessionEnded && canResumeAny ? (
                    <ResumeRunPill session={session} />
                  ) : null)}
              </div>
            }
          />
        )
      ) : (
        /* EXP-877: the ONE work header — the same node the issue route
           renders, so nothing moves when the face flips. No back control on
           md+ (EXP-870): the compact rail and the list nav's back row are the
           way out. No identity block, no phase caption: the title says what
           the run is, the transcript's footer says what it is doing. */
        <WorkHeader
          title={
            issueHeader ? (
              issueHeader.title
            ) : (
              <h1 className={RUN_TITLE_CLASS}>{identity.subject}</h1>
            )
          }
          trailing={
            <>
              {/* The toggle names the face actually SHOWING: a `?view=diff`
                  deep link before the diff replays falls back to the
                  transcript, and must not leave no segment selected. */}
              {/* EXP-897: the stack / batch pill leads the cluster — it
                  names what this work is PART of, before the controls that
                  act on it. */}
              {graphBadge}
              <WorkFaceToggle
                face={showDiffFace || showResultsFace ? face : `run`}
                items={faceItems}
              />
              {/* EXP-886: the switcher between the issue's runs, right after
                  the toggle whose "Runs" segment announces it. */}
              {issueRuns && onOpenRun && (
                <IssueRunSwitcher
                  runs={issueRuns}
                  viewedRunId={session.id}
                  onOpen={onOpenRun}
                />
              )}
              {runTrailing}
            </>
          }
          tray={issueHeader?.tray}
        />
      )}

      {banner}

      {showResultsFace ? (
        /* EXP-879: the results FACE — the run's published screenshots in the
           same 896 column under the same header, in place of the
           transcript. */
        <div
          className={cn(
            `min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40`,
            isMobile && MOBILE_WORK_BAR_CLEARANCE
          )}
        >
          <div className={cn(WORK_COLUMN_CLASS)}>
            <SessionResultsView results={results} />
          </div>
        </div>
      ) : showDiffFace ? (
        /* EXP-877: the diff FACE — the run's changes in the same 896 column
           under the same header, in place of the transcript. */
        <div
          className={cn(
            `min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40`,
            isMobile && MOBILE_WORK_BAR_CLEARANCE
          )}
        >
          <div className={cn(WORK_COLUMN_CLASS)}>
            {/* EXP-916: the face is the diff and nothing else — its merge,
                GitHub and PR state live in the work header above it. */}
            <ChangesView
              files={changesFiles}
              nav="auto"
              selected={diffFile}
              onSelect={setDiffFile}
            />
          </div>
        </div>
      ) : (
      <div className="flex min-h-0 flex-1 overflow-hidden">
      <div
        className={cn(
          `flex min-w-0 flex-1 flex-col overflow-hidden bg-card/40`,
          // EXP-893: the phone's floating bar owns the bottom edge — the
          // strips and the last row stop above it.
          isMobile && MOBILE_WORK_BAR_CLEARANCE
        )}
      >
          {/* EXP-356: conversation tabs — Main plus one per RUNNING subagent
              (ended tabs are dropped, EXP-387). */}
          {visibleTabs.length > 0 && (
            <div className="flex shrink-0 items-center gap-1 overflow-x-auto px-2 py-1">
              <AgentTab
                label="Main"
                active={activeAgent === null}
                onClick={() => setAgentTab(null)}
              />
              {visibleTabs.map((agent) => (
                <AgentTab
                  key={agent.subagentId}
                  label={subagentLabel(agent)}
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
              // EXP-619: the feed rides its own bottom edge, and
              // `overscroll-contain` keeps a downward wheel tick past the
              // last row from chaining out to the page behind it.
              // `agent-feed`: the hook the inline-code tint keys on
              // (EXP-698, styles.css) — chat-sized markdown alone is not it,
              // comment bodies render that way too.
              className="agent-feed h-full overflow-y-auto overscroll-contain"
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
                (phase.kind === `connecting` ||
                  phase.kind === `starting` ||
                  phase.kind === `history_pending`) ? (
                <CenteredState>
                  <UiLoadingIcon className="size-4 animate-spin text-muted-foreground" />
                  <span className="text-xs text-muted-foreground">
                    {/* EXP-773: a finished run's transcript is a file on the
                        device — the relay is asking it to republish. */}
                    {phase.kind === `history_pending`
                      ? (phase.detail ?? `Fetching the transcript…`)
                      : phase.kind === `starting`
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
                    `flex min-h-full flex-col justify-end py-2`,
                    TRANSCRIPT_COLUMN
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
                    `flex min-h-full flex-col justify-end py-2`,
                    TRANSCRIPT_COLUMN
                  )}
                >
                  {canLoadEarlier ? (
                    <div className="flex justify-center pb-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 text-xs text-muted-foreground"
                        onClick={loadEarlier}
                      >
                        Load earlier
                      </Button>
                    </div>
                  ) : null}
                  {rows.map((row, index) => {
                    // EXP-787: the rhythm is the ladder, not a uniform gap —
                    // each row carries the space ABOVE it, chosen from the row
                    // before it. The key stays the row id; the wrapper only
                    // holds that padding.
                    const gap = transcriptGapClass(
                      index === 0 ? null : rowClass(rows[index - 1]),
                      rowClass(row)
                    )
                    const wrap = (content: ReactNode) => (
                      <div
                        key={row.kind === `single` ? row.item.id : row.id}
                        className={gap}
                      >
                        {content}
                      </div>
                    )
                    // EXP-916: a run of consecutive edits IS the transcript's
                    // edited-files card — the same `FileDiffCard` stack every
                    // Changes surface draws, inline, never a jump away.
                    if (row.kind === `edits`) {
                      return wrap(
                        <EditsCardRow
                          items={
                            row.items as Extract<FeedItem, { kind: `tool` }>[]
                          }
                          liveRowId={liveRowId ?? null}
                        />
                      )
                    }
                    if (row.kind === `toolRun`) {
                      return wrap(
                        <ToolGroupRow
                          items={
                            row.items as Extract<FeedItem, { kind: `tool` }>[]
                          }
                          liveTail={
                            liveRowId !== undefined &&
                            row.items[row.items.length - 1]?.id === liveRowId
                          }
                        />
                      )
                    }
                    if (row.kind === `subagent`) {
                      // EXP-850 §3: a workflow's agents live INSIDE its card,
                      // never as a loose group row in the transcript — but
                      // ONLY when this client HOLDS that card (EXP-856): an
                      // edge tagged with a workflow whose frame never arrived
                      // keeps its ordinary row, warning and all. A file card
                      // anchored here still renders (§12): the turn it closes
                      // happened whether or not its row is drawn.
                      if (
                        nestedWorkflowId(
                          { subagentId: row.subagentId },
                          workflowAgents,
                          workflows
                        )
                      ) {
                        return null
                      }
                      return wrap(<SubagentGroupRow items={row.items} />)
                    }
                    if (row.kind === `ask`) {
                      return wrap(
                        <AskStepperCard
                          items={row.items as QuestionItem[]}
                          activeIds={questionIds}
                          canAnswer={canAnswer}
                          answerStates={answerStates}
                          onAnswer={answerQuestion}
                          pendingId={pendingCard?.id ?? null}
                          onSend={sendMessage}
                        />
                      )
                    }
                    const item = row.item
                    switch (item.kind) {
                      case `narration`:
                        return wrap(<NarrationBubble text={item.text} />)
                      case `tool`: {
                        // EXP-850 §3: the `Workflow` call renders as its CARD
                        // (same id), so the call's own settle folds in and no
                        // second row is ever drawn. A card this client has not
                        // received yet falls back to the plain tool row.
                        const workflow = item.workflowId
                          ? workflows.get(item.workflowId)
                          : undefined
                        if (workflow) {
                          return wrap(
                            <WorkflowCard
                              workflow={workflow}
                              agentEvents={workflowAgentEvents.get(workflow.id)}
                              duplicates={workflowDuplicates.get(workflow.id)}
                              className={TRANSCRIPT_TOOL_TEXT}
                            />
                          )
                        }
                        // EXP-916: such a call may still CARRY a patch (an
                        // edit tagged with a workflow). The card is the only
                        // place a patch renders now, so a one-member card
                        // draws it rather than a row that drops it.
                        if (item.diff) {
                          return wrap(
                            <EditsCardRow
                              items={[item]}
                              liveRowId={liveRowId ?? null}
                            />
                          )
                        }
                        return wrap(
                          <ToolRow
                            item={item}
                            flush
                            live={item.id === liveRowId}
                          />
                        )
                      }
                      case `user_message`: {
                        // EXP-724: a steered slash command renders as a
                        // compact pill, not as a chat bubble of prose.
                        const command = parseSteerCommand(
                          item.text,
                          agentCommands
                        )
                        return wrap(
                          command ? (
                            <CommandRow
                              name={command.command.name}
                              args={command.args}
                            />
                          ) : (
                            <UserMessageBubble text={item.text} />
                          )
                        )
                      }
                      case `compaction`:
                        return wrap(<CompactionRow />)
                      case `permission`:
                        return wrap(
                          <PermissionRow
                            tool={item.tool}
                            detail={item.detail}
                            active={
                              live && item.id === feed[feed.length - 1]?.id
                            }
                          />
                        )
                      case `subagent`:
                        // Same rule as the group row above: nested only when
                        // the card that would hold it exists.
                        if (
                          nestedWorkflowId(item, workflowAgents, workflows)
                        ) {
                          return null
                        }
                        return wrap(<SubagentGroupRow items={[item]} />)
                      case `question`:
                        return wrap(
                          <QuestionCard
                            item={item}
                            active={questionIds.has(item.id)}
                            canAnswer={canAnswer}
                            answerState={answerStates[answerKey(item)]}
                            onAnswer={answerQuestion}
                            hotkeys={pendingCard?.id === item.id}
                            onSend={sendMessage}
                          />
                        )
                    }
                  })}
                  {/* EXP-850 §3: a card whose `Workflow` tool row is not in
                      the rendered window still has to be seen — it lands at
                      the tail as a row of its own rather than taking its
                      agents and its warnings down with it (×4). */}
                  {orphanWorkflows.map((workflow, index) => (
                    <div
                      key={`workflow-${workflow.id}`}
                      className={transcriptGapClass(
                        index > 0
                          ? `tool`
                          : rows.length === 0
                            ? null
                            : rowClass(rows[rows.length - 1]),
                        `tool`
                      )}
                    >
                      <WorkflowCard
                        workflow={workflow}
                        agentEvents={workflowAgentEvents.get(workflow.id)}
                        duplicates={workflowDuplicates.get(workflow.id)}
                        className={TRANSCRIPT_TOOL_TEXT}
                      />
                    </div>
                  ))}
                  {/* EXP-389: the agent-is-busy footer under the newest
                      event (mobile parity) — main conversation only. */}
                  {working && (
                    <div
                      className={transcriptGapClass(
                        orphanWorkflows.length > 0
                          ? `tool`
                          : rows.length === 0
                            ? null
                            : rowClass(rows[rows.length - 1]),
                        `tool`
                      )}
                    >
                      <WorkingIndicatorRow
                        agent={session.agent}
                        startedAt={turnStartedAt}
                        tokens={turnTokens}
                        workflow={runningWorkflow}
                      />
                    </div>
                  )}
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

          {/* Status banners (feed retained above). EXP-877: no "ended" strip
              — the hidden composer and the header's Resume say it. */}
          {paused && feed.length > 0 && (
            <div className="flex items-center gap-1.5 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              <UiDeviceOfflineIcon className="size-3 shrink-0" />
              <span>
                {`Paused — ${pausedTitle}. ${pausedBody}`}
              </span>
            </div>
          )}
          {phase.kind === `closed` && !paused && (
            <div className="flex items-center gap-2 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              <span className="min-w-0 flex-1">
                {phase.detail ?? `Connection lost.`}
              </span>
              {/* EXP-877: a dropped stream redials from its own strip. */}
              <Button
                variant="outline"
                size="sm"
                className="h-6 shrink-0"
                onClick={() => store.reconnect()}
              >
                <UiRefreshIcon />
                Reconnect
              </Button>
            </div>
          )}
          {/* EXP-804: the PERSISTED usage wall off the session row — a walled
              run is still running. Deliberately not the same thing as
              `RateLimitBanner` below, which is the LIVE stream's own report:
              this one is already there when you open a run whose stream has
              not connected yet, which is exactly the moment a silently walled
              run looks healthy. */}
          {blockedLabel && (
            <div
              className="border-t border-border/60 px-3 py-1.5 text-[11px] font-medium text-amber-400"
              data-testid="session-blocked-strip"
            >
              {blockedLabel}
            </div>
          )}
          {/* EXP-724: the compaction strip. Indeterminate on purpose — the
              fold takes 10-170s with nothing measurable to report; the
              persistent marker row lands in the feed when it finishes. */}
          {compactingNow && (
            <div className="border-t border-border/60 px-3 py-2">
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <CodingCompactIcon className="size-3 shrink-0" />
                <span>{COMPACTING_LABEL}</span>
                {compacting?.trigger === `manual` && (
                  <span className="text-muted-foreground/60">requested</span>
                )}
              </div>
              <Progress value={null} className="mt-1 h-1" />
            </div>
          )}
          {/* EXP-784: the agent's rate-limit window, while it reports one —
              the slot clears on an empty/`ok` status and the banner goes. */}
          {rateLimit && !switchRequested && (
            <RateLimitBanner
              state={rateLimit}
              // EXP-849: a spent window is a WALL, and the one thing that
              // clears it right now is another account — so that is the
              // notice's PRIMARY button (it opens the account rows; the
              // continuation's own slot is empty, so the notice goes with the
              // old run). Hidden only when this run could never switch — or
              // has no pill to anchor the overlay to (EXP-863).
              onSwitchAccount={
                hasAccountRows && usageSlot !== null
                  ? () => setUsageOpen(true)
                  : undefined
              }
            />
          )}
          {phase.kind === `starting` && !paused && feed.length > 0 && (
            <div className="flex items-center gap-1.5 border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
              <UiLoadingIcon className="size-3 animate-spin" />
              The agent is starting. Waiting for the live stream…
            </div>
          )}

          {/* EXP-850 §11: the desktop's bottom "Changes" bar is GONE — the
              diff is the pane beside the transcript. The phone's floating
              version renders inside the scroll wrapper above. */}

          {/* EXP-850 §1/§2: monitors and background shell commands, right
              above the composer. */}
          <BackgroundStrip lines={stripLines} />

          {/* EXP-861: the messages the agent has not read yet — held behind a
              compaction (an X revokes one and hands the text back to an
              empty draft, the CLI's "edit queued message") or sent mid-turn
              and awaiting the agent's replay (EXP-873: no X; Stop takes it
              back). */}
          <QueueStrip
            messages={queue}
            onRemove={(entry) => {
              store.unqueue(entry.id)
              if (store.getDraftSnapshot().text.trim() === ``) {
                store.setDraftText(entry.text)
              }
            }}
          />

          {/* Steering composer. Steering is fully seamless (EXP-312) — no
              captions, no operator state; live implies ownership. */}
          {composerVisible && !isMobile && (
            <div className="border-t border-border p-2">
              <div className={WORK_COLUMN_CLASS}>
                <SteerComposer
                  store={store}
                  // `connected` matters beyond the phase: a silent
                  // slow-consumer redial keeps `live` while the socket is
                  // briefly down, and the send button should dim honestly
                  // for that gap.
                  live={live && connected}
                  onSend={sendMessage}
                  // EXP-790: the send glyph is Stop while the agent works
                  // and nothing is typed.
                  working={working}
                  sessionId={session.id}
                  users={teamUsers}
                  agent={session.agent}
                  // EXP-746: the live config rides down as a PROP. The
                  // composer deliberately subscribes to the draft snapshot
                  // only (a keystroke must not re-render the feed), and this
                  // view already holds the full one.
                  config={config}
                  // EXP-877: the context meter lives in the composer's tool
                  // row.
                  usageSlot={usageSlot}
                />
              </div>
            </div>
          )}
      </div>
      </div>
      )}

      {mobileBar}

      {killDialog}
    </div>
  )
}

/** EXP-863/EXP-909: the usage overlay — the SAME layout on all four clients
 *  (desktop `usage_sheet.rs`, iOS `AgentUsageSheet`, Android's Usage sheet),
 *  sections separated by hairlines and every meter the same `Meter`:
 *
 *  1. header — the RUN's account: brand mark, its caption (email, else plan,
 *     else signed in/out), and trailing either the health badge or the plan.
 *     The plan string appears HERE and nowhere else in the overlay;
 *  2. its windows — two lines each (title + countdown, then meter + `NN%`),
 *     dimmed and captioned `as of …` when the report is not current
 *     (`usageAge`), `Checking…` while that login has no windows yet. Never
 *     hidden for staleness: aged numbers still beat no numbers;
 *  3. "Context" — one line (`147k / 1000k (14%)` + the cost) over its meter.
 *     EXP-746: THIS run's window, a sibling of the machine's windows — a token
 *     count has no percent window of its own, and folding it into
 *     `usageGroups` would break the ×4 fixture lock;
 *  4. "Accounts" — ONLY the other accounts, each with its icon-only switch,
 *     its `UsageMini` line and its row-specific refusal; hidden when none;
 *  5. ONE footer note — the run-level blocker when every other account is
 *     refused for the same one, else the one-time cost. */
function SessionUsageSections({
  sessionUsage,
  agentUsage,
  agent,
  accountSwitch,
  now,
}: {
  sessionUsage: SessionUsageState | null
  agentUsage: ReturnType<typeof useSessionAgentUsage>
  /** The run's own agent — the header's brand mark. */
  agent: string | null
  accountSwitch: SessionAccountSwitch
  now: Date
}) {
  const { options } = accountSwitch
  // The desktop's `SwitchTarget.current` rule (EXP-909 §1): the run's own
  // `agent_account` when it names a listed profile, else the login the
  // machine's report names by email, else the machine's active login.
  // Unknown = every row is another account.
  const activeIx = activeAccountIndex(options, agentUsage?.account?.email)
  const active = activeIx >= 0 ? options[activeIx] : null
  const others = options.filter((_, ix) => ix !== activeIx)
  // The run's account, as one caption. A machine that reported no profiles at
  // all still names its ambient login through the usage report.
  const header = active?.label ?? (agentUsage?.account ? accountCaption(agentUsage.account) : null)
  const headerHealth = active
    ? healthBadgeLabel(active.row.health)
    : agentUsage?.account
      ? healthBadgeLabel(agentHealth(agentUsage.account))
      : null
  const headerPlan = active?.plan ?? agentUsage?.account?.plan ?? null
  // The plan only when the caption is the EMAIL — a plan-only caption would
  // otherwise print it twice.
  const showPlan =
    !headerHealth && headerPlan !== null && headerPlan !== header && header !== null
  // EXP-909: the windows are the RUN's login's, not the machine's active one.
  // `deviceLoginRows` already falls the ACTIVE profile back to the top-level
  // `agentUsage[agent]` slot (the only login a device ever puts there), so
  // this reads the resolved row first and the hook's report only for a machine
  // that reported no accounts at all.
  const activeUsage = active?.row.usage ?? agentUsage?.usage ?? null
  const windowsPending =
    active !== null &&
    usageState({
      signedIn: active.row.signedIn,
      unmonitored: active.row.unmonitored,
      usage: active.row.usage,
    }) === `checking`
  const cost = sessionUsage ? formatUsageCost(sessionUsage) : null
  // ONE footer sentence: the blocker every other row shares, else the cost.
  const blocker = globalSwitchBlocker(others)
  const footer = others.length > 0 ? (blocker ?? SWITCH_COST_NOTE) : null
  const section = `border-t border-border/60 px-3 py-2.5`
  const title = `text-[11px] uppercase tracking-wide text-muted-foreground`
  return (
    <>
      <div className="flex min-w-0 items-center gap-2 px-3 py-2.5">
        {agent && <AgentMark agent={agent} className="size-3.5" />}
        <span className="min-w-0 flex-1 truncate text-xs" title={header ?? undefined}>
          {header ?? `Usage`}
        </span>
        {headerHealth && (
          <span className="shrink-0 text-[10px] font-medium text-amber-500">
            {headerHealth}
          </span>
        )}
        {showPlan && (
          <span className="shrink-0 text-[10px] text-muted-foreground">
            {headerPlan}
          </span>
        )}
      </div>
      {activeUsage && !windowsPending ? (
        <div className={section}>
          <UsageWindows usage={activeUsage} now={now} />
        </div>
      ) : windowsPending ? (
        // The login is signed in and this machine simply has not read it yet
        // (a beat or two) — `No usage reported` would read as broken.
        <div className={section}>
          <p className="text-[11px] text-muted-foreground">Checking…</p>
        </div>
      ) : null}
      {sessionUsage && (
        <div className={cn(section, `space-y-1.5`)}>
          <div className="flex items-baseline gap-2 text-xs">
            <span className={title}>{CONTEXT_SECTION_TITLE}</span>
            <span className="tabular-nums">{formatContextUsage(sessionUsage)}</span>
            {cost && (
              <span className="ml-auto text-muted-foreground">{cost}</span>
            )}
          </div>
          <Meter
            value={contextPercent(sessionUsage) ?? 0}
            tone={severity(contextPercent(sessionUsage) ?? 0)}
            className="h-1"
          />
        </div>
      )}
      {/* EXP-849: the OTHER accounts on this run's machine — their own mini
          bars and the icon-only switch (claude, own machine, between turns;
          disabled with the reason otherwise). A switch opens the
          continuation run's page by itself. */}
      {others.length > 0 && (
        <div className={cn(section, `space-y-1.5`)}>
          <p className={title}>{ACCOUNTS_SECTION_TITLE}</p>
          <SessionAccountRows
            options={others}
            switchingTo={accountSwitch.switchingTo}
            onSwitch={accountSwitch.switchTo}
            now={now}
            omitReason={blocker}
          />
        </div>
      )}
      {footer && (
        <div className={section}>
          <p className="text-[11px] text-muted-foreground/70">{footer}</p>
        </div>
      )}
    </>
  )
}

// ── Pieces ───────────────────────────────────────────────────────────────────

function CenteredState({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      {children}
    </div>
  )
}

/** Assistant prose — a chat bubble with a small glyph, selectable text. */
/** The trailing "agent is busy" row (EXP-389, rewritten by EXP-850 §5): the
 *  turn's verb, its clock and the tokens it has produced —
 *  `Pondering… (2m 04s · ↓ 12.4k tokens)` — beside the RUNNING AGENT's brand
 *  mark, pulsing. While a workflow runs the text is that workflow's caption
 *  (§7) with the same suffix. A publisher that sends no turn start (codex,
 *  and every pre-EXP-850 desktop) degrades to the old bare "Working…", which
 *  is why the group is optional.
 *
 *  The clock ticks HERE, once a second, so a running turn re-renders one row
 *  rather than the whole transcript. Static under reduced motion. */
function WorkingIndicatorRow({
  agent,
  startedAt,
  tokens,
  workflow,
}: {
  agent: string | null
  startedAt: number | null
  tokens: number | null
  workflow: WorkflowState | null
}) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (startedAt === null) return
    const timer = setInterval(() => setNow(Date.now()), 1_000)
    return () => clearInterval(timer)
  }, [startedAt])
  const caption = workingCaption({
    startedAt,
    now,
    tokens,
    workflow: workflow ?? undefined,
  })
  return (
    <div className={cn(`flex items-center gap-2`, TRANSCRIPT_TOOL_TEXT)}>
      <AgentBrandMark agent={agent} pulse />
      <span className="min-w-0 truncate text-muted-foreground">{caption}</span>
    </div>
  )
}

/** EXP-850 §1/§2: the strip directly above the composer — one line per
 *  background task the CLI is running (`↻`, the repeat concept) and one per
 *  OPEN `wait` tool row ("Waiting on …"). Absent when both are empty; the
 *  wait row itself stays an ordinary tool row in the transcript. */
function BackgroundStrip({ lines }: { lines: BackgroundStripLine[] }) {
  if (lines.length === 0) return null
  return (
    <div
      className="flex flex-col gap-0.5 border-t border-border/60 px-3 py-1.5"
      data-testid="session-background-strip"
    >
      {lines.map((line) => (
        <div
          key={line.key}
          className={cn(
            `flex min-w-0 items-center gap-1.5 text-muted-foreground`,
            TRANSCRIPT_TOOL_TEXT
          )}
        >
          {line.kind === `task` ? (
            <UiRepeatIcon className="size-3 shrink-0" />
          ) : (
            <UiLoadingIcon className="size-3 shrink-0 motion-safe:animate-spin" />
          )}
          <span className="min-w-0 truncate" title={line.text}>
            {line.text}
          </span>
        </div>
      ))}
    </div>
  )
}

/** EXP-861: the strip directly below the background one — one line per
 *  message the agent has not read yet (the queue is on the DEVICE; this is
 *  its latest-wins mirror). A held line carries an X that revokes it; a
 *  `sent` one (EXP-873, already with the agent) draws none. Absent when
 *  nothing is queued. The composer is never disabled by it. */
function QueueStrip({
  messages,
  onRemove,
}: {
  messages: QueuedMessage[]
  onRemove: (entry: QueuedMessage) => void
}) {
  if (messages.length === 0) return null
  return (
    <div
      className="flex flex-col gap-0.5 border-t border-border/60 px-3 py-1.5"
      aria-label={QUEUE_STRIP_TITLE}
      data-testid="session-queue-strip"
    >
      {messages.map((entry) => (
        <div
          key={entry.id}
          className={cn(
            `flex min-w-0 items-center gap-1.5 text-muted-foreground`,
            TRANSCRIPT_TOOL_TEXT
          )}
        >
          <UiQueuedIcon className="size-3 shrink-0" />
          <span className="min-w-0 flex-1 truncate" title={entry.text}>
            {entry.text}
          </span>
          {entry.sent !== true && (
            <Button
              variant="ghost"
              size="icon-xs"
              className="shrink-0"
              aria-label={QUEUE_REMOVE_LABEL}
              title={QUEUE_REMOVE_LABEL}
              onClick={() => onRemove(entry)}
            >
              <UiCloseIcon className="size-3" />
            </Button>
          )}
        </div>
      ))}
    </div>
  )
}

/** A workflow agent's own rows, folded away inside its card (§3) — the same
 *  rows the transcript uses, grouped the same way (EXP-916: a lane's
 *  consecutive edits are ITS edited-files card, not a stack of bare rows). */
function NestedAgentEvents({ items }: { items: FeedItem[] }) {
  return <LaneRows items={items} />
}

/** EXP-916: ONE lane's items — a subagent's fold, a workflow agent's rows, a
 *  subagent's own conversation TAB — through the same projection the main
 *  transcript runs (`groupLaneRows`), so an edit run reads as the card it is
 *  everywhere else. The ONE row switch a lane has: a tab used to carry a copy
 *  of it.
 *
 *  `gaps` is the only difference between the two readings. A conversation tab
 *  IS a transcript, so its rows take the gap ladder (EXP-787); folded inside a
 *  card the lane keeps the tighter uniform rhythm, with its tool rows flush
 *  against each other. */
function LaneRows({ items, gaps = false }: { items: FeedItem[]; gaps?: boolean }) {
  const rows = useMemo(() => groupLaneRows(items), [items])
  return (
    <>
      {rows.map((row, index) => {
        const gap = gaps
          ? transcriptGapClass(
              index === 0 ? null : rowClass(rows[index - 1]),
              rowClass(row)
            )
          : null
        if (row.kind === `edits`) {
          return (
            <div key={row.id} className={gap ?? `py-0.5`}>
              <EditsCardRow items={row.items as ToolItem[]} liveRowId={null} />
            </div>
          )
        }
        if (row.kind === `toolRun`) {
          const tools = (row.items as ToolItem[]).map((tool) => (
            <ToolRow key={tool.id} item={tool} />
          ))
          return gap === null ? (
            <Fragment key={row.id}>{tools}</Fragment>
          ) : (
            <div key={row.id} className={gap}>
              {tools}
            </div>
          )
        }
        if (row.kind === `subagent` || row.kind === `ask`) return null
        const item = row.item
        if (item.kind === `tool`) {
          return gap === null ? (
            <ToolRow key={item.id} item={item} />
          ) : (
            <div key={item.id} className={gap}>
              <ToolRow item={item} />
            </div>
          )
        }
        if (item.kind === `narration`) {
          return (
            <div key={item.id} className={gap ?? `py-0.5`}>
              <NarrationBubble text={item.text} />
            </div>
          )
        }
        if (item.kind === `user_message`) {
          return (
            <div key={item.id} className={gap ?? `py-0.5`}>
              <UserMessageBubble text={item.text} />
            </div>
          )
        }
        return null
      })}
    </>
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
      // EXP-760: agents narrate BARE identifiers ("landed EXP-758"), so the
      // steering feed — and only it — chips those too. Descriptions and
      // comments keep the `#IDENT` contract.
      bareIssueRefs
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
      <IssueRefText text={text} />
    </div>
  )
}

/** Linkified prose whose issue identifiers — `#EXP-758` AND the bare
 *  `EXP-758` agents actually write — render as chips (EXP-760). Only a
 *  RESOLVED, same-team issue chips; everything else stays prose and goes
 *  through the URL linkifier, exactly as `MarkerText` splits around its image
 *  markers. */
function IssueRefText({ text }: { text: string }) {
  const issueRefs = useIssueRefs()
  return (
    <>
      {splitIssueRefs(text, { bare: true }).map((segment, i) => {
        const resolved = segment.identifier
          ? (issueRefs?.resolve(segment.identifier) ?? null)
          : null
        if (resolved)
          return (
            <IssueChip
              key={i}
              issue={resolved}
              onClick={() => issueRefs?.open(resolved.identifier)}
            />
          )
        return linkSegments(segment.text).map((part, j) =>
          part.href ? (
            // break-all: the EXP-430 sign-in URL has no break points.
            <a
              key={`${i}-${j}`}
              href={part.href}
              target="_blank"
              rel="noreferrer"
              className="break-all text-primary underline underline-offset-2 hover:opacity-80"
            >
              {part.text}
            </a>
          ) : (
            <Fragment key={`${i}-${j}`}>{part.text}</Fragment>
          ),
        )
      })}
    </>
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
    <div className="flex items-start gap-2">
      <CodingAssistantIcon className="mt-1.5 size-3 shrink-0 text-muted-foreground/60" />
      <div
        className={cn(`min-w-0 flex-1 text-foreground/90`, TRANSCRIPT_BODY_TEXT)}
      >
        <FeedText text={text} ariaLabel="Agent message" hardBreaks />
      </div>
    </div>
  )
})

/** EXP-783 — the window anchor that means "the feed's first row, whatever it
 *  is": set when the reader asks for a page the feed does not hold yet, so a
 *  prepended page is inside the window the moment it lands. Every real id is
 *  above it (ids only ever count down at the front by a finite amount). */
const WINDOW_FROM_START = Number.NEGATIVE_INFINITY

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

/** Splits the prose around its `[Image #N]` markers (EXP-698) and draws each
 *  one as a chip that opens that image. A message carrying markers came from
 *  the composer as chat prose, so the inline linkified rendering is the right
 *  one — markdown blocks could not flow around an inline chip anyway. */
function MarkerText({
  text,
  count,
  onOpen,
}: {
  text: string
  /** How many images the message actually carries. A marker outside
   *  `1..count` — hand-typed, or left behind by an edit — is prose, not a
   *  chip: chipping it would promise a preview there is no image for. */
  count: number
  onOpen: (index: number) => void
}) {
  // A capture group keeps the delimiters, and a literal keeps `lastIndex`
  // out of it (the exported pattern is global).
  const parts = text.split(/(\[Image #\d+\])/)
  return (
    <div className="whitespace-pre-wrap break-words">
      {parts.map((part, i) => {
        const marker = /^\[Image #(\d+)\]$/.exec(part)
        if (marker && Number(marker[1]) >= 1 && Number(marker[1]) <= count) {
          const index = Number(marker[1])
          return (
            <button
              key={i}
              type="button"
              className="align-middle"
              onClick={() => onOpen(index)}
            >
              <Pill size="sm" mode="readonly" leading={<EditorImageIcon />}>
                {`Image ${index}`}
              </Pill>
            </button>
          )
        }
        return linkSegments(part).map((segment, j) =>
          segment.href ? (
            <a
              key={`${i}-${j}`}
              href={segment.href}
              target="_blank"
              rel="noreferrer"
              className="break-all text-primary underline underline-offset-2 hover:opacity-80"
            >
              {segment.text}
            </a>
          ) : (
            <Fragment key={`${i}-${j}`}>{segment.text}</Fragment>
          ),
        )
      })}
    </div>
  )
}

/** A human turn (EXP-78): the initial prompt or a steered message — rendered
 *  right-aligned like the sender's own chat bubble, long text folded. A
 *  steered message with images (EXP-511) splits into its prose — whose
 *  `[Image #N]` markers become chips — and the embeds themselves, which stay
 *  below it. */
const UserMessageBubble = memo(function UserMessageBubble({
  text,
}: {
  text: string
}) {
  const { text: body, attachmentIds, markers } = parseSteerMessage(text)
  const hasImages = attachmentIds.length > 0
  // Clamp the PROSE, never the wire message: four embed lines are four more
  // "lines" of nothing, and they used to push a two-line steer into the fold
  // — which then hid the images the fold was measuring.
  const { expanded, setExpanded, clampable } = useClampToggle(
    hasImages ? body : text
  )
  const [preview, setPreview] = useState<number | null>(null)
  const openMarker = (index: number) => {
    if (index >= 1 && index <= attachmentIds.length) setPreview(index - 1)
  }
  return (
    <div className="flex justify-end pl-8">
      {/* EXP-696: the natives' neutral glass bubble, not a primary tint —
          slightly brighter than the assistant's glass sections so the
          sender's own turn reads apart from the feed. */}
      <div
        className={cn(
          `min-w-0 rounded-xl border border-glass-stroke-strong bg-glass-active px-3 py-2 text-foreground/90`,
          TRANSCRIPT_BODY_TEXT
        )}
      >
        {/* A height clamp, not `line-clamp`: line clamping needs a plain text
            flow, and a markdown body is a stack of blocks. */}
        <div
          className={cn(clampable && !expanded && `max-h-40 overflow-hidden`)}
        >
          {!hasImages ? (
            <FeedText text={text} ariaLabel="Your message" hardBreaks />
          ) : (
            body &&
            (markers.length > 0 ? (
              <MarkerText
                text={body}
                count={attachmentIds.length}
                onOpen={openMarker}
              />
            ) : (
              <FeedText text={body} ariaLabel="Your message" hardBreaks />
            ))
          )}
        </div>
        {clampable && (
          <ShowMoreButton
            expanded={expanded}
            onToggle={() => setExpanded((v) => !v)}
          />
        )}
        {/* OUTSIDE the fold: the images are the point of the message, and a
            long prose clamp must never be what hides them. */}
        {hasImages && (
          <div className={cn(`flex flex-col gap-2`, body && `mt-2`)}>
            {attachmentIds.map((id, i) => (
              <button
                key={id}
                type="button"
                onClick={() => setPreview(i)}
                className="block"
              >
                <img
                  src={`/api/attachments/${id}`}
                  alt={`Image ${i + 1}`}
                  className="max-h-64 w-auto max-w-full rounded-md border border-glass-stroke-card"
                />
              </button>
            ))}
          </div>
        )}
      </div>
      {preview !== null && attachmentIds[preview] && (
        <ImagePreviewDialog
          open
          onOpenChange={(open) => {
            if (!open) setPreview(null)
          }}
          src={`/api/attachments/${attachmentIds[preview]}`}
          label={`Image ${preview + 1}`}
        />
      )}
    </div>
  )
})

/** EXP-724: a steered slash command. Right-aligned like the sender's own
 *  bubble (it IS their turn), but a compact pill — `/compact` is an
 *  instruction to the tool, not prose worth a chat bubble. */
function CommandRow({ name, args }: { name: string; args: string }) {
  return (
    <div className="flex justify-end pl-8">
      <div
        className={cn(
          `flex min-w-0 items-center gap-1.5 rounded-xl border border-glass-stroke-strong bg-glass-active px-3 py-1.5`,
          TRANSCRIPT_TOOL_TEXT
        )}
      >
        <CodingCommandIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="shrink-0 font-mono">/{name}</span>
        {args && (
          <span className="truncate text-muted-foreground">{args}</span>
        )}
      </div>
    </div>
  )
}

/** EXP-724: the hairline a finished compaction leaves in the transcript —
 *  everything above it is no longer in the agent's context. */
function CompactionRow() {
  return (
    <div className={cn(`flex items-center gap-2`, TRANSCRIPT_TOOL_TEXT)}>
      <span className="h-px flex-1 bg-border/60" />
      <span className="shrink-0 text-muted-foreground/70">
        {COMPACTED_LABEL}
      </span>
      <span className="h-px flex-1 bg-border/60" />
    </div>
  )
}

type AnswerHandler = (
  item: QuestionItem,
  keys: string[],
  labels: string[],
  /** EXP-513: the typed reply for a `freeText` option. */
  text?: string
) => void

type SendHandler = (text: string) => boolean

/** The interactive half of a question card: the options, the immediate lock
 *  once an answer goes out, and the resolved answer. ONE answer path (EXP-672):
 *  the card's wire id rides the semantic `answer` frame and the desktop
 *  confirms with `answer_ack`. A card WITHOUT an id comes from a desktop too
 *  old to publish one — it renders read-only with an update hint rather than a
 *  dead control (the raw-keystroke fallback is gone).
 *
 *  EXP-788: ONE panel on all four clients — a vertical list of full-width
 *  option buttons wearing number chips, `1`-`9` and Enter selecting while no
 *  field has focus. EXP-820: the free answer is typed IN the card — a
 *  question's "Type something." row and a plan's reject row open an inline
 *  field under themselves (the composer is hidden while a card is pending).
 *  `editing` re-opens an answered step of a still-open ask so its answer can
 *  be changed (the engine re-records and re-resolves it in place). */
function QuestionPrompt({
  item,
  active,
  canAnswer,
  answerState,
  onAnswer,
  onSend,
  variant = `default`,
  hotkeys = false,
  editing = false,
  onAnswered,
}: {
  item: QuestionItem
  /** Still answerable per the feed — the session is blocked on this card. */
  active: boolean
  /** Live (and not ended) — whether this client may answer at all. */
  canAnswer: boolean
  answerState?: AnswerState
  onAnswer: AnswerHandler
  /** A plan's typed feedback follows its reject as the next message. */
  onSend?: SendHandler
  /** `plan`/`submit` promote the first option to the primary action. */
  variant?: `default` | `plan` | `submit`
  /** This is THE pending card: the number keys and Enter act on it. */
  hotkeys?: boolean
  /** EXP-820: an answered step being revisited — answerable although
   *  resolved, starting from what was chosen. */
  editing?: boolean
  /** Fires once an answer went out (the stepper closes its edit on it). */
  onAnswered?: () => void
}) {
  const locked = isAnswerLocked(answerState)
  /** A card an old desktop published without a wire id — unanswerable here. */
  const unanswerable = item.questionId === undefined
  const answerable =
    active &&
    canAnswer &&
    !locked &&
    !unanswerable &&
    (editing || item.resolved !== true)
  const [picked, setPicked] = useState<string[]>(() =>
    editing ? recordedKeys(item) : []
  )
  /** EXP-820: the option whose inline field is open, by key. */
  const [typing, setTyping] = useState<string | null>(null)

  const labelsFor = (keys: string[]) =>
    item.options.filter((o) => keys.includes(o.key)).map((o) => o.label)

  const answer = (keys: string[], labels: string[], text?: string) => {
    onAnswer(item, keys, labels, text)
    onAnswered?.()
  }

  const choose = (option: QuestionOption, index: number) => {
    if (!answerable) return
    if (item.multiSelect) {
      // The picks stay local until the answer frame goes out.
      setPicked((prev) =>
        prev.includes(option.key)
          ? prev.filter((k) => k !== option.key)
          : [...prev, option.key]
      )
      return
    }
    // EXP-820: the free-text row (and a plan's reject) opens its field.
    if (opensInlineField(item, index)) {
      setTyping((prev) => (prev === option.key ? null : option.key))
      return
    }
    answer([option.key], [option.label])
  }

  const submitPicked = () => {
    if (!answerable || picked.length === 0) return
    answer(picked, labelsFor(picked))
  }

  /** The inline field's send: a question's typed reply rides the row's key
   *  as `text`; a plan's feedback rejects and follows as the next message
   *  (`freeAnswerFor`), and an EMPTY plan send is the plain reject. */
  const submitInline = (option: QuestionOption, text: string) => {
    if (!answerable) return
    const trimmed = text.trim()
    if (item.planMode) {
      if (trimmed.length === 0) {
        answer([option.key], [option.label])
        return
      }
      const free = freeAnswerFor(item, trimmed)
      if (!free) return
      answer(free.keys, free.labels, free.text)
      if (free.followUp !== undefined) onSend?.(free.followUp)
      return
    }
    if (trimmed.length === 0) return
    answer([option.key], [trimmed], trimmed)
  }

  // EXP-788: `1`-`9` pick the option with that chip, Enter takes the primary
  // (or submits the multi-select picks) — never while a field has focus (the
  // inline answer field, a dialog's input), never with a modifier, a dialog
  // or a popover in the way. The listener is window-wide because the card
  // itself never holds focus.
  const hot = answerable && hotkeys && typing === null
  const chooseRef = useRef({ choose, submitPicked })
  chooseRef.current = { choose, submitPicked }
  useEffect(() => {
    if (!hot) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return
      if (event.isComposing) return
      const target = event.target as HTMLElement | null
      if (
        target &&
        (target.tagName === `INPUT` ||
          target.tagName === `TEXTAREA` ||
          target.isContentEditable)
      ) {
        return
      }
      if (
        document.querySelector(
          `[role="dialog"][data-state="open"], [data-radix-popper-content-wrapper]`
        )
      ) {
        return
      }
      if (event.key === `Enter`) {
        if (event.shiftKey) return
        if (item.multiSelect) {
          chooseRef.current.submitPicked()
          event.preventDefault()
          return
        }
        const primary = item.options[0]
        if (!primary) return
        event.preventDefault()
        chooseRef.current.choose(primary, 0)
        return
      }
      const option = optionForHotkey(item.options, event.key)
      if (!option) return
      event.preventDefault()
      chooseRef.current.choose(option, item.options.indexOf(option))
    }
    window.addEventListener(`keydown`, onKeyDown)
    return () => window.removeEventListener(`keydown`, onKeyDown)
  }, [hot, item])

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
  if (item.resolved === true && !editing) {
    return (
      <AnsweredLine answer={item.answer} dismissed={item.dismissed === true} />
    )
  }

  return (
    <>
      <div className="mt-2 flex flex-col items-stretch gap-1">
        {item.options.map((option, index) => {
          const primary = variant !== `default` && index === 0
          const selected = picked.includes(option.key)
          const open = typing === option.key
          const chip = optionHotkey(index)
          const label =
            variant === `submit` && index === 0 ? `Submit answers` : option.label
          if (!answerable) {
            return (
              <span
                key={option.key}
                className="flex items-baseline gap-1.5 text-xs text-muted-foreground"
              >
                {chip && <span className="font-mono">{chip}</span>}
                <span>{label}</span>
              </span>
            )
          }
          return (
            <Fragment key={option.key}>
              <Button
                variant={primary ? `default` : `outline`}
                size="sm"
                className={cn(
                  // EXP-850 §13: an option is a ROW, so it wears the row
                  // radius (design token `radius.md` = 10px) ×4 — never the
                  // Button's capsule.
                  `h-auto min-h-8 w-full justify-start whitespace-normal rounded-md py-1.5 text-left text-xs`,
                  // EXP-820: styleguide — the promoted option is the primary
                  // fill, a pick (and an open field's row) the glass active
                  // fill; nothing on this card is blue.
                  !primary &&
                    (selected || open) &&
                    `border-glass-stroke-active bg-glass-active`
                )}
                aria-pressed={item.multiSelect ? selected : undefined}
                aria-expanded={opensInlineField(item, index) ? open : undefined}
                onClick={() => choose(option, index)}
              >
                {item.multiSelect ? (
                  selected ? (
                    <UiSelectedIcon className="size-3.5 shrink-0 text-foreground" />
                  ) : (
                    <UiUnselectedIcon className="size-3.5 shrink-0 text-muted-foreground" />
                  )
                ) : null}
                <span className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
                  <span>{label}</span>
                  {option.description && (
                    <span
                      className={cn(
                        `font-normal text-[0.6875rem]`,
                        primary
                          ? `text-primary-foreground/80`
                          : `text-muted-foreground`
                      )}
                    >
                      {option.description}
                    </span>
                  )}
                </span>
                {chip && (
                  <kbd
                    className={cn(
                      `ml-auto shrink-0 rounded-sm border px-1 font-mono text-[0.625rem] font-normal leading-4`,
                      primary
                        ? `border-primary-foreground/30 text-primary-foreground/80`
                        : `border-glass-stroke-card text-muted-foreground`
                    )}
                  >
                    {chip}
                  </kbd>
                )}
              </Button>
              {open && (
                <InlineAnswerField
                  placeholder={
                    item.planMode
                      ? PLAN_FEEDBACK_PLACEHOLDER
                      : FREE_TEXT_PLACEHOLDER
                  }
                  // A plan's reject stands on its own; a typed reply needs text.
                  allowEmpty={item.planMode}
                  onSubmit={(text) => submitInline(option, text)}
                  onCancel={() => setTyping(null)}
                />
              )}
            </Fragment>
          )
        })}
      </div>
      {answerable && item.multiSelect && (
        <Pill
          size="sm"
          mode="action"
          className="mt-2"
          disabled={picked.length === 0}
          onClick={submitPicked}
        >
          Submit
        </Pill>
      )}
      {answerState?.status === `error` && (
        <div className="mt-1.5 text-[0.6875rem] text-amber-400">
          No confirmation from the desktop. Pick again to retry.
        </div>
      )}
      {unanswerable && active && canAnswer && (
        <div className="mt-2 text-xs text-muted-foreground">
          Update the desktop app to answer this here.
        </div>
      )}
      {active && !canAnswer && !unanswerable && (
        <div className="mt-2 text-xs text-muted-foreground">
          {item.planMode
            ? `Waiting for approval. You're viewing read-only.`
            : `Waiting for an answer. You're viewing read-only.`}
        </div>
      )}
    </>
  )
}

/** EXP-820: the keys behind a resolved step's answer — the options whose
 *  labels the resolution lists — so revisiting the step starts from them. */
function recordedKeys(item: QuestionItem): string[] {
  if (!item.answer) return []
  const labels = new Set(item.answer.split(`, `))
  return item.options.filter((o) => labels.has(o.label)).map((o) => o.key)
}

/** EXP-820: the inline answer field an option row opens under itself — the
 *  composer's own card (chrome and send glyph), a plain field inside. Enter
 *  sends, Shift+Enter breaks the line, Escape closes it. */
function InlineAnswerField({
  placeholder,
  allowEmpty,
  onSubmit,
  onCancel,
}: {
  placeholder: string
  /** A plan's reject sends with nothing typed; a typed reply never does. */
  allowEmpty: boolean
  onSubmit: (text: string) => void
  onCancel: () => void
}) {
  const [text, setText] = useState(``)
  const sendable = allowEmpty || text.trim().length > 0
  return (
    <Composer
      className="mt-0.5"
      submit={
        <ComposerSubmit disabled={!sendable} onClick={() => onSubmit(text)} />
      }
    >
      <Textarea
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        rows={1}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return
          if (e.key === `Escape`) {
            e.preventDefault()
            onCancel()
            return
          }
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            if (sendable) onSubmit(text)
          }
        }}
        className="max-h-32 min-h-9 border-none bg-transparent px-3 pb-1 pt-3 text-sm shadow-none focus-visible:border-transparent"
      />
    </Composer>
  )
}

/** EXP-784: the agent's rate-limit window, in the status stack beside the
 *  compaction strip — its own message (else a status fallback) and the local
 *  reset time when it named one. Hand-mirrored copy ×4 (`rateLimitBanner`).
 *  EXP-831: ticks so the countdown moves and the banner drops itself once
 *  the reset is behind us, without waiting on a slot update. */
function RateLimitBanner({
  state,
  onSwitchAccount,
}: {
  state: SessionRateLimitState
  /** EXP-849: open the account rows — the PRIMARY way out of a wall. Absent
   *  when this run has no other account to move to. */
  onSwitchAccount?: () => void
}) {
  const now = useNow(30_000)
  const banner = rateLimitBanner(state, now)
  if (!banner) return null
  const { text, resets } = banner
  return (
    <div className="flex items-center gap-1.5 border-t border-border/60 px-3 py-2 text-xs text-amber-400">
      <UiUsageIcon className="size-3 shrink-0" />
      <span className="min-w-0 truncate">{text}</span>
      {resets && (
        <span className="shrink-0 text-muted-foreground">{resets}</span>
      )}
      {onSwitchAccount && (
        <Pill
          size="sm"
          mode="action"
          className="ml-auto shrink-0"
          onClick={onSwitchAccount}
        >
          <UiSwapIcon className="size-3" />
          {WALL_SWITCH_LABEL}
        </Pill>
      )}
    </div>
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

/** The chrome BOTH ask cards wear (EXP-698): the NEUTRAL glass card, and per
 *  the styleguide (EXP-820) only the header line is tinted — primary for a
 *  plan, yellow (tokens.json `semantic.yellow`) for a question — never blue.
 *  The two cards below stay separate components (a single-question card and a
 *  multi-step stepper have almost no body in common) but cannot drift apart
 *  on chrome. */
function AskCard({
  plan,
  label,
  meta,
  children,
}: {
  plan?: boolean
  label?: ReactNode
  /** A trailing note beside the label — the stepper's "k of n". */
  meta?: ReactNode
  children: ReactNode
}) {
  return (
    <GlassCard className="p-3">
      <div className="flex items-start gap-2">
        {plan ? (
          <CodingPlanIcon className="mt-0.5 size-3.5 shrink-0 text-primary" />
        ) : (
          <UiHelpIcon className="mt-0.5 size-3.5 shrink-0 text-yellow-400" />
        )}
        <div className="min-w-0 flex-1">
          {(label !== undefined || meta !== undefined) && (
            <div className="mb-1 flex items-center gap-2">
              {label !== undefined && (
                <span
                  className={cn(
                    `truncate text-xs font-medium`,
                    plan ? `text-primary` : `text-yellow-400`
                  )}
                >
                  {label}
                </span>
              )}
              {meta}
            </div>
          )}
          {children}
        </div>
      </div>
    </GlassCard>
  )
}

/** A standalone question (EXP-78): a plan approval, or an AskUserQuestion from
 *  a desktop that publishes no ask grouping. `planMode` cards (EXP-97) get a
 *  "Plan ready" presentation with the first option as the primary approve
 *  action and the plan ALWAYS rendered as markdown in full — a plan never
 *  folds behind "Show more" (EXP-738, iOS/Android parity) — labels/keys
 *  always come from the wire `options`, the desktop owns the TUI key
 *  mapping. */
function QuestionCard({
  item,
  active,
  canAnswer,
  answerState,
  onAnswer,
  onSend,
  hotkeys,
}: {
  item: QuestionItem
  active: boolean
  canAnswer: boolean
  answerState?: AnswerState
  onAnswer: AnswerHandler
  onSend: SendHandler
  /** EXP-788: this is the card the number keys answer. */
  hotkeys: boolean
}) {
  const plan = item.planMode
  const clamp = useClampToggle(item.text)
  // The plan is the thing the reader must approve — it is never folded.
  const clampable = !plan && clamp.clampable
  const { expanded, setExpanded } = clamp
  return (
    <AskCard plan={plan} label={plan ? `Plan ready` : item.header}>
      <>
          {plan ? (
            // The plan is GFM markdown — always rendered as markdown, always
            // in full (EXP-738).
            <div className="text-sm">
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
                clampable && !expanded && `max-h-40 overflow-hidden`
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
            onSend={onSend}
            variant={plan ? `plan` : `default`}
            hotkeys={hotkeys}
          />
      </>
    </AskCard>
  )
}

/** One multi-question ask (protocol v2 `askId`), claude-style: one question at
 *  a time with "k of n" progression, answered steps collapsed behind their
 *  chosen answer, and the ask's final review step rendered with an explicit
 *  "Submit answers" button once the desktop publishes it. The stepper advances
 *  the moment a step locks — `answer_ack` then confirms it.
 *
 *  EXP-820: back and forth — while the ask is still open (`view.complete` is
 *  false) an answered step is a button that re-opens it: the step expands
 *  with its options (the recorded answer pre-picked) and the step the ask is
 *  actually on folds into a row that leads back. A new pick sends an `answer`
 *  for THAT step's id; the engine re-records it and re-resolves the card in
 *  place, the stepper stays where it was. */
function AskStepperCard({
  items,
  activeIds,
  canAnswer,
  answerStates,
  onAnswer,
  onSend,
  pendingId,
}: {
  items: QuestionItem[]
  activeIds: Set<number>
  canAnswer: boolean
  answerStates: AnswerStates
  onAnswer: AnswerHandler
  onSend: SendHandler
  /** EXP-788: the id of the card the number keys answer, if it is one of ours. */
  pendingId: number | null
}) {
  const view = askStepperView(items, answerStates)
  const current =
    view.steps.find((s) => s.phase === `current`) ??
    (view.submit?.phase === `current` ? view.submit : null)
  const answered = view.steps.filter((s) => s.phase === `answered`)
  const header = current?.item.header ?? items[0]?.header
  const submitStep = current !== null && current.item.index === undefined

  // EXP-820: the answered step being revisited, while it still can be.
  const [editingId, setEditingId] = useState<number | null>(null)
  const editable = canAnswer && !view.complete
  const editing =
    editable && editingId !== null
      ? (answered.find((s) => s.item.id === editingId) ?? null)
      : null
  useEffect(() => {
    if (editingId !== null && editing === null) setEditingId(null)
  }, [editingId, editing])

  return (
    <AskCard
      label={header ?? (submitStep ? `Review answers` : `Question`)}
      meta={
        view.total > 1 ? (
          <span className="shrink-0 text-[0.6875rem] text-muted-foreground">
            {current && !submitStep
              ? `${view.position} of ${view.total}`
              : `${view.total} questions`}
          </span>
        ) : undefined
      }
    >
      <>
          {answered.map((step) =>
            editing?.item.id === step.item.id ? (
              <div key={step.item.id} className="mt-1.5">
                <div className="text-sm text-foreground/90">
                  <FeedText text={step.item.text} ariaLabel="Question" />
                </div>
                <QuestionPrompt
                  key={`edit-${step.item.id}`}
                  item={step.item}
                  active
                  canAnswer={canAnswer}
                  answerState={answerStates[answerKey(step.item)]}
                  onAnswer={onAnswer}
                  onSend={onSend}
                  hotkeys
                  editing
                  onAnswered={() => setEditingId(null)}
                />
                {current && (
                  <button
                    type="button"
                    className="mt-1.5 text-xs text-muted-foreground hover:text-foreground"
                    onClick={() => setEditingId(null)}
                  >
                    {BACK_TO_CURRENT_STEP}
                  </button>
                )}
              </div>
            ) : (
              <AnsweredStepRow
                key={step.item.id}
                text={step.item.text}
                answer={step.answer}
                dismissed={step.item.dismissed === true}
                onEdit={
                  editable && !isAnswerLocked(answerStates[answerKey(step.item)])
                    ? () => setEditingId(step.item.id)
                    : undefined
                }
              />
            )
          )}
          {current ? (
            editing ? (
              // EXP-820: the step the ask is on, folded while another is
              // being revisited — the way forward again.
              <button
                type="button"
                className="mt-1 flex w-full items-center gap-1.5 py-1 text-left text-xs text-muted-foreground hover:text-foreground"
                onClick={() => setEditingId(null)}
              >
                <ChevronRight className="size-3 shrink-0" />
                <span className="min-w-0 flex-1 truncate">
                  {submitStep ? `Review answers` : current.item.text}
                </span>
              </button>
            ) : (
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
                  onSend={onSend}
                  variant={submitStep ? `submit` : `default`}
                  hotkeys={pendingId === current.item.id}
                />
              </div>
            )
          ) : (
            view.waiting && (
              <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
                <UiLoadingIcon className="size-3 animate-spin" />
                Waiting for the next question…
              </div>
            )
          )}
      </>
    </AskCard>
  )
}

/** An answered step inside the stepper — the question, folded to one line,
 *  next to what was chosen. EXP-820: with `onEdit` the row is a button that
 *  re-opens the step (a trailing edit glyph says so). */
function AnsweredStepRow({
  text,
  answer,
  dismissed,
  onEdit,
}: {
  text: string
  answer?: string
  dismissed: boolean
  onEdit?: () => void
}) {
  const body = (
    <>
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
    </>
  )
  if (onEdit) {
    return (
      <button
        type="button"
        className="-mx-1 flex w-[calc(100%+0.5rem)] items-center gap-1.5 rounded-md border-b border-border/40 px-1 py-1 text-left text-xs last:border-b-0 hover:bg-glass-row"
        title="Change this answer"
        onClick={onEdit}
      >
        {body}
        <UiEditIcon className="size-3 shrink-0 text-muted-foreground/70" />
      </button>
    )
  }
  return (
    <div className="flex items-center gap-1.5 border-b border-border/40 py-1 text-xs last:border-b-0">
      {body}
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
    <div className={cn(`min-w-0 pl-0.5`, TRANSCRIPT_TOOL_TEXT)}>
      <div className="flex min-w-0 items-center gap-2">
        <UiPermissionIcon className="size-3 shrink-0 text-amber-400/70" />
        <span className="shrink-0 font-medium text-amber-400/90">
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
 *  chevron on an empty group would expand to nothing (EXP-350).
 *  EXP-748: the CAPTION counts what the publisher reported (`toolCount`), the
 *  chevron keys on the rows actually here — a replayed run can honestly say
 *  "12 tool calls" and expand to the handful that survived the buffer. */
function SubagentGroupRow({ items }: { items: FeedItem[] }) {
  const [expanded, setExpanded] = useState(false)
  const tools = items.filter(
    (i): i is Extract<FeedItem, { kind: `tool` }> => i.kind === `tool`
  )
  const summary = summarizeSubagentRow(items)
  const { done, detail, toolCount } = summary
  // EXP-847: the same contract caption a collapsed tool group carries
  // ("Ran 4 commands · edited 2 files"), not a bare "N tool calls". When the
  // publisher counted more calls than survived the replay buffer, the extras
  // count as `other` — the caption then reads "Used 12 tools", never a lie
  // about what they were.
  const caption = useMemo(
    () =>
      toolGroupCaption(
        tools.length >= toolCount
          ? tools
          : [
              ...tools,
              ...Array.from({ length: toolCount - tools.length }, () => ({})),
            ]
      ),
    [tools, toolCount]
  )
  const expandable = tools.length > 0
  // EXP-856 §4: a second copy of this agent started while the first was still
  // running. Amber, verbatim off the wire, and OUTSIDE the fold — a collapsed
  // group must not hide the one row that says two agents are editing the same
  // files.
  const duplicate = summary.duplicateDetail
  const header = (
    <>
      <CodingSubagentIcon className="size-3 shrink-0 text-muted-foreground/60" />
      {/* EXP-847: the spawning call's description names the subagent; the
          agent type stays a secondary caption beside it. */}
      <span className="shrink-0 font-medium">{subagentLabel(summary)}</span>
      {summary.title && (
        <span className="shrink-0 text-[0.6875rem]">{summary.agentType}</span>
      )}
      {!done && <UiLoadingIcon className="size-3 shrink-0 animate-spin" />}
      <span className="shrink-0 text-[0.6875rem]">
        {done ? `done` : `running`}
        {toolCount > 0 && ` · ${caption}`}
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
      <div className={cn(`min-w-0`, TRANSCRIPT_TOOL_TEXT)}>
        <div className="flex min-w-0 items-center gap-2 pl-0.5 text-muted-foreground">
          {header}
        </div>
        {duplicate && (
          <div className="pl-0.5 pt-0.5">
            <DuplicateWarningRow detail={duplicate} />
          </div>
        )}
      </div>
    )
  }
  return (
    <div className={cn(`min-w-0`, TRANSCRIPT_TOOL_TEXT)}>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full min-w-0 items-center gap-2 pl-0.5 text-left text-muted-foreground hover:text-foreground"
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0" />
        ) : (
          <ChevronRight className="size-3 shrink-0" />
        )}
        {header}
      </button>
      {duplicate && (
        <div className="pl-0.5 pt-0.5">
          <DuplicateWarningRow detail={duplicate} />
        </div>
      )}
      {expanded && (
        <div className="ml-5">
          <LaneRows items={tools} />
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
    <Pill
      size="sm"
      mode="select"
      selected={active}
      leading={
        running ? (
          <UiLoadingIcon className="size-3 shrink-0 animate-spin" />
        ) : undefined
      }
      onClick={onClick}
    >
      {label}
    </Pill>
  )
}

/** A focused subagent conversation (EXP-356): its delegation summary on top,
 *  then its stream in feed order — the flat feed stays the state, this is a
 *  per-agent projection like the grouped main view. EXP-773: the subagent's
 *  own narration and user turns render here (interleaved with its tool rows),
 *  because they are hidden from Main. */
function AgentConversation({
  summary,
  items,
}: {
  summary?: SubagentSummary
  items: FeedItem[]
}) {
  // A tab shows the agent's STREAM: its tool calls and the prose and user
  // turns the mapper stamped with its id. Its lifecycle edges and everything
  // else are the summary's business, and dropping them here keeps two calls
  // either side of one from reading as two runs.
  const stream = useMemo(
    () =>
      items.filter(
        (item) =>
          item.kind === `tool` ||
          item.kind === `narration` ||
          item.kind === `user_message`
      ),
    [items]
  )
  return (
    <>
      {summary && (
        <div
          className={cn(
            `flex min-w-0 items-center gap-2 py-0.5 pl-0.5 text-muted-foreground`,
            TRANSCRIPT_TOOL_TEXT
          )}
        >
          <CodingSubagentIcon className="size-3 shrink-0 text-muted-foreground/60" />
          <span className="shrink-0 font-medium">
            {subagentLabel(summary)}
          </span>
          {summary.title && (
            <span className="shrink-0 text-[0.6875rem]">{summary.agentType}</span>
          )}
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
      {stream.length === 0 ? (
        <div className="py-1 pl-0.5 text-xs text-muted-foreground">
          Nothing from this agent yet.
        </div>
      ) : (
        /* EXP-787: a subagent's conversation is a transcript too — same rows,
           same gap ladder. */
        <LaneRows items={stream} gaps />
      )}
    </>
  )
}

/** Tool-call headline — compact single line, consecutive rows visually tight.
 *  `flush` drops the row's own padding: in the main transcript the gap ladder
 *  (EXP-787) supplies the rhythm, while the rows NESTED in an expanded tool
 *  group or a subagent conversation keep their tighter inner one.
 *  EXP-786: an `edit` call's own unified diff renders under the headline in a
 *  bounded box (the publisher already cut it to the contract's caps; the cut
 *  note becomes a muted footer, never a diff line); a `failed` call is tinted
 *  rose. The pinned "Latest changes" bar is untouched — that is the whole
 *  worktree, this is the one call.
 *
 *  EXP-895: the transcript runs inside the flow, so `live` — set for the ONE
 *  row `liveToolRowId` names — is the only row that opens itself: its diff
 *  cards unfold, its output box is up. Every other row is the headline plus
 *  its compact evidence (the collapsed file cards' `+a −b`, the `failed`
 *  chip), and the reader's own tap still opens a settled one. */
function ToolRow({
  item,
  flush = false,
  live = false,
}: {
  item: ToolItem
  flush?: boolean
  /** EXP-895: this call is the one still RUNNING (`liveToolRowId`). */
  live?: boolean
}) {
  const failed = item.failed === true
  // EXP-895: `null` = "whatever the flow says" (open while the call runs), and
  // the reader's tap pins it either way. The pin drops on the live edge, so a
  // row folds by itself once the transcript has moved past it — and a row the
  // reader opened AFTER it settled stays open, because `live` no longer moves.
  const [pinned, setPinned] = useState<boolean | null>(null)
  useEffect(() => setPinned(null), [live])
  const open = pinned ?? live
  // EXP-846: one of OUR MCP tools reads as a sentence with our mark on it —
  // "Created issue · <title>" plus the preview its answer carried — instead of
  // the raw `mcp__exponential__exponential_issues_create`.
  const exp = expToolDisplay(item.name)
  if (exp) return <ExpToolRow item={item} display={exp} flush={flush} />
  const headline = (
    <div
      className={cn(
        `flex min-w-0 items-center gap-2`,
        TRANSCRIPT_TOOL_TEXT,
        failed && `text-rose-400`
      )}
    >
      <CodingToolIcon
        className={cn(
          `size-3 shrink-0`,
          failed ? `text-rose-400/70` : `text-muted-foreground/60`
        )}
      />
      <span className="shrink-0 font-medium">{item.name}</span>
      {item.detail && (
        <span
          className={cn(
            `truncate font-mono text-[0.6875rem]`,
            failed ? `text-rose-400/80` : `text-muted-foreground`
          )}
          title={item.detail}
        >
          {item.detail}
        </span>
      )}
      {failed && <span className="shrink-0 text-[0.6875rem]">failed</span>}
      {item.output !== undefined && (
        <>
          {/* The chevron sits on the TRAILING edge (iOS/Android parity): a
              leading one would indent the log-carrying rows out of line with
              every other tool row in the same run. */}
          <span className="min-w-0 flex-1" />
          {open ? (
            <ChevronDown className="size-3 shrink-0 text-muted-foreground/60" />
          ) : (
            <ChevronRight className="size-3 shrink-0 text-muted-foreground/60" />
          )}
        </>
      )}
    </div>
  )
  return (
    <div className={cn(`min-w-0 pl-0.5`, !flush && `py-0.5`)}>
      {item.output === undefined ? (
        headline
      ) : (
        // The output is the only thing a row-level toggle has to reveal —
        // EXP-916 moved an edit's patch into its run's edited-files card.
        <button
          type="button"
          onClick={() => setPinned(!open)}
          className="w-full min-w-0 text-left"
          aria-label={open ? `Hide the output` : `Show the output`}
        >
          {headline}
        </button>
      )}
      {/* EXP-916: an edit call's patch belongs to the edited-files CARD its
          run forms (`groupFeedRows`), never to a lone tool row. */}
      {open && item.output !== undefined && (
        <ToolOutput output={item.output} live={live} />
      )}
    </div>
  )
}

/** EXP-895 — what one `execute` call printed, as its settle put it on the wire:
 *  already redacted and tail-cut by the publisher, so this only has to be a
 *  readable box. A cut output OPENS with the `\ N more lines truncated` marker
 *  (the dropped lines were at the front, unlike a patch's trailing note), which
 *  reads as the first line of the log and needs no parsing.
 *
 *  Scrolled to the BOTTOM on mount: the verdict is the last line, and it is why
 *  the output is on the wire at all. */
const ToolOutput = memo(function ToolOutput({
  output,
  live = false,
}: {
  output: string
  /** EXP-910: the call is still RUNNING — show its TAIL
   *  (`liveToolOutputTail`), not the whole log. A command that prints while it
   *  works owns the one open row, and an unbounded one owns the screen. The
   *  settled row (and the reader's own tap on it) still gets everything. */
  live?: boolean
}) {
  const shown = useMemo(
    () =>
      live ? liveToolOutputTail(output, LIVE_TOOL_OUTPUT_TAIL_LINES) : output,
    [live, output]
  )
  const box = useRef<HTMLPreElement | null>(null)
  useEffect(() => {
    const node = box.current
    if (node) node.scrollTop = node.scrollHeight
  }, [shown])
  return (
    <pre
      ref={box}
      className="mt-1 max-h-72 overflow-auto overscroll-contain whitespace-pre-wrap break-words rounded-md border border-border/60 px-2 py-1.5 font-mono text-[0.6875rem] leading-relaxed text-muted-foreground"
    >
      {shown}
    </pre>
  )
})

/** EXP-846: an Exponential MCP call. The brand mark leads (the same asset the
 *  auth shell and the About card draw), then the contract caption —
 *  progressive while the call runs ("Creating issue"), done once it settled
 *  ("Created issue") — then the call's subject, then the preview the engine
 *  distilled from the answer. A FAILED call keeps the generic failed styling
 *  and shows no preview: there is no result to preview. Nothing is forced —
 *  mark plus caption is a complete row when the answer named nothing. */
function ExpToolRow({
  item,
  display,
  flush = false,
}: {
  item: ToolItem
  display: ExpToolDisplay
  flush?: boolean
}) {
  const failed = item.failed === true
  const caption = expToolCaption(display, item.settled === true)
  // The subject is the input field the contract names (`subjectKey`), which
  // the publisher sends as the call's `detail`. Once the call SETTLED the
  // answer's own title/identifier is the better word for the same thing (and
  // the honest one while a publisher derives `detail` generically), so it
  // wins there.
  const subject =
    (item.settled
      ? (item.preview?.title ?? item.preview?.identifier ?? item.detail)
      : item.detail) ?? null
  return (
    <div className={cn(`min-w-0 pl-0.5`, !flush && `py-0.5`)}>
      <div
        className={cn(
          `flex min-w-0 items-center gap-2`,
          TRANSCRIPT_TOOL_TEXT,
          failed && `text-rose-400`
        )}
      >
        <ExponentialLogo
          variant="light"
          size={12}
          className={cn(
            `size-3 shrink-0`,
            failed ? `text-rose-400/70` : `text-muted-foreground/60`
          )}
        />
        <span className="shrink-0 font-medium">{caption}</span>
        {subject && (
          <span
            className={cn(
              `truncate text-[0.6875rem]`,
              failed ? `text-rose-400/80` : `text-muted-foreground`
            )}
            title={subject}
          >
            {subject}
          </span>
        )}
        {failed && <span className="shrink-0 text-[0.6875rem]">failed</span>}
      </div>
      {!failed && item.preview && (
        <ExpToolResult kind={display.result} preview={item.preview} />
      )}
    </div>
  )
}

/** EXP-846: the settled call's result, by contract `result` kind. An issue
 *  gets the very chip an `#IDENT` reference renders (preview on hover, tap
 *  opens the issue) when the row is synced here, and the same chip inert with
 *  a muted glyph when it is not (EXP-887); a PR gets its link; a list its row count; the
 *  named things a small chip. `none` renders nothing at all. */
function ExpToolResult({
  kind,
  preview,
}: {
  kind: string
  preview: ExpToolPreview
}) {
  const issueRefs = useIssueRefs()
  const label = preview.title ?? preview.identifier ?? preview.id ?? null
  if (kind === `issue`) {
    const resolved = preview.id
      ? (issueRefs?.resolveById(preview.id) ??
        (preview.identifier
          ? issueRefs?.resolve(preview.identifier)
          : null) ??
        null)
      : preview.identifier
        ? (issueRefs?.resolve(preview.identifier) ?? null)
        : null
    if (resolved) {
      return (
        <div className="ml-5 pt-0.5">
          <IssueChip
            issue={resolved}
            onClick={() => issueRefs?.open(resolved.identifier)}
          />
        </div>
      )
    }
    if (!preview.identifier && !preview.title) return null
    // EXP-887: an unsynced row is still an ISSUE — it draws the same chip,
    // just inert: no target, no hover preview, and a muted backlog glyph
    // standing in for the status this client cannot resolve yet.
    return (
      <div className="ml-5 pt-0.5">
        <IssueChipView
          identifier={preview.identifier ?? ``}
          title={preview.title ?? ``}
          status={{ icon: `circle-dashed`, colorClass: `text-muted-foreground` }}
        />
      </div>
    )
  }
  if (kind === `pr`) {
    if (!preview.url) return null
    return (
      <div className="ml-5 min-w-0 pt-0.5 text-[0.6875rem]">
        <a
          href={preview.url}
          target="_blank"
          rel="noreferrer"
          className="block truncate text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
          title={preview.url}
        >
          {preview.url}
        </a>
      </div>
    )
  }
  if (kind === `list`) {
    if (preview.count === undefined) return null
    return (
      <div className="ml-5 pt-0.5 text-[0.6875rem] text-muted-foreground">
        {`${preview.count} result${preview.count === 1 ? `` : `s`}`}
      </div>
    )
  }
  if (
    kind === `session` ||
    kind === `board` ||
    kind === `action` ||
    kind === `automation` ||
    kind === `comment`
  ) {
    if (!label) return null
    return (
      <div className="ml-5 pt-0.5">
        <Pill size="sm" className="max-w-[18rem]" title={label}>
          <span className="min-w-0 truncate">{label}</span>
        </Pill>
      </div>
    )
  }
  return null
}

/** The open set of a card nobody has touched and that has no live row — one
 *  shared empty set, so `openPaths` keeps a stable identity. */
const NO_OPEN_PATHS: ReadonlySet<string> = new Set<string>()

/** EXP-916 — the transcript's edited-files CARD: one row per path, the same
 *  `FileDiffCard` every Changes surface draws, folded until the reader opens
 *  one. The projection is the contract's (`editCard`), memoised per CARD —
 *  never once over the whole feed — on `items`, the row's own slice out of the
 *  memoised feed projection: a new array exactly when the projection changed,
 *  which is every way a patch can land or grow (a same-LENGTH rewrite
 *  included).
 *
 *  Open state is ONE nullable set. `null` = follow the live row: while the
 *  card's last member is the LIVE tool row that row is open by itself, so the
 *  reader watches the edit land, and the card folds itself the moment it
 *  settles. A tap replaces it with the reader's own set — seeded from what is
 *  effectively open, so the live row can be collapsed like any other — and
 *  that set survives the settle, because their opens are theirs. */
function EditsCardRow({
  items,
  liveRowId,
}: {
  items: ToolItem[]
  liveRowId: number | null
}) {
  const view = useMemo(() => editCard(items, liveRowId), [items, liveRowId])
  const livePath =
    view.liveIndex === null ? null : (view.rows[view.liveIndex]?.path ?? null)
  const [open, setOpen] = useState<Set<string> | null>(null)
  const openPaths = useMemo(
    () => open ?? (livePath === null ? NO_OPEN_PATHS : new Set([livePath])),
    [open, livePath]
  )
  const toggle = useCallback(
    (path: string) => {
      setOpen((prev) => {
        const next = new Set(prev ?? (livePath === null ? [] : [livePath]))
        if (next.has(path)) next.delete(path)
        else next.add(path)
        return next
      })
    },
    [livePath]
  )
  return <EditedFilesCard view={view} openPaths={openPaths} onToggle={toggle} />
}

/** A run of ≥2 consecutive tool calls collapsed into one row (EXP-97),
 *  expandable to the individual rows. EXP-785: the caption is the contract's
 *  `toolGroupSummary` over the rows' kinds ("Ran 4 commands · edited 2 files
 *  · 1 failed"), byte-identical on every client. While the run is the
 *  trailing row of a live session, the latest call stays visible under the
 *  caption so the viewer still sees live progress. */
function ToolGroupRow({
  items,
  liveTail,
}: {
  items: ToolItem[]
  liveTail: boolean
}) {
  const [expanded, setExpanded] = useState(false)
  const latest = items[items.length - 1]
  const caption = useMemo(() => toolGroupCaption(items), [items])
  return (
    <div className="min-w-0">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className={cn(
          `flex min-w-0 items-center gap-2 pl-0.5 text-muted-foreground hover:text-foreground`,
          TRANSCRIPT_TOOL_TEXT
        )}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0" />
        ) : (
          <ChevronRight className="size-3 shrink-0" />
        )}
        <CodingToolIcon className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="min-w-0 truncate font-medium" title={caption}>
          {caption}
        </span>
      </button>
      {expanded ? (
        <div className="ml-5">
          {items.map((item) => (
            // EXP-895: inside the group only the RUNNING call is expanded —
            // the same rule the top-level rows follow.
            <ToolRow
              key={item.id}
              item={item}
              live={liveTail && item.id === latest.id}
            />
          ))}
        </div>
      ) : (
        liveTail && (
          <div className="ml-5">
            <ToolRow item={latest} live />
          </div>
        )
      )}
    </div>
  )
}
