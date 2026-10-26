import { ASK_DOCK_COPY } from "@exp/domain-contract/ask-dock"
import {
  AGENT_HINT_BANNER,
  AGENT_MENU_SENDS,
  agentInstructionsChanged,
  commandDivider,
} from "@exp/domain-contract/chats"
import type { DockRow, DockSource } from "@exp/domain-contract/dock"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import type { ReactNode } from "react"
import {
  createContext,
  Fragment,
  memo,
  useContext,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react"
import { editCard } from "@exp/domain-contract/edit-card"
import {
  parseTestResults,
  traceFailedTests,
  traceReasoning,
  traceSubagent,
  traceTool,
  TRACE_COPY,
  type TraceLineModel,
} from "@exp/domain-contract/trace"
import { expToolGroupCaption } from "@exp/domain-contract/exp-tool-group"
import { linkSegments } from "@/lib/linkify"
import { Check, X } from "lucide-react"
import {
  AskCard,
  DockSheetBody,
  AskPrompt,
  ThreadDock,
  ThreadConversation,
  TranscriptDivider,
  useLiveArrivals,
  ChatBubble,
  ASK_CARD_COPY,
  conceptIcon,
  EditedFilesCard,
  JumpToBottomButton,
  JUMP_TO_BOTTOM_GAP,
  LoadEarlierButton,
  useIsMobile,
  Button,
  Pill,
  IssueChip as IssueChipView,
  AgentWorkingMark,
  SessionInlineResultTile,
  sessionResultPicture,
  WORK_COLUMN_CLASS,
  DETAIL_FADE_BOTTOM_CLASS,
  ExponentialLogo,
  ImagePreviewDialog,
  AttachmentFileLink,
  AttachmentThumb,
  DisclosureHeader,
  EmptyState,
  WorkCard,
  Trace,
  TraceLine,
  TraceIcon,
  TraceTitle,
  TraceDescription,
  TraceCollapsible,
  TraceCollapsibleTrigger,
  TraceCollapsibleContent,
  CodeBlock,
  CodeBlockHeader,
  CodeBlockTitle,
  CodeBlockActions,
  CodeBlockCopyButton,
  CodeBlockContent,
  ScriptCard,
  STARTING_COPY,
  SkeletonRows,
  type WorkCardLaneTarget,
} from "@exp/ui"
import { codeText } from "@exponential_app/ui"
import { Empty } from "@/components/empty"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useSessionDevice } from "@/hooks/use-session-device"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useNow } from "@/hooks/use-now"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import {
  activeQuestionIds,
  answerKey,
  askStepperView,
  collectSubagents,
  expToolCaption,
  expToolDisplay,
  freeAnswerFor,
  BACK_TO_CURRENT_STEP,
  groupFeedRows,
  groupLaneRows,
  FEED_WINDOW,
  FEED_WINDOW_STEP,
  isAnswerLocked,
  liveToolRowId,
  nestedWorkflowId,
  orphanWorkflowIds,
  pendingAnswerable,
  QUEUE_REMOVE_LABEL,
  QUEUE_STRIP_TITLE,
  type QueuedMessage,
  rateLimitBanner,
  rateLimitDetail,
  rowClass,
  sessionIsWorking,
  subagentLabel,
  subagentIdOf,
  SUBAGENT_FALLBACK_TYPE,
  summarizeSubagentRow,
  toolGroupCaption,
  transcriptGapToken,
  visibleSubagentTabs,
  workflowSubagentIds,
  workflowWorkCard,
  workflowFoldedPhases,
  workflowLaneAgent,
  sessionTaskQueue,
  subagentDurationMs,
  isScriptCall,
  scriptCardInput,
  type AnswerState,
  type AnswerStates,
  type ExpToolDisplay,
  type ExpToolPreview,
  type RowClass,
  type BackgroundTask,
  type TaskListEntry,
  type SubagentSummary,
  type TranscriptGapToken,
  type WorkflowState,
} from "@/lib/agent-feed"
import { anchorSessionCards, sessionCards } from "@exp/domain-contract/ask-dock"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { useSessionMessages } from "@/hooks/use-session-tree"
import { useAnswerMessage } from "@/components/thread/agent-channel"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { FeedText } from "@/components/feed-text"
import { workingCaption } from "@/lib/working-caption"
import {
  COMMENT_CALL_TOOL,
  CommentCallRow,
  RunCardsContext,
} from "@/components/agent-comment-card"
import { scriptCard } from "@exp/domain-contract/script-card"
import taskQueueSpec from "@exp/domain-contract/fixtures/task-queue.json"
import {
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
  type ToolItem,
} from "@/lib/steer-session-store"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { IssueChip } from "@/components/issue-chip"
import { SessionAccountSwitchButton } from "@/components/session-account-switch"
import { sendAgentLine } from "@/components/thread/chat-menu"
import { useAgentRow } from "@/hooks/use-agent-row"
import { EntityRefChips } from "@/components/entity-preview/entity-ref-chips"
import { parseSteerMessage } from "@/lib/steer-image-message"
import {
  buildSessionThread,
  sessionState,
  toTreeMessage,
  toTreeSession,
  type SessionRow,
  type ThreadRow,
} from "@/lib/session-tree"
import { cn } from "@/lib/utils"

// EXP-317: the session glyphs the native clients also draw resolve through
// the shared registry (packages/icons/icons.json).
const CodingAssistantIcon = conceptIcon(`coding-assistant`)
const CodingCommandIcon = conceptIcon(`coding-command`)
const CodingSubagentIcon = conceptIcon(`coding-subagent`)
const EditorImageIcon = conceptIcon(`editor-image`)
const UiDeviceOfflineIcon = conceptIcon(`ui-device-offline`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiPermissionIcon = conceptIcon(`ui-permission`)
const UiRefreshIcon = conceptIcon(`ui-refresh`)
const UiWarningIcon = conceptIcon(`ui-warning`)
const TraceChevronIcon = conceptIcon(`trace-chevron`)
const TraceToolIcon = conceptIcon(`trace-tool`)
const UiQueuedIcon = conceptIcon(`ui-queued`)
const UiCloseIcon = conceptIcon(`ui-close`)
/** EXP-1321: the Queue's test ids, the fixture's (×4). */
const TASK_QUEUE_TEST_IDS = taskQueueSpec.testIds
// EXP-529: multi-select options carry an explicit checkbox state (Android
// parity) — the amber tint alone read as "nothing selected".

// The custom-rendered agent-session viewer (EXP-63). NO terminal rendering:
// the viewer joins the steer relay's scrubbed ACTIVITY channel
// ({"t":"join","channel":"activity"}, apps/steer-relay/src/protocol.ts) and
// renders structured events — narration, compact tool rows, edit cards, the
// Work / Script / Tasks cards — never raw PTY bytes. Question and plan
// answers ride the semantic `answer` frame (steer protocol v2, EXP-249).
//
// EXP-1293 Threads v2: this is the RAW WORK of one session, opened from its
// session card (`/t/$teamSlug/sessions/$sessionId`). It ALWAYS shows the work
// (no Show work, no faces, no Guide, no merge, no issue header, no run
// switching): the route owns the header (back to the thread, the title,
// Stop) and the reply box (`SessionComposer`), handed in as slots.

// ── Transcript rhythm (EXP-787) ──────────────────────────────────────────────
// The measure and the gap ladder are shared tokens (packages/design-tokens
// tokens.json `transcript`, mirrored as --transcript-* in styles.css and
// derived ONCE in lib/agent-feed.ts). The renderer only maps a ladder token
// onto its custom property, so no number is restated here.

/** The centred reading column every transcript row sits in. EXP-1191: on
 *  md+ its gutter is the work column's, a phone keeps its 12. */
const TRANSCRIPT_COLUMN = `${WORK_COLUMN_CLASS} px-3 md:px-5`
/** NEW-6: how long an idle session's connect may show placeholders. */
const TRANSCRIPT_STALL_MS = 8_000
/** N-12: the transcript's last row clears THE Dock's band by this much. */
const DOCK_GAP = 16
/** N-12: the jump circle sits this far above the band (EXP-1191: 12px). */
/** N-12: the relay's own waiting card is the newest ask in the Dock. */
const RELAY_ASK_ORDER = Number.MAX_SAFE_INTEGER

/** Prose scale — agent narration and the sender's own bubbles. Markdown
 *  bodies get the same size/leading from the `.agent-feed .tiptap-content`
 *  rule in styles.css. */
const TRANSCRIPT_BODY_TEXT = `text-[length:var(--transcript-body-size)] leading-[var(--transcript-body-line-height)]`

/** Tool scale — tool rows, group captions, permission and subagent rows, the
 *  compaction hairline and the "Working…" indicator. */
const TRANSCRIPT_TOOL_TEXT = `text-[length:var(--transcript-tool-size)] leading-[var(--transcript-tool-line-height)]`

/** EXP-1256: the top edge fade is a MASK on the scroller's own content,
 *  only once the body scrolled; the bottom one is `DETAIL_FADE_BOTTOM_CLASS`'s,
 *  restated here when both apply. */
function edgeFadeStyle({
  top,
  bottom,
}: {
  top: boolean
  bottom: boolean
}): React.CSSProperties | undefined {
  if (!top) return undefined
  const tail = bottom
    ? `black calc(100% - 48px), rgb(0 0 0 / 0.6) calc(100% - 24px), transparent`
    : `black`
  const mask = `linear-gradient(to bottom, transparent, black 24px, ${tail})`
  return { maskImage: mask, WebkitMaskImage: mask }
}

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
// reducer all live in lib/steer-session-store.ts — a module-level per-session
// store that OUTLIVES this view. This file only renders.

// ── The raw session view: structured activity feed over the relay ──────────

export function AgentSessionView({
  session,
  header,
  composer,
  composerFloats = false,
}: {
  /** The synced `sessions` row (any node of a tree). */
  session: SessionRow
  /** The route's header (back to the thread, the title, Stop). */
  header?: ReactNode
  /** The reply box under the feed (`SessionComposer`). */
  composer?: ReactNode
  /** EXP-1316: the phones' `FloatingComposer` — it floats over the feed
   *  (no in-flow slot), the feed reserves the capsule's clearance. */
  composerFloats?: boolean
}) {
  // EXP-621: the connection lives in a module-level per-session store that
  // outlives this view — mounting subscribes to the retained state and dials
  // only when nothing is connected yet.
  const store = useMemo(() => acquireSteerSession(session.id), [session.id])
  const { users: teamUsers } = useTeamUsers(session.teamId)
  const {
    phase,
    feed,
    compacting,
    config,
    turnState,
    turnStartedAt,
    turnTokens,
    workflows,
    runningWorkflow,
    answerStates,
    canLoadEarlier: snapshotCanLoadEarlier,
  } = useSyncExternalStore(store.subscribe, store.getSnapshot)
  /** EXP-724: the agent is folding its context. */
  const compactingNow = compacting !== null
  useEffect(() => store.connect(), [store])
  // The synced row is the truth for "still running" inside the redial loops.
  useEffect(
    () => store.noteSessionStatus(session.status),
    [store, session.status]
  )

  /** EXP-356: the selected conversation tab — `null` is the main agent; a
   *  subagent id focuses that agent's stream. */
  const [agentTab, setAgentTab] = useState<string | null>(null)

  const sendMessage = (text: string): boolean => store.sendMessage(text)
  const answerQuestion = (
    item: QuestionItem,
    keys: string[],
    labels: string[],
    text?: string
  ) => store.answerQuestion(item, keys, labels, text)

  // ── Follow-scroll: pinned to the newest event until the user scrolls up ───
  // (`useFollowScroll`, shared with the thread Conversation, EXP-1316).
  const follow = useFollowScroll([feed, phase.kind, session.needsInput, compactingNow])
  const { scrollRef, atBottom, jumpToBottom } = follow

  // Switching conversation tabs re-pins to the newest event (EXP-356).
  useEffect(() => {
    jumpToBottom()
  }, [agentTab, jumpToBottom])

  const live = phase.kind === `live`
  const sessionEnded = session.status === `ended`

  /** Identity-scoped questions stay answerable until they resolve. */
  const questionIds = useMemo(() => activeQuestionIds(feed), [feed])
  const canAnswer = live && !sessionEnded
  // EXP-783: the transcript keeps the WHOLE session, this view paints a
  // WINDOW over it (`windowFrom` = the oldest rendered row's id).
  const [windowFrom, setWindowFrom] = useState<number | null>(null)
  const windowStart = useMemo(() => {
    const tail = Math.max(0, feed.length - FEED_WINDOW)
    if (windowFrom === null) return tail
    const at = feed.findIndex((item) => item.id >= windowFrom)
    return Math.min(at === -1 ? tail : at, tail)
  }, [feed, windowFrom])
  /** Render rows: tool runs collapse, one ask's questions become a stepper,
   *  a subagent's work its own group — a projection only. */
  const rows = useMemo(
    () => groupFeedRows(feed, windowStart),
    [feed, windowStart]
  )
  /** THE ask dock (ask-dock.json ×4): the newest card still waiting on the
   *  viewer leaves the transcript and takes the composer's slot. */
  const dockedRow = useMemo(() => {
    if (!canAnswer) return null
    for (let i = rows.length - 1; i >= 0; i--) {
      const row = rows[i]!
      const items = row.kind === `ask` ? row.items : row.kind === `single` ? [row.item] : []
      if (items.some((item) => item.kind === `question` && questionIds.has(item.id))) return row
    }
    return null
  }, [rows, questionIds, canAnswer])
  /** EXP-1280: the cards a TOOL posted (`plan_propose` / `sessions_ask`)
   *  off the synced `messages` (`isToolPostedCard`): the relay holds no
   *  open ask for them, so they render as THE thread card and answer via
   *  `sessions.answer` only. A waiting one takes the ask dock when no relay
   *  card does; the rest sit after the call that posted them. */
  const sessionMessages = useSessionMessages(session.id)
  const toolCards = useMemo(() => toolCardRows(session, sessionMessages), [session, sessionMessages])
  const canAnswerCards = !sessionEnded
  const dockedToolCards = useMemo(
    () => (canAnswerCards ? toolCards.filter(cardWaiting) : []),
    [toolCards, canAnswerCards]
  )
  /** N-12: the relay's own waiting card (claude's AskUserQuestion or its
   *  plan) as a Dock source, in front of the tool-posted ones. */
  const relaySource = useMemo<DockSource | null>(() => {
    if (dockedRow === null) return null
    const items = dockedRow.kind === `ask` ? (dockedRow.items as QuestionItem[]) : dockedRow.kind === `single` ? [dockedRow.item as QuestionItem] : []
    const item = items.find((entry) => questionIds.has(entry.id)) ?? items[0]
    if (!item) return null
    const id = `relay:${dockedRow.kind === `single` ? dockedRow.item.id : dockedRow.id}`
    return item.planMode
      ? { kind: `plan`, id, text: item.text, sessionName: null, questions: 0, createdAt: RELAY_ASK_ORDER }
      : { kind: `question`, id, text: item.text, sessionName: null, createdAt: RELAY_ASK_ORDER }
  }, [dockedRow, questionIds])
  /** N-12: THE Dock's band height = the transcript's bottom padding. */
  const [dockHeight, setDockHeight] = useState(0)
  const sessionDock = useSessionDock(session)
  const dockSources = useMemo(
    () => (relaySource ? [relaySource, ...sessionDock.sources] : sessionDock.sources),
    [relaySource, sessionDock.sources]
  )
  const renderDockSheet = (row: DockRow, close: () => void): ReactNode => {
    if (relaySource && `id` in relaySource && row.id === relaySource.id && dockedRow !== null) {
      return (
        <DockSheetBody>
          {dockedRow.kind === `ask` ? (
            <AskStepperCard
              items={dockedRow.items as QuestionItem[]}
              activeIds={questionIds}
              canAnswer={canAnswer}
              answerStates={answerStates}
              onAnswer={answerQuestion}
              pendingId={pendingCard?.id ?? null}
              onSend={sendMessage}
              chromeless
            />
          ) : dockedRow.kind === `single` ? (
            <QuestionCard
              item={dockedRow.item as QuestionItem}
              active={questionIds.has(dockedRow.item.id)}
              canAnswer={canAnswer}
              answerState={answerStates[answerKey(dockedRow.item as QuestionItem)]}
              onAnswer={answerQuestion}
              hotkeys={pendingCard?.id === dockedRow.item.id}
              onSend={sendMessage}
              chromeless
            />
          ) : null}
        </DockSheetBody>
      )
    }
    return sessionDock.renderSheet(row, close)
  }
  const { answer: answerCard, pending: cardsPending } = useAnswerMessage()
  // Thread 58cb32df (chats.json SURFACE CARD): an agent's inline preview
  // (`session_self_show`) sits right after the call that posted it, like a
  // tool-posted card; a `reply` press sends its text as the next message.
  const surfaceCards = useMemo(() => surfaceCardRows(session, sessionMessages), [session, sessionMessages])
  // EXP-1398 (ask-dock.json `anchorCases`): every card after the call that
  // posted it, paired per kind; the rest at the head.
  const cardAnchors = useMemo(
    () => anchorSessionCards(feed, sessionCards(sessionMessages.map(toTreeMessage))),
    [feed, sessionMessages]
  )
  const surfaceArrived = useLiveArrivals(surfaceCards.map((row) => row.id))
  const replyFromSurface = (text: string) => {
    trpc.sessions.send
      .mutate({ teamId: session.teamId, sessionId: session.id, text }, SKIP_ERROR_TOAST)
      .catch((error) => failedAction(`send`, `the message`, error))
  }
  // FEED-140: a waiting tool-posted card STAYS inline (the dock holds it too).
  const renderToolCards = (ids: readonly string[], keyPrefix: string): ReactNode =>
    ids
      .map((id) => {
        const row = toolCards.find((r) => r.id === id) ?? surfaceCards.find((r) => r.id === id)
        if (!row) return null
        return (
          <div key={`${keyPrefix}-${id}`} className={TRANSCRIPT_GAP_CLASS.gapBlock} data-testid="session-tool-card">
            <ThreadConversation
              rows={[row]}
              renderText={renderThreadText}
              attachmentSrc={attachmentUrl}
              onAnswer={canAnswerCards ? (rowId, input) => void answerCard(rowId, input) : undefined}
              onDecide={canAnswerCards ? (rowId, input) => void answerCard(rowId, input) : undefined}
              pending={cardsPending}
              onSurfaceReply={sessionEnded ? undefined : replyFromSurface}
              arrived={surfaceArrived}
            />
          </div>
        )
      })
  /** EXP-895/1206: the ONE row still RUNNING (`liveToolRowId` ×4). */
  const liveRowId = useMemo(
    () => (live ? liveToolRowId(feed) : undefined),
    [live, feed]
  )
  const canLoadEarlier = windowStart > 0 || snapshotCanLoadEarlier
  /** Pull the next page in, anchored to the distance from the bottom. */
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
  // At the newest rows the window SLIDES with the stream; scrolled up, it
  // is PINNED.
  useEffect(() => {
    if (atBottom) setWindowFrom(null)
  }, [atBottom])
  /** EXP-356/387: one conversation tab per still-running subagent. */
  const agents = useMemo(() => collectSubagents(feed), [feed])
  const visibleTabs = useMemo(
    () => visibleSubagentTabs(agents, agentTab),
    [agents, agentTab]
  )
  const activeAgent =
    agentTab !== null && visibleTabs.some((a) => a.subagentId === agentTab)
      ? agentTab
      : null
  const agentItems = useMemo(
    () =>
      activeAgent === null
        ? []
        : feed.filter((item) => subagentIdOf(item) === activeAgent),
    [feed, activeAgent]
  )
  /** A trailing question/plan means the session waits on a person. */
  const awaitingInput = live && questionIds.size > 0
  /** EXP-788/820: the card the keyboard answers. */
  const pendingCard = useMemo(
    () =>
      live && canAnswer
        ? pendingAnswerable(feed, questionIds, answerStates)
        : null,
    [live, canAnswer, feed, questionIds, answerStates]
  )
  /** EXP-724/746: the slash commands this session's agent can run, so a
   *  steered `/<command>` renders as a command row, not prose. NEW-2: the
   *  contract builtins only; the CLI's injected commands never count. */
  const catalogAgent = steerAgentId(session.cli, config !== null)
  const agentCommands = useMemo(
    () => steerCommandsFor(catalogAgent),
    [catalogAgent]
  )
  /** EXP-389/848: the agent is EXECUTING a turn. */
  const working = sessionIsWorking({
    live,
    sessionEnded,
    turnState,
    awaitingInput,
    needsInput: session.needsInput,
    blocked: session.blocked != null,
    compacting: compactingNow,
  })
  /** EXP-549/550: the host machine per the synced devices row. */
  const device = useSessionDevice({
    deviceId: session.deviceId,
    userId: session.userId,
  })
  const isMobile = useIsMobile()
  /** EXP-550: no live stream AND the host machine is offline — PAUSED on
   *  that machine, not starting and not gone. */
  const paused =
    device.online === false &&
    !sessionEnded &&
    (phase.kind === `starting` ||
      phase.kind === `connecting` ||
      phase.kind === `idle` ||
      phase.kind === `closed`)
  const deviceName = device.label || `Desktop`
  const pausedTitle = `${device.label ?? `The device`} is offline`
  const pausedBody = `The agent is paused on that machine and continues when it comes back online.`
  // EXP-773: the history phases name the machine the transcript lives on.
  useEffect(
    () => store.noteDeviceLabel(device.label),
    [store, device.label]
  )
  // A `closed` phase does not redial on its own: nudge the store once the
  // device flips back online (EXP-625 `kick`).
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

  /** EXP-850 §3: the subagents that belong to a workflow card. */
  const workflowAgents = useMemo(() => workflowSubagentIds(feed), [feed])
  /** EXP-1321/1324: the Queue's two folds; the reader's toggles stick for
  /** §4: the duplicate warnings a workflow card carries, by workflow id. */
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
  /** EXP-850 §3: cards whose `Workflow` tool row this window does not hold
   *  land at the TAIL of the transcript. */
  const orphanWorkflows = useMemo(
    () =>
      orphanWorkflowIds(rows, workflows)
        .map((id) => workflows.get(id))
        .filter((workflow): workflow is WorkflowState => workflow !== undefined),
    [rows, workflows]
  )

  /** N-11: before the run's first turn (the session card's `queued`, not
   *  paused) the empty column reads "Starting on <device>"; there is no
   *  status row over the feed, the live row at its bottom is the working
   *  signal (needs-you = the Dock's card, blocked/paused = Dock banners). */
  const starting =
    !paused &&
    !awaitingInput &&
    !working &&
    sessionState(toTreeSession(session)) === `queued`
  const startingCaption = STARTING_COPY.label.replace(`{device}`, deviceName)

  const ownerName =
    teamUsers.find((user) => user.id === session.userId)?.name ?? null
  const runCards = useMemo(
    () => ({ agent: session.cli ?? null, owner: ownerName, users: teamUsers }),
    [session.cli, ownerName, teamUsers]
  )

  /** One transcript row; `list` is the sequence it sits in (its gap reads
   *  the row before it). */
  const renderTranscriptRow = (
    row: (typeof rows)[number],
    index: number,
    list: readonly (typeof rows)[number][]
  ): ReactNode => {
    // FEED-140 (chats.json): the docked card STAYS inline at its place with
    // its actions (the dock holds it too); only the keyboard is the dock's.
    const docked = row === dockedRow
    // EXP-787: each row carries the space ABOVE it, from the row before it.
    const gap = transcriptGapClass(
      index === 0 ? null : rowClass(list[index - 1]),
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
    // EXP-916: a run of consecutive edits IS the edited-files card.
    if (row.kind === `edits`) {
      return wrap(
        <EditsCardRow
          items={row.items as Extract<FeedItem, { kind: `tool` }>[]}
          liveRowId={liveRowId ?? null}
        />
      )
    }
    // EXP-948: a run of the SAME Exponential tool is its own captioned row.
    if (row.kind === `expRun`) {
      return wrap(
        <ExpToolGroupRow
          items={row.items as Extract<FeedItem, { kind: `tool` }>[]}
        />
      )
    }
    if (row.kind === `toolRun`) {
      return wrap(
        <ToolGroupRow
          items={row.items as Extract<FeedItem, { kind: `tool` }>[]}
          liveTail={
            liveRowId !== undefined &&
            row.items[row.items.length - 1]?.id === liveRowId
          }
        />
      )
    }
    if (row.kind === `subagent`) {
      // EXP-850 §3/EXP-856: a workflow's agents live INSIDE its card, but
      // only when this client HOLDS that card.
      if (
        nestedWorkflowId(
          { subagentId: row.subagentId },
          workflowAgents,
          workflows
        )
      ) {
        return null
      }
      return wrap(
        <SubagentLineRow
          items={row.items}
          live={live && !sessionEnded}
          onOpen={() => setAgentTab(row.subagentId)}
        />
      )
    }
    if (row.kind === `ask`) {
      return wrap(
        <AskStepperCard
          items={row.items as QuestionItem[]}
          activeIds={questionIds}
          canAnswer={canAnswer}
          answerStates={answerStates}
          onAnswer={answerQuestion}
          pendingId={docked ? null : (pendingCard?.id ?? null)}
          onSend={sendMessage}
        />
      )
    }
    const item = row.item
    switch (item.kind) {
      case `narration`:
        // Thread 58cb32df (chats.json TRANSCRIPT): phones paint the agent's
        // text as THE chat bubble (left, bg-card, its time bottom-right).
        return wrap(
          isMobile ? (
            <ChatBubble>
              <div className={TRANSCRIPT_BODY_TEXT}>
                <FeedText text={item.text} ariaLabel="Agent message" hardBreaks />
              </div>
            </ChatBubble>
          ) : (
            <NarrationBubble text={item.text} />
          )
        )
      case `thought`:
        // EXP-1324: the reasoning line; still open only while the run lives.
        return wrap(
          <ThoughtRow
            item={item}
            running={!item.settled && live && !sessionEnded}
          />
        )
      case `tool`: {
        // EXP-850 §3: the `Workflow` call renders as its CARD.
        const workflow = item.workflowId
          ? workflows.get(item.workflowId)
          : undefined
        if (workflow) {
          return wrap(
            <WorkflowCardRow
              workflow={workflow}
              duplicates={workflowDuplicates.get(workflow.id)}
              onOpenAgent={setAgentTab}
            />
          )
        }
        // EXP-916: a call that CARRIES a patch draws a one-member card.
        if (item.diff) {
          return wrap(
            <EditsCardRow items={[item]} liveRowId={liveRowId ?? null} />
          )
        }
        return wrap(<ToolRow item={item} flush live={item.id === liveRowId} />)
      }
      case `user_message`: {
        // Thread 58cb32df (chats.json TRANSCRIPT): `/new` and `/compact`
        // are THE divider pill, never a bubble.
        const divider = commandDivider(item.text)
        if (divider) {
          return wrap(
            <TranscriptDivider
              kind={divider}
              at={item.at ? new Date(item.at).toISOString() : null}
            />
          )
        }
        // EXP-724: a steered slash command renders as a compact pill.
        const command = parseSteerCommand(item.text, agentCommands)
        return wrap(
          command ? (
            <CommandRow name={command.command.name} args={command.args} />
          ) : (
            <UserMessageBubble text={item.text} />
          )
        )
      }
      case `compaction`:
        return wrap(<CompactionRow />)
      case `api_error`:
        return wrap(
          <ApiErrorRow message={item.message} errorType={item.errorType} />
        )
      case `permission`:
        return wrap(
          <PermissionRow
            tool={item.tool}
            detail={item.detail}
            active={live && item.id === feed[feed.length - 1]?.id}
          />
        )
      case `subagent`:
        if (nestedWorkflowId(item, workflowAgents, workflows)) return null
        return wrap(
          <SubagentLineRow
            items={[item]}
            live={live && !sessionEnded}
            onOpen={() => setAgentTab(item.subagentId)}
          />
        )
      case `question`:
        return wrap(
          <QuestionCard
            item={item}
            active={questionIds.has(item.id)}
            canAnswer={canAnswer}
            answerState={answerStates[answerKey(item)]}
            onAnswer={answerQuestion}
            hotkeys={!docked && pendingCard?.id === item.id}
            onSend={sendMessage}
          />
        )
    }
    return null
  }

  const feedPlaceholder = feed.length === 0
  // NEW-6 (N-1): a session waiting on input with no live room shows no
  // endless placeholders: once the connect has waited long enough while the
  // row is not busy, the column says the transcript is not reachable.
  const waitingForRoom =
    feedPlaceholder &&
    !session.busy &&
    session.status !== `ended` &&
    (phase.kind === `connecting` || phase.kind === `starting` || phase.kind === `history_pending`)
  const [stalled, setStalled] = useState(false)
  useEffect(() => {
    if (!waitingForRoom) {
      setStalled(false)
      return
    }
    const timer = window.setTimeout(() => setStalled(true), TRANSCRIPT_STALL_MS)
    return () => window.clearTimeout(timer)
  }, [waitingForRoom])
  /** The transcript STREAMS: a live publisher mid-turn (`TranscriptStreamingContext`). */
  const streaming = live && !sessionEnded && turnState === `started`

  return (
    <SessionResultsRawContext.Provider value={session.guide}>
    <RunCardsContext.Provider value={runCards}>
    <TranscriptStreamingContext.Provider value={streaming}>
    <div className="flex h-full min-h-0 flex-col" data-testid="agent-session-view">
      {header}
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card/40">
        {/* EXP-356: conversation tabs — Main plus one per RUNNING subagent. */}
        {visibleTabs.length > 0 && (
          <div className="shrink-0 py-1" data-testid="session-agent-tabs">
            <div
              className={cn(
                TRANSCRIPT_COLUMN,
                `flex items-center gap-1 overflow-x-auto`
              )}
            >
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
          </div>
        )}
        {/* The activity feed (bottom-anchored, follow-scroll) */}
        <div className="relative min-h-0 flex-1">
          <div
            ref={scrollRef}
            onScroll={follow.onScroll}
            // `agent-feed`: the hook the inline-code tint keys on (EXP-698).
            className={cn(
              `agent-feed h-full overflow-y-auto overscroll-contain`,
              !isMobile && DETAIL_FADE_BOTTOM_CLASS
            )}
            style={edgeFadeStyle({ top: follow.scrolled, bottom: !isMobile })}
          >
            {feedPlaceholder && paused ? (
              <CenteredState>
                <EmptyState
                  icon={UiDeviceOfflineIcon}
                  title={pausedTitle}
                  description={pausedBody}
                />
              </CenteredState>
            ) : feedPlaceholder && starting ? (
              /* N-11: no status row over the feed; before the first turn the
                 column holds the placeholder rows over "Starting on …". */
              <div
                className={cn(TRANSCRIPT_COLUMN, `flex flex-col gap-2 py-4`)}
                data-testid="transcript-placeholder"
              >
                <SkeletonRows rowHeight={36} className="-mx-3" />
                <span className="text-xs text-muted-foreground">{startingCaption}</span>
              </div>
            ) : waitingForRoom && stalled ? (
              <CenteredState>
                <EmptyState
                  title="No live transcript"
                  description="The device holding this session isn't publishing right now. Its conversation stays in the chat."
                />
              </CenteredState>
            ) : feedPlaceholder &&
              (phase.kind === `connecting` ||
                phase.kind === `starting` ||
                phase.kind === `history_pending`) ? (
              /* EXP-1259 D8.3: placeholder rows in the column, the phase as
                 a muted caption. */
              <div
                className={cn(TRANSCRIPT_COLUMN, `flex flex-col gap-2 py-4`)}
                data-testid="transcript-placeholder"
              >
                <SkeletonRows rowHeight={36} className="-mx-3" />
                <span className="text-xs text-muted-foreground">
                  {phase.kind === `history_pending`
                    ? (phase.detail ?? `Fetching the transcript…`)
                    : phase.kind === `starting`
                      ? `The agent is starting. Waiting for the live stream…`
                      : `Connecting…`}
                </span>
              </div>
            ) : feedPlaceholder && live ? (
              <CenteredState>
                <EmptyState
                  title="Waiting for activity"
                  description="This session isn't publishing an activity feed yet."
                />
              </CenteredState>
            ) : activeAgent !== null ? (
              <div
                ref={follow.contentRef}
                className={cn(`flex min-h-full flex-col justify-end py-2`, TRANSCRIPT_COLUMN)}
                style={{ paddingBottom: dockHeight + DOCK_GAP }}
              >
                <AgentConversation
                  summary={agents.find((a) => a.subagentId === activeAgent)}
                  items={agentItems}
                  live={live && !sessionEnded}
                />
              </div>
            ) : (
              <div
                ref={follow.contentRef}
                className={cn(`flex min-h-full flex-col justify-end py-2`, TRANSCRIPT_COLUMN)}
                style={{ paddingBottom: dockHeight + DOCK_GAP }}
              >
                {canLoadEarlier ? <LoadEarlierButton onClick={loadEarlier} /> : null}
                {renderToolCards(cardAnchors.unanchored, `head`)}
                {rows.map((row, index) => (
                  <Fragment key={row.kind === `single` ? `row-${row.item.id}` : `row-${row.id}`}>
                    {renderTranscriptRow(row, index, rows)}
                    {renderToolCards(
                      rowItemIds(row).flatMap((id) => cardAnchors.anchors.get(id) ?? []),
                      `after`
                    )}
                  </Fragment>
                ))}
                {/* EXP-850 §3: a card whose `Workflow` row is outside the
                    window lands at the tail (×4). */}
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
                    <WorkflowCardRow
                      workflow={workflow}
                      duplicates={workflowDuplicates.get(workflow.id)}
                      onOpenAgent={setAgentTab}
                    />
                  </div>
                ))}
                {/* EXP-389: the agent-is-busy footer under the newest event. */}
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
                      agent={session.cli}
                      startedAt={turnStartedAt}
                      tokens={turnTokens}
                      workflow={runningWorkflow}
                    />
                  </div>
                )}
              </div>
            )}
          </div>
          {/* EXP-1191: the arrow-only circle ×4, 12px above the composer. */}
          <JumpToBottomButton
            visible={!atBottom && feed.length > 0}
            className="absolute left-1/2 z-30 -translate-x-1/2"
            style={{ bottom: dockHeight + JUMP_TO_BOTTOM_GAP }}
            onClick={jumpToBottom}
          />
          {/* N-12: THE Dock over the transcript's bottom edge: the waiting
              cards (the relay's, the tool-posted ones), THE Queue, the
              queued messages and the status banners, then the composer (a
              phone's floats in the work bar instead). */}
          <ThreadDock
            rows={dockedToolCards}
            sessionName={noSessionName}
            renderText={renderThreadText}
            attachmentSrc={attachmentUrl}
            pending={cardsPending}
            onAnswer={(rowId, input) => void answerCard(rowId, input)}
            onDecide={(rowId, input) => void answerCard(rowId, input)}
            sources={dockSources}
            renderSheet={renderDockSheet}
            renderAction={sessionDock.renderAction}
            composer={composerFloats ? undefined : composer}
            onHeight={setDockHeight}
            phoneClearance={composerFloats}
            autoOpenReady={live}
          />
        </div>

        {composer && composerFloats ? composer : null}
      </div>
    </div>
    </TranscriptStreamingContext.Provider>
    </RunCardsContext.Provider>
    </SessionResultsRawContext.Provider>
  )
}

/** EXP-1280: a tool-posted card's pictures resolve against the app. */
const attachmentUrl = (attachmentId: string) => `/api/attachments/${attachmentId}`
const noSessionName = () => null

/** A card still waiting on a person. */
function cardWaiting(row: ThreadRow): boolean {
  // FEED-140: a `superseded` plan is decided (never in the dock).
  if (row.type === `plan`) return row.decision === null
  return row.type === `question` && !row.answered
}

/** EXP-1280: this session's TOOL-posted cards (ask-dock.json
 *  `sessionCards`) in THE thread row form, oldest first. */
function toolCardRows(session: SessionRow, messages: readonly MessageRowLike[]): ThreadRow[] {
  const ids = new Set(
    sessionCards(messages.map(toTreeMessage))
      .filter((card) => card.kind !== `surface`)
      .map((card) => card.id)
  )
  const cards = messages.filter((message) => ids.has(message.id))
  if (cards.length === 0) return []
  return buildSessionThread({
    root: toTreeSession(session),
    sessions: [],
    sessionIssues: [],
    issues: [],
    messages: cards.map(toTreeMessage),
    pullRequests: [],
    now: new Date().toISOString(),
  }).rows.filter((row) => row.type === `plan` || row.type === `question`)
}

type MessageRowLike = Parameters<typeof toTreeMessage>[0]

/** Thread 58cb32df: this session's `surface` cards as THE thread rows. */
function surfaceCardRows(session: SessionRow, messages: readonly MessageRowLike[]): ThreadRow[] {
  const cards = messages.filter((message) => (message.card as { kind?: unknown } | null)?.kind === `surface`)
  if (cards.length === 0) return []
  return buildSessionThread({
    root: toTreeSession(session),
    sessions: [],
    sessionIssues: [],
    issues: [],
    messages: cards.map(toTreeMessage),
    pullRequests: [],
    now: new Date().toISOString(),
  }).rows.filter((row) => row.type === `surface`)
}

/** The feed item ids a transcript row draws. */
function rowItemIds(row: { kind: string; item?: { id: number }; items?: readonly { id: number }[] }): number[] {
  if (row.item) return [row.item.id]
  return row.items ? row.items.map((item) => item.id) : []
}

/** EXP-1323 (#EXP-1333): a THREAD's live tail ×4 — while the root's session
 *  works the Conversation ends on ONE status line (the "Working…" row).
 *  N-12: THE Queue left the transcript for the Dock (`useSessionDock`).
 *  Same relay store as the raw view (one connection per session, shared). */
export function SessionLiveTail({ session }: { session: SessionRow }) {
  const store = useMemo(() => acquireSteerSession(session.id), [session.id])
  const {
    phase,
    feed,
    compacting,
    turnState,
    turnStartedAt,
    turnTokens,
    runningWorkflow,
  } = useSyncExternalStore(store.subscribe, store.getSnapshot)
  useEffect(() => store.connect(), [store])
  useEffect(() => store.noteSessionStatus(session.status), [store, session.status])
  const live = phase.kind === `live`
  const questionIds = useMemo(() => activeQuestionIds(feed), [feed])
  const working = sessionIsWorking({
    live,
    sessionEnded: session.status === `ended`,
    turnState,
    awaitingInput: live && questionIds.size > 0,
    needsInput: session.needsInput,
    blocked: session.blocked != null,
    compacting: compacting !== null,
  })
  if (!working) return null
  return (
    <div data-testid="thread-live-tail" className="flex flex-col gap-2">
      {working && (
        <WorkingIndicatorRow
          agent={session.cli}
          startedAt={turnStartedAt}
          tokens={turnTokens}
          workflow={runningWorkflow}
        />
      )}
    </div>
  )
}

// ── Pieces ───────────────────────────────────────────────────────────────────

// Vertically centres the feed's placeholder (an `EmptyState`, or the
// connecting spinner, which is not an empty state) in the scroller.
function CenteredState({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      {children}
    </div>
  )
}

/** The trailing "agent is busy" row (EXP-389, rewritten by EXP-850 §5): the
 *  turn's verb, its clock and the tokens it has produced —
 *  `Pondering… (2m 04s · ↓ 12.4k tokens)` — beside the RUNNING AGENT's
 *  working mark (EXP-1191: Claude's spark, the sidebar's; others pulse). While a workflow runs the text is that workflow's caption
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
      <AgentWorkingMark agent={agent} />
      <span className="min-w-0 truncate text-muted-foreground">{caption}</span>
    </div>
  )
}

/** One Trace line off a pure model (`@exp/domain-contract/trace`): the
 *  concept icon, the title (shimmering while running), the muted
 *  description. `trailing` rides after it (a chevron, a badge). */
function TraceModelLine({
  model,
  description,
  trailing,
  tooltip,
}: {
  model: TraceLineModel
  /** Overrides the model's description slot (the subagent's badge). */
  description?: ReactNode
  trailing?: ReactNode
  tooltip?: string
}) {
  const Icon = conceptIcon(model.icon as Parameters<typeof conceptIcon>[0])
  return (
    <TraceLine status={model.status}>
      <TraceIcon>
        {/* The slot's own size, whatever button wraps the line. */}
        <Icon className="size-full" />
      </TraceIcon>
      <TraceTitle title={tooltip ?? model.title}>{model.title}</TraceTitle>
      {description ??
        (model.description ? (
          <TraceDescription>{model.description}</TraceDescription>
        ) : null)}
      {trailing}
    </TraceLine>
  )
}

/** EXP-1321/1324: THE Queue, the ONE block at the bottom of the transcript
 *  (`taskQueue`, fixture `task-queue.json` ×4): "n to do" (open by default)
 *  lists the running work — shimmering — then the pending tasks, muted;
 *  "n completed" stays folded. It replaces the background-task / "Waiting
 *  on" lines and the task-list Work card; a running background task's
 *  elapsed time ticks once a second. Absent when nothing is left to do. */
function TaskQueueStrip({
  taskList,
  backgroundTasks,
  feed,
  todoOpen,
  onTodoOpenChange,
  completedOpen,
  onCompletedOpenChange,
  plain = false,
}: {
  taskList: TaskListEntry[]
  backgroundTasks: BackgroundTask[]
  feed: readonly FeedItem[]
  todoOpen: boolean
  onTodoOpenChange: (open: boolean) => void
  completedOpen: boolean
  onCompletedOpenChange: (open: boolean) => void
  /** N-12: inside THE Dock's sheet (no column, no cap of its own). */
  plain?: boolean
}) {
  const ticking = backgroundTasks.some((task) => task.kind !== `agent`)
  const now = useNow(ticking ? 1_000 : 60_000)
  const model = useMemo(
    () =>
      sessionTaskQueue({ taskList, backgroundTasks, feed, now: now.getTime() }),
    [taskList, backgroundTasks, feed, now]
  )
  if (model === null) return null
  const sections = [
    {
      section: model.todo,
      open: todoOpen,
      onOpenChange: onTodoOpenChange,
      testId: TASK_QUEUE_TEST_IDS.todo,
    },
    ...(model.completed
      ? [
          {
            section: model.completed,
            open: completedOpen,
            onOpenChange: onCompletedOpenChange,
            testId: TASK_QUEUE_TEST_IDS.completed,
          },
        ]
      : []),
  ]
  return (
    <div
      className={plain ? undefined : `max-h-[40vh] overflow-y-auto py-1.5`}
      data-testid={TASK_QUEUE_TEST_IDS.queue}
    >
      <div className={plain ? undefined : TRANSCRIPT_COLUMN}>
        <Trace size="sm">
          {sections.map(({ section, open, onOpenChange, testId }) => {
            const Icon = conceptIcon(section.icon as Parameters<typeof conceptIcon>[0])
            return (
              <TraceCollapsible
                key={testId}
                open={open}
                onOpenChange={onOpenChange}
                data-testid={testId}
              >
                <TraceCollapsibleTrigger>
                  <TraceLine>
                    <TraceIcon>
                      <Icon />
                    </TraceIcon>
                    <TraceTitle>{section.title}</TraceTitle>
                  </TraceLine>
                </TraceCollapsibleTrigger>
                <TraceCollapsibleContent>
                  <div className="flex flex-col gap-1.5">
                    {section.items.map((item) => {
                      const ItemIcon = conceptIcon(
                        item.icon as Parameters<typeof conceptIcon>[0]
                      )
                      return (
                        <TraceLine
                          key={item.key}
                          status={
                            item.state === `running`
                              ? `running`
                              : item.state === `pending` || item.state === `done`
                                ? `pending`
                                : null
                          }
                        >
                          <TraceIcon>
                            <ItemIcon />
                          </TraceIcon>
                          <TraceTitle title={item.label}>{item.label}</TraceTitle>
                          {item.description && (
                            <TraceDescription>{item.description}</TraceDescription>
                          )}
                        </TraceLine>
                      )
                    })}
                  </div>
                </TraceCollapsibleContent>
              </TraceCollapsible>
            )
          })}
        </Trace>
      </div>
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
  plain = false,
}: {
  messages: QueuedMessage[]
  onRemove: (entry: QueuedMessage) => void
  /** N-12: inside THE Dock's sheet (no column). */
  plain?: boolean
}) {
  if (messages.length === 0) return null
  return (
    <div
      className={plain ? undefined : `py-1.5`}
      aria-label={QUEUE_STRIP_TITLE}
      data-testid="session-queue-strip"
    >
      {/* EXP-927: the transcript's reading column, like the strip above. */}
      <div className={cn(plain ? undefined : TRANSCRIPT_COLUMN, `flex flex-col gap-0.5`)}>
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
    </div>
  )
}

/** EXP-916: ONE lane's items — a subagent's fold or a subagent's own
 *  conversation TAB (EXP-1225: a workflow agent's too) — through the same
 *  projection the main transcript runs (`groupLaneRows`), so an edit run
 *  reads as the card it is everywhere else. The ONE row switch a lane has: a
 *  tab used to carry a copy of it.
 *
 *  `gaps` is the only difference between the two readings. A conversation tab
 *  IS a transcript, so its rows take the gap ladder (EXP-787); folded inside a
 *  group row the lane keeps the tighter uniform rhythm, with its tool rows
 *  flush against each other. */
function LaneRows({
  items,
  gaps = false,
  live = false,
}: {
  items: FeedItem[]
  gaps?: boolean
  /** The lane's agent is still running: its newest unsettled call is live. */
  live?: boolean
}) {
  const rows = useMemo(() => groupLaneRows(items), [items])
  const liveRowId = live ? liveToolRowId(items) : undefined
  return (
    <>
      {rows.map((row, index) => {
        const gap = gaps
          ? transcriptGapClass(
              index === 0 ? null : rowClass(rows[index - 1]),
              rowClass(row)
            )
          : `py-0.5`
        if (row.kind === `edits`) {
          return (
            <div key={row.id} className={gap}>
              <EditsCardRow items={row.items as ToolItem[]} liveRowId={liveRowId ?? null} />
            </div>
          )
        }
        // EXP-948: a lane folds a run of OUR tools into the same captioned
        // row the transcript draws.
        if (row.kind === `expRun`) {
          return (
            <div key={row.id} className={gap}>
              <ExpToolGroupRow items={row.items as ToolItem[]} />
            </div>
          )
        }
        // EXP-1324: a tab is a transcript — its runs are lines `lineGap`
        // apart, never folded (the tab IS the agent's own detail).
        if (row.kind === `toolRun`) {
          return (
            <div key={row.id} className={cn(gap, `flex flex-col gap-1.5`)}>
              {(row.items as ToolItem[]).map((tool) => (
                <ToolRow key={tool.id} item={tool} live={tool.id === liveRowId} />
              ))}
            </div>
          )
        }
        if (row.kind === `subagent` || row.kind === `ask`) return null
        const item = row.item
        if (item.kind === `tool`) {
          return (
            <div key={item.id} className={gap}>
              <ToolRow item={item} live={item.id === liveRowId} />
            </div>
          )
        }
        if (item.kind === `thought`) {
          return (
            <div key={item.id} className={gap}>
              <ThoughtRow item={item} running={live && !item.settled} />
            </div>
          )
        }
        if (item.kind === `narration`) {
          return (
            <div key={item.id} className={gap}>
              <NarrationBubble text={item.text} />
            </div>
          )
        }
        if (item.kind === `user_message`) {
          return (
            <div key={item.id} className={gap}>
              <UserMessageBubble text={item.text} />
            </div>
          )
        }
        if (item.kind === `api_error`) {
          return (
            <div key={item.id} className={gap}>
              <ApiErrorRow message={item.message} errorType={item.errorType} />
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

// memo: a live feed re-renders on every incoming frame, and a bubble that
// rendered markdown owns a TipTap editor — re-running that for unchanged text
// is the one cost worth avoiding here.
export const NarrationBubble = memo(function NarrationBubble({
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
    <Button
      variant="text"
      size="inline"
      className="mt-1 font-medium"
      onClick={onToggle}
    >
      {expanded ? `Show less` : `Show more`}
    </Button>
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
  const { text: body, attachmentIds, files, markers } = parseSteerMessage(text)
  const hasImages = attachmentIds.length > 0
  // Wave D: file lines are peeled off like the embeds and drawn as the
  // comment thread's file chips, never as broken images or raw links.
  const hasAttachments = hasImages || files.length > 0
  // Clamp the PROSE, never the wire message: four embed lines are four more
  // "lines" of nothing, and they used to push a two-line steer into the fold
  // — which then hid the images the fold was measuring.
  const { expanded, setExpanded, clampable } = useClampToggle(
    hasAttachments ? body : text
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
          {!hasAttachments ? (
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
              <AttachmentThumb
                key={id}
                size="inline"
                src={`/api/attachments/${id}`}
                alt={`Image ${i + 1}`}
                openLabel={`Open image ${i + 1}`}
                onOpen={() => setPreview(i)}
              />
            ))}
          </div>
        )}
        {files.length > 0 && (
          <div
            className={cn(
              `flex flex-wrap gap-2`,
              (body || hasImages) && `mt-2`
            )}
          >
            {files.map((file) => (
              <AttachmentFileLink
                key={file.id}
                href={`/api/attachments/${file.id}`}
                name={file.name}
              />
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

  // EXP-1316: THE card's prompt (`@exp/ui` AskPrompt, shared with the
  // thread conversation); a plan reject's typed feedback follows as the next
  // message (`freeAnswerFor`).
  const answer = (keys: string[], labels: string[], text?: string) => {
    if (item.planMode && text !== undefined) {
      const free = freeAnswerFor(item, text)
      if (!free) return
      onAnswer(item, free.keys, free.labels, free.text)
      if (free.followUp !== undefined) onSend?.(free.followUp)
    } else {
      onAnswer(item, keys, labels, text)
    }
    onAnswered?.()
  }

  return (
    <AskPrompt
      options={item.options}
      multiSelect={item.multiSelect}
      planMode={item.planMode}
      variant={variant}
      answerable={answerable}
      locked={locked ? { labels: answerState?.labels ?? [] } : null}
      resolved={
        item.resolved === true && !editing
          ? { answer: item.answer, dismissed: item.dismissed === true }
          : null
      }
      initialPicked={editing ? recordedKeys(item) : undefined}
      hotkeys={hotkeys}
      onAnswer={answer}
    >
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
    </AskPrompt>
  )
}

/** EXP-820: the keys behind a resolved step's answer — the options whose
 *  labels the resolution lists — so revisiting the step starts from them. */
function recordedKeys(item: QuestionItem): string[] {
  if (!item.answer) return []
  const labels = new Set(item.answer.split(`, `))
  return item.options.filter((o) => labels.has(o.label)).map((o) => o.key)
}

/** A synced timestamp as the ISO string `agentInstructionsChanged` reads. */
function stampOf(value: Date | string | null | undefined): string | null {
  if (!value) return null
  return typeof value === `string` ? value : value.toISOString()
}

/** #EXP-1383 (chats.json `agentHint`): the "Instructions changed" banner
 *  of an agent chat's ROOT that still runs on instructions older than the
 *  agent's current ones (`agentInstructionsChanged`), as a Dock source, and
 *  its action: the agent menu's New conversation (`sendAgentLine` with
 *  `AGENT_MENU_SENDS.newConversation`). null source = no hint. */
export function useAgentHintDock(session: SessionRow | null): {
  source: DockSource | null
  renderAction: (row: DockRow) => ReactNode
} {
  const agentChatId = session && session.parentId === null ? session.agentId : null
  const { agent } = useAgentRow(agentChatId)
  const changed =
    session !== null &&
    agent !== null &&
    agentInstructionsChanged({
      sessionStamp: stampOf(session.agentInstructionsAt),
      agentStamp: stampOf(agent.instructionsUpdatedAt),
    })
  const source = useMemo<DockSource | null>(
    () =>
      changed
        ? {
            kind: `banner`,
            id: AGENT_HINT_BANNER.id,
            icon: AGENT_HINT_BANNER.icon,
            label: AGENT_HINT_BANNER.label,
            text: AGENT_HINT_BANNER.text,
            tone: AGENT_HINT_BANNER.tone as `muted`,
            action: AGENT_HINT_BANNER.action,
          }
        : null,
    [changed]
  )
  const renderAction = (row: DockRow): ReactNode => {
    const text = AGENT_MENU_SENDS.newConversation
    if (row.id !== `banner:${AGENT_HINT_BANNER.id}` || !session || !agent || !text) return null
    return (
      <Button
        variant="outline"
        size="sm"
        className="h-6"
        data-testid="dock-instructions-new"
        onClick={() =>
          void sendAgentLine({ teamId: session.teamId, agentId: agent.id, conversationId: session.id, text })
        }
      >
        {row.action}
      </Button>
    )
  }
  return { source, renderAction }
}

/** N-12: the session's Dock beyond the asks: THE Queue (task-queue.json),
 *  the queued messages (EXP-861, the X revokes a held one) and the status
 *  banners (EXP-550 paused on an offline device, EXP-877 a dropped stream
 *  with its Reconnect, EXP-804 the persisted usage wall with EXP-849's
 *  account switch, EXP-724 the compaction strip, EXP-784 the rate-limit
 *  window, the starting line), as `DockSource`s with their sheets and
 *  actions (dock.json `banner`: the host's words). The session view and
 *  the thread's Conversation share it (one relay store per session). */
export function useSessionDock(session: SessionRow): {
  sources: DockSource[]
  renderSheet: (row: DockRow, close: () => void) => ReactNode
  renderAction: (row: DockRow) => ReactNode
} {
  const store = useMemo(() => acquireSteerSession(session.id), [session.id])
  const { phase, feed, compacting, rateLimit, taskList, backgroundTasks, queue } = useSyncExternalStore(
    store.subscribe,
    store.getSnapshot
  )
  const device = useSessionDevice({ deviceId: session.deviceId, userId: session.userId })
  const now = useNow(30_000)
  const sessionEnded = session.status === `ended`
  const paused =
    device.online === false &&
    !sessionEnded &&
    (phase.kind === `starting` || phase.kind === `connecting` || phase.kind === `idle` || phase.kind === `closed`)
  const blockedLabel = blockedBadgeLabel(session.blocked, now)
  const ticking = backgroundTasks.some((task) => task.kind !== `agent`)
  const tick = useNow(ticking ? 1_000 : 60_000)
  const queueModel = useMemo(
    () => sessionTaskQueue({ taskList, backgroundTasks, feed, now: tick.getTime() }),
    [taskList, backgroundTasks, feed, tick]
  )
  const [todoOpen, setTodoOpen] = useState(true)
  const [completedOpen, setCompletedOpen] = useState(false)
  const limit = rateLimit ? rateLimitBanner(rateLimit, now) : null
  const agentHint = useAgentHintDock(session)
  const hintSource = agentHint.source
  const sources = useMemo<DockSource[]>(() => {
    const out: DockSource[] = [
      { kind: `queued`, messages: queue.map((entry) => ({ id: entry.id, text: entry.text })) },
      { kind: `tasks`, queue: queueModel },
    ]
    if (paused && feed.length > 0) {
      out.push({
        kind: `banner`,
        id: `paused`,
        icon: `ui-device-offline`,
        label: `${device.label ?? `The device`} is offline`,
        text: `The agent is paused on that machine and continues when it comes back online.`,
        tone: `muted`,
        action: null,
      })
    }
    if (phase.kind === `closed` && !paused) {
      out.push({
        kind: `banner`,
        id: `closed`,
        icon: `ui-refresh`,
        label: `Connection lost`,
        text: phase.detail ?? `Connection lost.`,
        tone: `muted`,
        action: `Reconnect`,
      })
    }
    if (blockedLabel) {
      out.push({
        kind: `banner`,
        id: `blocked`,
        icon: `ui-usage`,
        label: `Usage limit`,
        text: blockedLabel,
        tone: `warning`,
        action: !sessionEnded && (session.blocked?.kind ?? `rate_limit`) === `rate_limit` ? `Switch account` : null,
      })
    }
    if (compacting !== null) {
      out.push({
        kind: `banner`,
        id: `compacting`,
        icon: `coding-compact`,
        label: `Compacting`,
        text: compacting.trigger === `manual` ? `${COMPACTING_LABEL} · requested` : COMPACTING_LABEL,
        tone: `muted`,
        action: null,
      })
    }
    if (limit) {
      out.push({
        kind: `banner`,
        id: `rate-limit`,
        icon: `ui-usage`,
        label: `Rate limit`,
        text: `${limit.text} · ${rateLimitDetail(limit.resets)}`,
        tone: `warning`,
        action: null,
      })
    }
    if (phase.kind === `starting` && !paused && feed.length > 0) {
      out.push({
        kind: `banner`,
        id: `starting`,
        icon: `ui-loading`,
        label: `Starting`,
        text: `The agent is starting. Waiting for the live stream…`,
        tone: `muted`,
        action: null,
      })
    }
    if (hintSource) out.push(hintSource)
    return out
  }, [queue, queueModel, paused, feed.length, device.label, phase, blockedLabel, sessionEnded, session.blocked, compacting, limit, hintSource])
  const renderSheet = (row: DockRow): ReactNode => {
    if (row.kind === `tasks`) {
      return (
        <DockSheetBody>
          <TaskQueueStrip
            taskList={taskList}
            backgroundTasks={backgroundTasks}
            feed={feed}
            todoOpen={todoOpen}
            onTodoOpenChange={setTodoOpen}
            completedOpen={completedOpen}
            onCompletedOpenChange={setCompletedOpen}
            plain
          />
        </DockSheetBody>
      )
    }
    if (row.kind === `queued`) {
      return (
        <DockSheetBody>
          <QueueStrip messages={queue} onRemove={(entry) => store.unqueue(entry.id)} plain />
        </DockSheetBody>
      )
    }
    return null
  }
  const renderAction = (row: DockRow): ReactNode => {
    if (row.id === `banner:closed`) {
      return (
        <Button variant="outline" size="sm" className="h-6" onClick={() => store.reconnect()}>
          <UiRefreshIcon />
          Reconnect
        </Button>
      )
    }
    if (row.id === `banner:blocked` && row.action) return <SessionAccountSwitchButton session={session} />
    return agentHint.renderAction(row)
  }
  return { sources, renderSheet, renderAction }
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
  chromeless = false,
}: {
  item: QuestionItem
  active: boolean
  canAnswer: boolean
  answerState?: AnswerState
  onAnswer: AnswerHandler
  onSend: SendHandler
  /** EXP-788: this is the card the number keys answer. */
  hotkeys: boolean
  /** N-12: inside THE Dock's sheet (its header is the card's chrome): the
   *  body never folds, no AskCard around it. */
  chromeless?: boolean
}) {
  const plan = item.planMode
  const clamp = useClampToggle(item.text)
  // The plan is the thing the reader must approve — it is never folded; a
  // sheet's body scrolls instead.
  const clampable = !plan && !chromeless && clamp.clampable
  const { expanded, setExpanded } = clamp
  const Shell = chromeless ? BareShell : AskCard
  return (
    <Shell plan={plan} label={plan ? ASK_DOCK_COPY.planReady : item.header}>
      <>
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
        </>
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
    </Shell>
  )
}

/** N-12: the card's content with no chrome (THE Dock's sheet draws it). */
function BareShell({ children }: { plan?: boolean; label?: ReactNode; meta?: ReactNode; children: ReactNode }) {
  return <>{children}</>
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
  chromeless = false,
}: {
  items: QuestionItem[]
  activeIds: Set<number>
  canAnswer: boolean
  answerStates: AnswerStates
  onAnswer: AnswerHandler
  onSend: SendHandler
  /** EXP-788: the id of the card the number keys answer, if it is one of ours. */
  pendingId: number | null
  /** N-12: inside THE Dock's sheet: no AskCard around it. */
  chromeless?: boolean
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

  const Shell = chromeless ? BareShell : AskCard
  return (
    <Shell
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
                  <Button
                    variant="text"
                    size="inline"
                    className="mt-1.5"
                    onClick={() => setEditingId(null)}
                  >
                    {BACK_TO_CURRENT_STEP}
                  </Button>
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
              <DisclosureHeader
                open={false}
                onToggle={() => setEditingId(null)}
                className="mt-1 gap-1.5 py-1 text-xs"
              >
                <span className="min-w-0 flex-1 truncate">
                  {submitStep ? `Review answers` : current.item.text}
                </span>
              </DisclosureHeader>
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
    </Shell>
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
        {dismissed ? ASK_CARD_COPY.dismissed : (answer ?? PLAN_CARD_COPY.answered)}
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
/** A transient API failure (timeout, dropped connection, rejected request)
 *  where it happened in the transcript: a quiet warning line, never the amber
 *  RateLimitBanner, which stays for real walls. Mirrored ×4. */
function ApiErrorRow({
  message,
  errorType,
}: {
  message: string
  errorType?: string
}) {
  return (
    <div
      className={cn(`flex min-w-0 items-start gap-1.5 pl-0.5`, TRANSCRIPT_TOOL_TEXT)}
      title={errorType}
      data-testid="api-error-row"
    >
      <UiWarningIcon className="mt-0.5 size-3 shrink-0 text-destructive/70" />
      <span className="min-w-0 whitespace-pre-wrap break-words text-muted-foreground">
        {message}
      </span>
      {errorType && (
        <span className="shrink-0 text-muted-foreground/60">
          {errorType.replaceAll(`_`, ` `)}
        </span>
      )}
    </div>
  )
}

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

/** EXP-856 §4: a second copy of a live agent. Amber, verbatim off the wire —
 *  the sentence is the engine's (`duplicate_agent_detail`), never rebuilt per
 *  client — and never hidden behind a fold. */
function DuplicateWarningRow({ detail }: { detail: string }) {
  return (
    <div
      className="flex min-w-0 items-start gap-2 text-amber-400"
      data-testid="subagent-duplicate-warning"
    >
      <UiWarningIcon className="mt-0.5 size-3 shrink-0" />
      <span className="min-w-0">{detail}</span>
    </div>
  )
}

/** EXP-856 §4: the duplicate warnings under a card's own rows. */
function DuplicateWarnings({ details }: { details: readonly string[] }) {
  if (details.length === 0) return null
  return (
    <div className="space-y-1 border-t border-glass-stroke px-3 py-2">
      {details.map((detail) => (
        <DuplicateWarningRow key={detail} detail={detail} />
      ))}
    </div>
  )
}

/** EXP-850 §3 / EXP-1240: a claude `Workflow` call as THE Work card — the
 *  phases as a segment bar, the finished ones folded into one row, the
 *  running phase's lanes one line each. A lane opens that agent's own tab
 *  (EXP-1225: a workflow agent is a conversation tab like any subagent). */
function WorkflowCardRow({
  workflow,
  duplicates = [],
  onOpenAgent,
}: {
  workflow: WorkflowState
  /** §4: the duplicate warnings that named THIS workflow. */
  duplicates?: readonly string[]
  onOpenAgent: (subagentId: string) => void
}) {
  const card = useMemo(() => workflowWorkCard(workflow), [workflow])
  const folded = useMemo(
    () => workflowFoldedPhases(workflow, card),
    [workflow, card]
  )
  const openLane = (target: WorkCardLaneTarget) => {
    const agentId = workflowLaneAgent(workflow, target)?.agentId
    if (agentId) onOpenAgent(agentId)
  }
  return (
    <WorkCard
      card={card}
      foldedPhases={folded.length > 0 ? folded : undefined}
      onOpenLane={openLane}
      className={TRANSCRIPT_TOOL_TEXT}
    >
      <DuplicateWarnings details={duplicates} />
    </WorkCard>
  )
}

/** EXP-1324: a subagent as its Trace line — "Running the Explore agent"
 *  (shimmering) → "Ran the Explore agent" with the duration as an outline
 *  badge (`traceSubagent`); the line opens the subagent's own tab, where its
 *  rows live. The label is the agent's TYPE; the spawning call's description
 *  rides the tooltip. */
function SubagentLineRow({
  items,
  live,
  onOpen,
}: {
  items: FeedItem[]
  /** The run is live: an unfinished subagent is still running. */
  live: boolean
  onOpen: () => void
}) {
  const summary = useMemo(() => summarizeSubagentRow(items), [items])
  const running = live && !summary.done
  const now = useNow(running ? 1_000 : 3_600_000)
  const durationMs = subagentDurationMs(
    items as { kind: string; status?: string; at?: number }[],
    now.getTime()
  )
  const type = summary.agentType.trim()
  const model = traceSubagent({
    label: type === SUBAGENT_FALLBACK_TYPE ? `` : type,
    running,
    failed: false,
    // A finished agent whose completed edge carried no stamp has no span.
    durationMs: summary.done || running ? durationMs : null,
  })
  // EXP-856 §4: a second copy of this agent started while the first was still
  // running. Amber, verbatim off the wire, and never folded away.
  const duplicate = summary.duplicateDetail
  return (
    <>
      <Trace size="sm">
        {/* The words-only Button in the Trace child's geometry (what the
            group row's TraceCollapsibleTrigger wears): the line IS the
            control, its own text styles over the button's. */}
        <Button
          type="button"
          variant="text"
          size="inline"
          className="w-fit max-w-full min-w-0 justify-start gap-0 rounded-md text-left font-normal"
          onClick={onOpen}
          data-testid="session-subagent-line"
        >
          <TraceModelLine
            model={model}
            tooltip={subagentLabel(summary)}
            description={
              model.description ? (
                <TraceDescription className="rounded-full border border-border px-1.5 text-[0.625rem] leading-4">
                  {model.description}
                </TraceDescription>
              ) : null
            }
            trailing={
              <TraceIcon>
                <TraceChevronIcon className="size-full" />
              </TraceIcon>
            }
          />
        </Button>
      </Trace>
      <DuplicateWarnings details={duplicate ? [duplicate] : []} />
    </>
  )
}

/** EXP-1324: the agent's reasoning as its Trace line — "Thinking" while it
 *  streams, "Thought for 5s" once its `ended` edge landed (`traceReasoning`).
 *  Never collapsible: no text rides the wire. */
function ThoughtRow({
  item,
  running,
}: {
  item: Extract<FeedItem, { kind: `thought` }>
  running: boolean
}) {
  const model = traceReasoning({ running, durationMs: item.durationMs ?? null })
  return (
    <Trace size="sm" data-testid="session-thought-line">
      <TraceModelLine model={model} />
    </Trace>
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
  live,
}: {
  summary?: SubagentSummary
  items: FeedItem[]
  live: boolean
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
          item.kind === `thought` ||
          item.kind === `narration` ||
          item.kind === `user_message` ||
          item.kind === `api_error`
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
        <Empty id="transcript" />
      ) : (
        /* EXP-787: a subagent's conversation is a transcript too — same rows,
           same gap ladder. */
        <LaneRows items={stream} gaps live={live && summary?.done !== true} />
      )}
    </>
  )
}

/** EXP-1206: the transcript STREAMS — a live publisher mid-turn
 *  (`turnState === 'started'`). While it does, every unsettled call is a
 *  RUNNING one (claude runs tool calls in parallel: only the last of them is
 *  the `liveToolRowId` row); once the stream is gone, an unsettled call reads
 *  settled. Provided once by the raw view; a lane or tab inherits it. */
const TranscriptStreamingContext = createContext(false)

/** EXP-1324: one tool call as its Trace line (`traceTool`): the kind's
 *  icon, "Running ./gradlew …" (shimmering) → "Ran ./gradlew …", rose when
 *  it failed. A settled call that printed is COLLAPSIBLE and opens onto its
 *  output in the CodeBlock (`execute` = the `ansi` Terminal pattern, a failed
 *  call the destructive block). A settled `execute` whose output parses as a
 *  test run with failures IS the failed-tests line instead ("Failed: 1 of 12
 *  tests in StartupBenchmark", `traceFailedTests`), its content the output.
 *
 *  EXP-1206: `live` — set for the ONE row `liveToolRowId` names, the call
 *  still RUNNING — is the line ALONE: nothing streams under it, so the
 *  transcript never jumps while a command prints; a settled row's output
 *  stays folded until the reader's own tap opens it. The TENSE is the row's
 *  own: an unsettled call reads "Running …" while it is live OR the
 *  transcript still streams (parallel calls run beside the live one); one
 *  that never settled once the stream is gone (a publisher that died
 *  mid-call) reads settled. */
function ToolRow({
  item,
  flush = true,
  live = false,
}: {
  item: ToolItem
  flush?: boolean
  /** EXP-895: this call is the one still RUNNING (`liveToolRowId`). */
  live?: boolean
}) {
  const failed = item.failed === true
  const streaming = useContext(TranscriptStreamingContext)
  const [pinned, setPinned] = useState(false)
  const open = !live && pinned
  const model = useMemo(() => {
    const settled = item.settled === true || (!live && !streaming)
    if (item.toolKind === `execute` && item.settled === true && item.output) {
      const tests = traceFailedTests(parseTestResults(item.output))
      if (tests) return tests
    }
    return traceTool({
      toolKind: item.toolKind ?? `other`,
      name: item.name,
      detail: item.detail ?? null,
      settled,
      failed,
    })
  }, [item.toolKind, item.settled, item.output, item.name, item.detail, live, streaming, failed])
  // EXP-846: one of OUR MCP tools reads as a sentence with our mark on it.
  // EXP-1279: a code-mode call is its Script card.
  if (isScriptCall(item)) return <ScriptCallRow item={item} flush={flush} />
  const exp = expToolDisplay(item.name)
  // EXP-1241: a SETTLED comment call is the comment itself, in the comment
  // look; running or failed it keeps its ordinary row (the desktop's rule).
  if (
    exp?.name === COMMENT_CALL_TOOL &&
    item.settled === true &&
    item.failed !== true
  ) {
    return <CommentCallRow item={item} />
  }
  if (exp) return <ExpToolRow item={item} display={exp} live={live} />
  const tooltip =
    item.detail && model.description === null ? item.detail : undefined
  if (item.output === undefined || live) {
    return (
      <Trace size="sm" data-testid="session-tool-line">
        <TraceModelLine model={model} tooltip={tooltip} />
      </Trace>
    )
  }
  return (
    <Trace size="sm" data-testid="session-tool-line">
      <TraceCollapsible open={open} onOpenChange={setPinned}>
        <TraceCollapsibleTrigger
          aria-label={open ? `Hide the output` : `Show the output`}
        >
          <TraceModelLine model={model} tooltip={tooltip} />
        </TraceCollapsibleTrigger>
        <TraceCollapsibleContent>
          <ToolOutput
            output={item.output}
            language={item.toolKind === `execute` ? `ansi` : `plain`}
            failed={failed || model.icon === `trace-failed`}
          />
        </TraceCollapsibleContent>
      </TraceCollapsible>
    </Trace>
  )
}

/** EXP-1279: a code-mode `exec` call as its Script card ("Ran a script · 59
 *  calls · 3.0 s", the calls grouped by tool, the script behind "Script").
 *  The model is the contract's (`scriptCard`); an older codemode that logs
 *  nothing is the bare "Ran a script". */
function ScriptCallRow({ item, flush }: { item: ToolItem; flush: boolean }) {
  const card = useMemo(() => scriptCard(scriptCardInput(item)), [item])
  return (
    <div className={cn(`min-w-0`, !flush && `py-0.5`)}>
      <ScriptCard
        card={card}
        running={item.settled !== true}
        failed={item.failed === true}
        source={item.script?.source}
        className={TRANSCRIPT_TOOL_TEXT}
      />
    </div>
  )
}

/** EXP-895 — what one `execute` call printed, as its settle put it on the
 *  wire (already redacted and tail-cut by the publisher; a cut output OPENS
 *  with the `\ N more lines truncated` marker). EXP-1324: THE CodeBlock at
 *  size sm, titled `Output` with the copy action; `ansi` colours command
 *  output (the escapes never reach the text: the copy value is the SDK's
 *  `codeText`, what the block itself copies), a failed call is destructive.
 *  Scrolled to the BOTTOM on mount: the verdict is the last line. */
const ToolOutput = memo(function ToolOutput({
  output,
  language,
  failed,
}: {
  output: string
  language: string
  failed: boolean
}) {
  const box = useRef<HTMLPreElement | null>(null)
  useEffect(() => {
    const node = box.current
    if (node) node.scrollTop = node.scrollHeight
  }, [output])
  const text = useMemo(() => codeText(output, language), [output, language])
  return (
    <CodeBlock
      size="sm"
      variant={failed ? `destructive` : `default`}
      data-testid="session-tool-output"
    >
      <CodeBlockHeader>
        <CodeBlockTitle>{TRACE_COPY.output}</CodeBlockTitle>
        <CodeBlockActions>
          <CodeBlockCopyButton
            value={text}
            labels={{ copy: TRACE_COPY.copy, copied: TRACE_COPY.copied }}
          />
        </CodeBlockActions>
      </CodeBlockHeader>
      <CodeBlockContent ref={box} language={language}>
        {output}
      </CodeBlockContent>
    </CodeBlock>
  )
})

/** EXP-1172: the session's synced Guide (`sessions.guide`), so a settled
 *  `sessions_show` row can draw the picture its answer named. */
const SessionResultsRawContext = createContext<unknown>(null)

/** EXP-1172: the picture a settled `sessions_show` call filed (contract result
 *  kind `picture`), found by the attachment id its answer carried
 *  (`preview.id`) in the synced results — nothing while the upload is still
 *  in flight. It sits under the call, so pictures read in publish order. */
function ShownPicture({ item }: { item: ToolItem }) {
  const raw = useContext(SessionResultsRawContext)
  const entry = useMemo(
    () => sessionResultPicture(raw, item.preview?.id),
    [raw, item.preview?.id]
  )
  if (!entry) return null
  return (
    <SessionInlineResultTile
      entry={entry}
      attachmentSrc={(id) => `/api/attachments/${id}`}
      className="ml-5 pt-1.5"
    />
  )
}

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
  live = false,
}: {
  item: ToolItem
  display: ExpToolDisplay
  live?: boolean
}) {
  const failed = item.failed === true
  const caption = expToolCaption(display, item.settled === true)
  // The subject is the input field the contract names (`subjectKey`), which
  // the publisher sends as the call's `detail`. Once the call SETTLED the
  // answer's own title/identifier is the better word for the same thing (and
  // the honest one while a publisher derives `detail` generically), so it
  // wins there. EXP-1127: a single ref (`preview.refs`) is read first; the
  // legacy `title`/`identifier` stay as the permanent pre-EXP-920 branch.
  const ref =
    item.preview?.refs?.length === 1 ? item.preview.refs[0] : undefined
  const subject =
    (item.settled
      ? (ref?.title ??
        ref?.identifier ??
        item.preview?.title ??
        item.preview?.identifier ??
        item.detail)
      : item.detail) ?? null
  // EXP-1324: our call is a Trace line too — the mark, the sentence, the
  // subject muted after it; rose when it failed, shimmering while it runs.
  const status = failed
    ? (`error` as const)
    : live && item.settled !== true
      ? (`running` as const)
      : null
  return (
    <div className="min-w-0">
      <Trace size="sm" data-testid="session-exp-tool-line">
        <TraceLine status={status}>
          <TraceIcon>
            <ExponentialLogo variant="light" size={14} className="size-full" />
          </TraceIcon>
          <TraceTitle title={subject ? `${caption} ${subject}` : caption}>
            {caption}
            {subject && (
              <span className="text-muted-foreground/70">{` ${subject}`}</span>
            )}
          </TraceTitle>
        </TraceLine>
      </Trace>
      {!failed && display.result === `picture` && item.settled === true ? (
        <ShownPicture item={item} />
      ) : (
        !failed &&
        item.preview && (
          <ExpToolResult kind={display.result} preview={item.preview} />
        )
      )}
    </div>
  )
}

/** EXP-846: the settled call's result, by contract `result` kind. An issue
 *  gets the very chip an `#IDENT` reference renders (preview on hover, tap
 *  opens the issue) when the row is synced here, and the same chip inert with
 *  a muted glyph when it is not (EXP-887); a PR gets its link; a list its row count; the
 *  named things a small chip. `none` renders nothing at all.
 *
 *  EXP-920: a publisher that distilled `refs` gets the entity chip ROW instead
 *  — one chip per ref group, each with its hover card and target
 *  (`components/entity-preview/`), the same rendering ×4. The legacy branches
 *  below stay for previews that carry no refs. */
function ExpToolResult({
  kind,
  preview,
}: {
  kind: string
  preview: ExpToolPreview
}) {
  const issueRefs = useIssueRefs()
  const label = preview.title ?? preview.identifier ?? preview.id ?? null
  if (preview.refs && preview.refs.length > 0) {
    return (
      <div className="ml-5 pt-0.5">
        <EntityRefChips refs={preview.refs} />
      </div>
    )
  }
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
 *  · 1 failed"), byte-identical on every client. EXP-1324: the fold is a
 *  TraceCollapsible over the members' own Trace lines. While the run is the
 *  trailing row of a live session, the caption shimmers and the latest
 *  call's LINE stays visible under it while folded, so the viewer still sees
 *  live progress (EXP-1206: never its output). */
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
    <Trace size="sm" data-testid="session-tool-group">
      <TraceCollapsible open={expanded} onOpenChange={setExpanded}>
        <TraceCollapsibleTrigger>
          <TraceLine status={liveTail ? `running` : null}>
            <TraceIcon>
              <TraceToolIcon />
            </TraceIcon>
            <TraceTitle title={caption}>{caption}</TraceTitle>
          </TraceLine>
        </TraceCollapsibleTrigger>
        <TraceCollapsibleContent>
          <div className="flex flex-col gap-1.5">
            {items.map((item) => (
              // EXP-1206: the RUNNING call is its line alone — the same rule
              // the top-level rows follow.
              <ToolRow
                key={item.id}
                item={item}
                live={liveTail && item.id === latest.id}
              />
            ))}
          </div>
        </TraceCollapsibleContent>
      </TraceCollapsible>
      {!expanded && liveTail && (
        <div className="pl-5">
          <ToolRow item={latest} live />
        </div>
      )}
    </Trace>
  )
}

/** EXP-948: a run of ≥2 consecutive calls to the SAME Exponential MCP tool.
 *  Our own work never hides inside a generic "N other tools" fold: the run
 *  wears the contract's own plural caption ("Reading 3 issues" while any call
 *  is in flight, "Read 3 issues" once they all settled, plus "· N failed"),
 *  the brand mark every single call of ours already carries, and it opens to
 *  the individual rows ("Read issue EXP-901") like any tool group. */
function ExpToolGroupRow({ items }: { items: ToolItem[] }) {
  const [expanded, setExpanded] = useState(false)
  const caption = useMemo(() => expToolGroupCaption(items), [items])
  return (
    <Trace size="sm" data-testid="session-exp-tool-group">
      <TraceCollapsible open={expanded} onOpenChange={setExpanded}>
        <TraceCollapsibleTrigger>
          <TraceLine>
            <TraceIcon>
              <ExponentialLogo variant="light" size={14} className="size-full" />
            </TraceIcon>
            <TraceTitle title={caption}>{caption}</TraceTitle>
          </TraceLine>
        </TraceCollapsibleTrigger>
        <TraceCollapsibleContent>
          <div className="flex flex-col gap-1.5">
            {items.map((item) => (
              <ToolRow key={item.id} item={item} />
            ))}
          </div>
        </TraceCollapsibleContent>
      </TraceCollapsible>
      {/* EXP-1172: a run of show calls keeps its pictures in view while
          folded — a picture never hides in a fold (open, each row has its
          own). */}
      {!expanded &&
        items.map((item) =>
          item.settled === true &&
          item.failed !== true &&
          expToolDisplay(item.name)?.result === `picture` ? (
            <ShownPicture key={item.id} item={item} />
          ) : null
        )}
    </Trace>
  )
}
