import { chatPendingAsks, chatSessionNames } from "@exp/domain-contract/ask-dock"
import { WAITING_CARDS_CONSTANTS } from "@exp/domain-contract/waiting-cards"
import { sendSessionMessage } from "@/lib/enet/send-sealed"
import type { ChatLead } from "@exp/domain-contract/chats"
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import {
  CardHighlightContext,
  CHAT_COLUMN_CLASS,
  conceptIcon,
  FloatingComposer,
  JumpToBottomButton,
  JUMP_TO_BOTTOM_GAP,
  LoadEarlierButton,
  useTranscriptWindow,
  Menu,
  MenuGestureHost,
  menuProps,
  SkeletonRows,
  chatLeadPrState,
  ChatDock,
  chatDockedIds,
  ChatConversation,
  type WaitingCardRow,
  ChatTabs,
  useIsMobile,
  userMessageTime,
  type MenuSessionProps,
  type ChatStripIssue,
  type ChatStripOpen,
  type ChatTreeRow,
  PlanPage,
} from "@exp/ui"
import type { Issue, User } from "@/db/schema"
import { SessionLiveTail, useSessionDock } from "@/components/agent-session"
import { IssueChip as IssueRefChip } from "@/components/issue-chip"
import { IssueRefOutlet, useIssueRefs } from "@/components/issue-ref-provider"
import { IssueDetailView } from "@/components/issue-detail-view"
import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { SessionComposer, SessionComposerReplies } from "@/components/session-composer"
import {
  HEADER_BUTTON_CLASS,
  MOBILE_DETAIL_SCREEN_CLASS,
  MobileDetailHeader,
} from "@/components/team/mobile-detail-header"
import { ChatGuideMerge, chatGuidePullRequest } from "@/components/chat/chat-guide"
import { ChatResults } from "@/components/chat/chat-results"
import { useResultsTab } from "@/hooks/use-results-tab"
import { RESULTS_TAB } from "@exp/domain-contract/results-tab"
import { useAnswerMessage, useUnqueueMessage } from "@/hooks/use-answer-message"
import { renderChatText } from "@/components/chat/chat-markdown"
import { ChatScreenHeaderMenu, useChatTabMenu } from "@/components/chat/chat-tab-menu"
import { useChatActions } from "@/components/chat/chat-menu"
import { ChatHeaderTitle } from "@/components/chat/chat-header"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { BackStackButton, InPlaceSession } from "@/components/chat/session-body"
import { useSessionNav, useXrefAgent } from "@/hooks/use-session-nav"
import { useAgentRow } from "@/hooks/use-agent-row"
import { backSearch } from "@/lib/session-back"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import { primeTabEnter, TAB_BODY_TOUCH_CLASS, useFaceSwipe } from "@/hooks/use-face-swipe"
import { useMarkRead, useNewestAgentMessageAt } from "@/hooks/use-mark-read"
import { useSuggestionActions } from "@/hooks/use-suggestion-actions"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useSession } from "@/hooks/use-session"
import { useSessionMessages, useSessionWaitingCard, type MessagePage } from "@/hooks/use-session-tree"
import { useTeamBoards, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { chatPageOf, chatPages, chatPageSearch, type ChatPage } from "@/lib/chat-pages"
import { cn } from "@/lib/utils"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  sessionCard,
  sessionTabMarkState,
  CHAT_COPY,
  CHAT_ROW_WORDS,
  toTreeSession,
  treeOrder,
  type MessageRow,
  type PullRequestRow,
  type SessionRow,
  type SessionChatView,
} from "@/lib/session-tree"
import { agentChatTabs, chatMode, chatStripMode, type SessionChatRow } from "@exp/domain-contract/session-tree"
import { planRowLabel, planTabTarget } from "@exp/domain-contract/plan-page"

// EXP-1293 / S-3: THE chat screen (fixture session-chat.json
// ×4): the header (title + ONE tab strip), the Conversation (THE Dock, the
// composer, the jump button, selection), the issue / session tabs and the
// Results tab (results-tab.json, pinned right).
// The chat route (`/t/$teamSlug/sessions/$sessionId`) and an agent's chat
// (`/t/$teamSlug/agents/$agentId`, chats.json CHAT MODE: `chatMode`
// = bubbles, the agent's name, call button and agent menu in the header)
// both render it; the route owns the `?tab=` search and how it navigates.

export type ChatSearch = {
  tab?: `issue` | `session` | `results` | `plan`
  /** #EXP-1662: the plan card a row opened (`tab=plan`; else `planTabTarget`). */
  plan?: string
  issue?: string
  /** The session open in place (`tab=session`). */
  session?: string
  /** The Results tab's review pane (`tab=results`, the complete diff). */
  section?: `all`
  /** The file in focus on the review pane. */
  file?: string
  /** #FEED-191 (needs-you.json): the Conversation scrolls to this card, THE
   *  waiting chip's way, once it is there. */
  card?: string
  /** Where it was opened from (`TABLESS_ORIGIN` = a Running row: no tab). */
  from?: string
  /** S-3: the inter-session back stack (`lib/session-back.ts`); the
   *  header's `‹` pops it. */
  back?: string
}

/** The `?tab=` search the chat screen reads (the agent chat's too). */
export function validateChatSearch(search: Record<string, unknown>): ChatSearch {
  const pick = (value: unknown) => (typeof value === `string` && value ? value : undefined)
  // The retired `?tab=guide` and `?tab=stories` land on their successor.
  const tab =
    search.tab === `issue` || search.tab === `session` || search.tab === `results` || search.tab === `plan`
      ? search.tab
      : search.tab === `guide` || search.tab === `stories`
        ? `results`
        : undefined
  const section = tab === `results` && search.section === `all` ? (`all` as const) : undefined
  return {
    ...(tab ? { tab } : {}),
    ...(section ? { section } : {}),
    ...(section && pick(search.file) ? { file: pick(search.file) } : {}),
    ...(tab === `issue` && pick(search.issue) ? { issue: pick(search.issue) } : {}),
    ...(tab === `plan` && pick(search.plan) ? { plan: pick(search.plan) } : {}),
    ...(tab === `session` && pick(search.session) ? { session: pick(search.session) } : {}),
    ...(pick(search.card) ? { card: pick(search.card) } : {}),
    ...(pick(search.from) ? { from: pick(search.from) } : {}),
    ...backSearch(search.back),
  }
}

/** The issue chips' context-menu kind (`menuProps`). */
const CHAT_ISSUE_MENU = `chat-issue`

const PrivateIcon = conceptIcon(`ui-private`)
// #EXP-1380 (chats.json PERF): a STABLE callback, or every row re-renders
// with the page (`StableRow` compares it by identity).
const attachmentSrc = (id: string) => `/api/attachments/${id}`

/** S-3: an agent conversation's header parts (the route's). */
export interface ChatScreenAgent {
  name: string
  /** The agent's disc beside the title (md+). */
  glyph: ReactNode
  /** The phone header's title (the disc + the name). */
  mobileTitle: ReactNode
  /** The header's trailing slot (the call button, THE agent menu). */
  headerMenu: (className?: string) => ReactNode
  /** Over the body (the voice call card). */
  overlay?: ReactNode
}

export function ChatScreen({
  team,
  teamSlug,
  view,
  root,
  rows,
  titles,
  pullRequests,
  messagePage,
  search,
  onNavigate,
  agent,
}: {
  /** The route's `?tab=` state (the chat's or the agent chat's). */
  search: ChatSearch
  /** Navigate the SAME screen to `next` (replace = no history entry). */
  onNavigate: (next: ChatSearch, options?: { replace?: boolean }) => void
  /** S-3: an agent conversation's header (its name, the call button and
   *  THE agent menu); absent = the chat's. */
  agent?: ChatScreenAgent
  team: Team
  teamSlug: string
  view: SessionChatView
  root: SessionRow
  /** Every session of the tree (the chips' marks, the tree popover). */
  rows: SessionRow[]
  /** EXP-1341: every tree session's `sessionTitle` (the chips' names). */
  titles: ReadonlyMap<string, string>
  pullRequests: PullRequestRow[]
  /** #EXP-1380: the loaded newest messages (Load earlier pages more). */
  messagePage: MessagePage
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const deviceGlyph = useDeviceGlyphs()
  const permissions = useTeamPermissions(team)
  const { users } = useTeamUsers(team.id)
  const boards = useTeamBoards(team.id)
  const { resolve } = useTeamStatusesContext()
  // EXP-1316: the phone header band floats over the body; bodies pad by it.
  const [headerRef, headerSize] = useMeasuredSize()
  const topInset = isMobile ? headerSize.height : 0

  const issueTabs = view.tabs.filter(
    (tab): tab is Extract<SessionChatView[`tabs`][number], { kind: `issue` }> =>
      tab.kind === `issue`
  )
  const sessionTabs = view.tabs.filter(
    (tab): tab is Extract<SessionChatView[`tabs`][number], { kind: `session` }> =>
      tab.kind === `session`
  )
  // EXP-1334: every linked issue, the top-level chips' AND their folded
  // sub-issues (a nested one opens in place too).
  const issueIds = issueTabs.flatMap((tab) => [
    tab.issueId,
    ...tab.nested.map((nested) => nested.issueId),
  ])
  const issueKey = [...issueIds].sort().join(`,`)
  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query.from({ issues: issueCollection }).where(({ issues }) => inArray(issues.id, issueIds))
        : undefined,
    [issueKey]
  )
  const issuesById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])),
    [issueRows]
  )
  const guidePr = useMemo(() => chatGuidePullRequest(pullRequests), [pullRequests])

  const rowsById = useMemo(() => new Map(rows.map((row) => [row.id, row])), [rows])
  const focused = view.focusId ? (rowsById.get(view.focusId) ?? null) : null
  /** EXP-1323: a suggested-tasks card's Start with… lists the POSTING
   *  session's agent's models (a session of the tree; the root's as the
   *  fallback). */
  const agentFor = useCallback(
    (sessionId: string) => rowsById.get(sessionId)?.cli ?? root.cli,
    [rowsById, root.cli]
  )
  // EXP-1323: opening the chat marks its root read (again whenever a newer
  // agent reply lands anywhere in the tree while it is open); an opened
  // session marks itself (`session-body.tsx`).
  const treeIds = useMemo(() => rows.map((row) => row.id), [rows])
  useMarkRead(root.id, useNewestAgentMessageAt(treeIds))
  // NEW-6 (N-1) / NEW-46: on a single-session chat the Conversation tab
  // IS the root's raw work in place (the default body).
  // S-3 (chats.json CHAT MODE): an agent conversation always paints its
  // bubbles, never the root's raw work in place.
  const mainSession =
    view.mainSessionId && chatMode(root) !== `bubbles` ? (rowsById.get(view.mainSessionId) ?? null) : null
  // #EXP-1662 (plan-page.json): the chat's plan tab.
  const planRows = useMemo(
    () => view.rows.filter((row): row is Extract<SessionChatRow, { type: `plan` }> => row.type === `plan`),
    [view.rows]
  )
  // #FEED-191 (plan-page.json `planTabTarget(plans, kept)`): `kept` = the plan
  // the tab showed during THIS visit of the chat (its target, or a plan whose
  // row was pressed), so a plan decided while the person is here keeps its
  // chip; leaving the chat (unmount, or another root) clears it.
  const [keptState, setKeptState] = useState<{ rootId: string; planId: string } | null>(null)
  const kept = keptState?.rootId === root.id ? keptState.planId : null
  const tabPlanId = planTabTarget(planRows, kept)
  const tabPlan = planRows.find((row) => row.id === tabPlanId) ?? null
  const shownPlan = search.tab === `plan` ? (planRows.find((row) => row.id === search.plan) ?? tabPlan) : null
  const showingPlanId = shownPlan?.id ?? tabPlanId
  useEffect(() => {
    if (showingPlanId && showingPlanId !== kept) setKeptState({ rootId: root.id, planId: showingPlanId })
  }, [showingPlanId, kept, root.id])
  const tab =
    search.tab === `issue` && search.issue && issueIds.includes(search.issue)
      ? { kind: `issue` as const, issueId: search.issue }
      : search.tab === `results` && chatMode(root) !== `bubbles`
        ? { kind: `results` as const }
        : shownPlan
          ? { kind: `plan` as const }
        : search.tab === `session` && focused && focused.id !== root.id
        ? { kind: `session` as const, sessionId: focused.id }
        : mainSession
          ? { kind: `session` as const, sessionId: mainSession.id }
          : { kind: `conversation` as const }
  const sessionInPlace = tab.kind === `session` ? (focused?.id === tab.sessionId ? focused : mainSession) : null

  // S-3: the back stack rides along every tab switch; an xref pushes it.
  const keepBack = search.back ? { back: search.back } : {}
  const go = (next: ChatSearch) => onNavigate({ ...next, ...keepBack }, { replace: true })

  /** A session opens IN PLACE (a pushed history entry: Back returns). */
  const openSession = (sessionId: string) =>
    onNavigate({ tab: `session`, session: sessionId, ...keepBack })
  /** S-3: the session on show (an xref pushes it; `‹` pops back to it). */
  const nav = useSessionNav({
    teamSlug,
    here: tab.kind === `session` ? tab.sessionId : root.id,
    back: search.back,
  })
  /** #EXP-1464 (chat-row-action.json `open`): a conversation row's
   *  session opens by inbox-route.json `sessionTarget`: a child of THIS
   *  chat in place, anything else (a chat the agent started, a child of
   *  an agent conversation) on its own route. */
  const openRowSession = (sessionId: string) =>
    !root.agentId && sessionId !== root.id && rowsById.has(sessionId) ? openSession(sessionId) : nav.open(sessionId)
  // EXP-1334: the linked issues' statuses (the strip's chip / group rows).
  // S-3: the status resolves through the team's statuses (statusId first);
  // an issue row that has not synced draws NO glyph, never a Backlog one.
  const issueStatus = (issue: Issue | undefined) => {
    if (!issue) return null
    const status = resolve(issue)
    return {
      icon: status.icon,
      colorClass: statusColorClass(status),
      colorHex: status.builtinKey ? undefined : status.colorHex,
    }
  }

  // #EXP-1433: the strip's issues = every linked issue (a chip's sub-issues
  // after it) that has synced; the issue context menu rides the chip.
  const stripIssues: ChatStripIssue[] = issueTabs.flatMap((entry) =>
    [{ issueId: entry.issueId, identifier: entry.identifier }, ...entry.nested].flatMap((ref) => {
      const issue = issuesById.get(ref.issueId)
      if (!issue) return []
      return [
        {
          issueId: ref.issueId,
          identifier: ref.identifier,
          title: issue.title,
          status: issueStatus(issue),
          boxProps: { ...menuProps(CHAT_ISSUE_MENU, ref.issueId) },
        },
      ]
    })
  )

  // #EXP-1433: the strip's sessions = THE tree rows of every session under
  // the dispatcher (its children and theirs), SessionRow small + the guides.
  const treeRows = useMemo<ChatTreeRow[]>(() => {
    const nowIso = new Date().toISOString()
    return treeOrder(rows.map(toTreeSession)).map((session) => {
      const card = sessionCard(session, titles.get(session.id) ?? ``, nowIso)
      return {
        sessionId: session.id,
        parentId: session.parentId,
        name: card.name,
        agent: rowsById.get(session.id)?.cli ?? null,
        markState: sessionTabMarkState(session),
        stateWord: card.kindWord,
        stateTone: card.tone,
      }
    })
  }, [rows, rowsById, titles])
  const stripSessionIds = new Set(
    sessionTabs.flatMap((entry) => [entry.sessionId, ...entry.nested.map((nested) => nested.sessionId)])
  )
  const stripSessions = treeRows.filter((row) => stripSessionIds.has(row.sessionId))

  // What the strip has open in place (the main session = the Conversation).
  const stripOpen: ChatStripOpen =
    tab.kind === `issue`
      ? { kind: `issue`, id: tab.issueId }
      : tab.kind === `session` && tab.sessionId !== mainSession?.id
        ? { kind: `session`, id: tab.sessionId }
        : null
  const { mode: stripMode } = chatStripMode({ issues: stripIssues.length, sessions: stripSessions.length })
  const singleIssue = stripMode === `chips` ? (stripIssues[0]?.issueId ?? null) : null

  // EXP-1343 / #EXP-1433: phones page Conversation · (the one issue | the
  // item open from the group) · Results (swipe AND tap); a tap slides the
  // new page in from the side it sits on.
  const agentChat = chatMode(root) === `bubbles`
  const pages = useMemo(
    () =>
      agentChat
        ? (agentChatTabs(tabPlan !== null) as ChatPage[])
        : chatPages({ single: singleIssue, open: stripOpen ? chatPageOf(tab) : null, plan: tabPlan !== null }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [agentChat, singleIssue, stripOpen?.kind, stripOpen?.id, tabPlan !== null]
  )
  const page = chatPageOf(tab)
  const showPage = (next: ChatPage) => {
    if (isMobile) {
      // The pages around the move: a page opened from the group joins them.
      const item = next === `conversation` || next === `results` || next === `plan` ? (stripOpen ? page : null) : next
      primeTabEnter(agentChat ? pages : chatPages({ single: singleIssue, open: item, plan: tabPlan !== null }), page, next)
    }
    go(chatPageSearch(next))
  }
  const swipe = useFaceSwipe(pages, page, showPage)

  // #EXP-1562 (waiting-cards.json CHIPS): the WAITING cards' chips; a pick
  // shows the Conversation, scrolls the card to the centre and rings it.
  const waitingCards = useMemo(
    // #EXP-1662: a plan's chip is the plan tab; these are the questions.
    () =>
      chatPendingAsks(view.rows, () => null)
        .filter((ask) => ask.kind !== `plan`)
        .map(({ id, kind, text }) => ({ id, kind, text })),
    [view.rows]
  )
  const [scrollTo, setScrollTo] = useState<{ id: string; at: number } | null>(null)
  const [highlight, setHighlight] = useState<string | null>(null)
  const onConversation =
    tab.kind === `conversation` || (mainSession !== null && tab.kind === `session` && tab.sessionId === mainSession.id)
  const selectWaiting = (id: string) => {
    if (!onConversation) {
      if (mainSession) go({})
      else showPage(`conversation`)
    }
    setScrollTo({ id, at: Date.now() })
  }
  useEffect(() => {
    if (!scrollTo) return
    // The Conversation may still be mounting: look for the card a few frames.
    let frames = 0
    let raf = 0
    const find = () => {
      const card = document.querySelector(`[data-card-id="${CSS.escape(scrollTo.id)}"]`)
      if (card) {
        card.scrollIntoView({ block: `center`, behavior: `smooth` })
        setHighlight(scrollTo.id)
      } else if (frames++ < 60) raf = requestAnimationFrame(find)
    }
    raf = requestAnimationFrame(find)
    return () => cancelAnimationFrame(raf)
  }, [scrollTo])
  // #FEED-191 (needs-you.json): `?card=` runs the same scroll once the card's
  // row is here, then leaves the URL. A card (or `?plan=`) older than the loaded
  // page pages it in.
  const deepLink = search.card ?? (search.tab === `plan` ? search.plan : undefined)
  const deepLinkHere = deepLink ? view.rows.some((row) => row.id === deepLink) : false
  useEffect(() => {
    if (!search.card || !deepLinkHere) return
    selectWaiting(search.card)
    const { card: _card, ...rest } = search
    onNavigate(rest, { replace: true })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search.card, deepLinkHere])
  useEffect(() => {
    if (deepLink && !deepLinkHere && messagePage.ready && messagePage.more) messagePage.loadMore()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deepLink, deepLinkHere, messagePage.ready, messagePage.more, messagePage.loaded])
  useEffect(() => {
    if (!highlight) return
    const timer = setTimeout(() => setHighlight(null), WAITING_CARDS_CONSTANTS.highlightMs)
    return () => clearTimeout(timer)
  }, [highlight, scrollTo])
  /** A pick swaps the tab in place (a page on phones). */
  const pickSession = (sessionId: string) => showPage(`session:${sessionId}`)
  const pickIssue = (issueId: string) => showPage(`issue:${issueId}`)

  const firstIssue = issueTabs[0] ? issuesById.get(issueTabs[0].issueId) : undefined
  const menu = useChatTabMenu({
    rootId: root.id,
    teamSlug,
    issueIds,
    newIssueBoardId: firstIssue?.boardId,
    canEdit: permissions.isMember,
  })

  // RESULTS TAB (results-tab.json): every result of the tree.
  const results = useResultsTab(team.id, agentChat ? null : root.id)
  // EXP-1686 (chats.json AGENT CHAT): an agent's chat is the conversation
  // only: no strip, but for Conversation and the plan tab while it holds a plan.
  const tabs = agentChat && !agentChatTabs(tabPlan !== null).includes(`plan`) ? null : (
    <ChatTabs
      variant={isMobile ? `pager` : `bar`}
      conversation={{
        label: CHAT_COPY.conversation,
        // NEW-46: on a single-session chat the Conversation tab IS that
        // session's work in place (`mainSessionId`), active while it shows.
        active:
          tab.kind === `conversation` ||
          (mainSession !== null && tab.kind === `session` && tab.sessionId === mainSession.id),
        onSelect: () => (mainSession ? go({}) : showPage(`conversation`)),
      }}
      issues={agentChat ? [] : stripIssues}
      add={agentChat ? undefined : menu.addMenu}
      sessions={agentChat ? [] : stripSessions}
      open={stripOpen}
      onOpenIssue={pickIssue}
      onOpenSession={pickSession}
      plan={
        tabPlan
          ? {
              label: planRowLabel((shownPlan ?? tabPlan).plan),
              active: tab.kind === `plan`,
              waiting: (shownPlan ?? tabPlan).decision === null,
              decision: (shownPlan ?? tabPlan).decision,
              onSelect: () => showPage(`plan`),
            }
          : undefined
      }
      waiting={agentChat ? undefined : { cards: waitingCards, onSelect: selectWaiting }}
      results={
        agentChat
          ? undefined
          : {
              label: RESULTS_TAB.copy.label,
              active: tab.kind === `results`,
              onSelect: () => showPage(`results`),
            }
      }
    />
  )

  // ── The bodies ─────────────────────────────────────────────────────────────
  let body: ReactNode
  if (tab.kind === `session` && sessionInPlace) {
    body = (
      <InPlaceSession
        row={sessionInPlace}
        topInset={topInset}
        floating={isMobile}
        origin={view.origin}
        onOpenXref={nav.open}
      />
    )
  } else if (tab.kind === `results`) {
    body = (
      <div
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
        data-testid="chat-results-scroll"
        style={topInset ? { paddingTop: topInset } : undefined}
      >
        <div className="mx-auto w-full max-w-3xl px-4 pt-6 pb-10 md:px-5">
          <ChatResults
            tab={results}
            teamSlug={teamSlug}
            pullRequest={guidePr}
            canMerge={permissions.isMember}
            review={search.section === `all`}
            file={search.file ?? null}
            onReview={(open, file) =>
              go({ tab: `results`, ...(open ? { section: `all` as const } : {}), ...(open && file ? { file } : {}) })
            }
            onSurfaceReply={
              permissions.isMember
                ? (result, text) =>
                    void sendSessionMessage({ teamId: team.id, sessionId: result.sessionId, text }, SKIP_ERROR_TOAST).catch(
                      (error) => failedAction(`send`, `the message`, error)
                    )
                : undefined
            }
          />
        </div>
      </div>
    )
  } else if (tab.kind === `plan` && shownPlan) {
    body = (
      <ChatPlanBody
        row={shownPlan}
        mobile={isMobile}
        canDecide={permissions.isMember}
        topInset={topInset}
        // plan-page.json: a decision taken ON the tab returns to the
        // Conversation, where the agent works on.
        onDecided={() => (mainSession ? go({}) : showPage(`conversation`))}
      />
    )
  } else if (tab.kind === `issue`) {
    const issue = issuesById.get(tab.issueId)
    const board = issue ? boards.find((row) => row.id === issue.boardId) : undefined
    body =
      issue && board ? (
        <div className="min-h-0 flex-1">
          <IssueDetailView
            key={issue.id}
            issue={issue}
            users={users as User[]}
            board={board}
            teamSlug={teamSlug}
            teamId={team.id}
            readOnly={!permissions.canMutateIssue(issue)}
            embedded
            topInset={topInset}
          />
        </div>
      ) : (
        <div className="p-6">
          <SkeletonRows />
        </div>
      )
  } else {
    body = (
      <ConversationBody
        teamId={team.id}
        view={view}
        root={root}
        users={users as User[]}
        canReply={permissions.isMember}
        busy={root.busy}
        onOpenSession={openRowSession}
        onOpenXref={nav.open}
        onOpenPlan={(id) => go({ tab: `plan`, plan: id })}
        agentFor={agentFor}
        tree={rows}
        topInset={topInset}
        messagePage={messagePage}
      />
    )
  }

  // EXP-1307/1316: the header's `…` (Close PR, EXP-1338) on EVERY tab
  // while the chat has an open PR. Chat 58cb32df: the same `…` holds
  // THE chat menu (Stop · Stop and delete | Delete, chats.json MENUS).
  const chatActions = useChatActions(teamSlug)
  const headerMenu = (className?: string) =>
    agent ? agent.headerMenu(className) : (
    <ChatScreenHeaderMenu
      root={root}
      title={view.title}
      pr={guidePr}
      canClosePr={permissions.isMember}
      isOwner={permissions.isOwner}
      actions={chatActions}
      className={className}
    />
  )

  // EXP-1316: the title is derived (`sessionTitle`), never renamed.
  const title = (
    <div className="flex min-w-0 items-center gap-2">
      {agent?.glyph}
      <h1
        data-testid="chat-title"
        className="min-w-0 truncate text-2xl font-semibold tracking-tight"
      >
        {agent?.name ?? view.title}
      </h1>
      {view.private && (
        <PrivateIcon
          role="img"
          aria-label="Private"
          data-testid="chat-private"
          className="size-4 shrink-0 text-muted-foreground"
        />
      )}
    </div>
  )

  const issueMenus = {
    [CHAT_ISSUE_MENU]: ({ target, open, onOpenChange }: MenuSessionProps) => {
      const entry = issueTabs.find((row) => row.issueId === target.id)
      if (!entry) return null
      const entries = menu.entries({ id: entry.issueId, identifier: entry.identifier }, () =>
        onOpenChange(false)
      )
      if (entries.length === 0) return null
      return (
        <Menu
          mode="pointer"
          anchor={target.anchor}
          open={open}
          onOpenChange={onOpenChange}
          returnFocus={target.origin}
          aria-label={entry.identifier}
          title={entry.identifier}
          contentClassName="w-64"
          entries={entries}
        />
      )
    },
  }

  if (isMobile) {
    const trailing = headerMenu(HEADER_BUTTON_CLASS)
    const headerNeedsYou =
      rows.some((row) => row.needsInput && row.status !== `ended`) ||
      chatPendingAsks(view.rows, () => null).length > 0
    const headerBusy = rows.some((row) => row.busy)
    const headerLead: ChatLead = headerNeedsYou
      ? `needs_you`
      : headerBusy
        ? `working`
        : (chatLeadPrState(pullRequests) ?? `waiting`)
    return (
      <IssueRefOutlet issueIds={issueIds} onOpen={pickIssue}>
      <MenuGestureHost menus={issueMenus}>
        {/* EXP-1316: THE detail chrome — the header band (blur, scrim, edge
            fade) floats OVER the body, which scrolls under it; every body's
            scroller pads its top by the band's measured height. */}
        <div
          className={MOBILE_DETAIL_SCREEN_CLASS}
          data-testid="chat-page"
          ref={swipe.ref}
          style={swipe.style}
          onTouchStart={swipe.onTouchStart}
          onTouchMove={swipe.onTouchMove}
          onTouchEnd={swipe.onTouchEnd}
          onTouchCancel={swipe.onTouchCancel}
        >
          <MobileDetailHeader
            overlay
            ref={headerRef}
            title={
              // Chats (the draft, iOS/Android): the lead disc, the title over
              // the state word · the device; an agent chat its disc + name.
              agent ? agent.mobileTitle : <ChatHeaderTitle
                lead={headerLead}
                agent={root.cli ?? null}
                title={view.title}
                trailing={
                  view.private ? (
                    <PrivateIcon
                      aria-label="Private"
                      data-testid="chat-private"
                      className="size-3.5 shrink-0 text-muted-foreground"
                    />
                  ) : undefined
                }
                device={deviceGlyph(root.deviceId)?.name ?? null}
                needsYou={headerNeedsYou}
                busy={headerBusy}
                needsYouWord={CHAT_ROW_WORDS.needsInput ?? ``}
              />
            }
            onBack={nav.pop ?? (() => void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug } }))}
            menu={
              <span className="flex shrink-0 items-center gap-1">
                {/* S-3: the Merge pill moved up from the retired bottom
                    capsule into the header, like md+. */}
                <ChatGuideMerge pr={guidePr} canMerge={permissions.isMember} />
                {trailing}
              </span>
            }
            below={tabs ? <div className="px-3 pb-2">{tabs}</div> : undefined}
          />
          {/* The page under the band: only the visible one renders. */}
          <div
            data-face-body=""
            data-testid="chat-page-body"
            className={cn(`flex min-h-0 flex-1 flex-col`, TAB_BODY_TOUCH_CLASS)}
          >
            {agent?.overlay}
            <CardHighlightContext.Provider value={highlight}>{body}</CardHighlightContext.Provider>
          </div>
        </div>
      </MenuGestureHost>
      {chatActions.dialog}
      </IssueRefOutlet>
    )
  }

  return (
    <IssueRefOutlet issueIds={issueIds} onOpen={pickIssue}>
    <MenuGestureHost menus={issueMenus}>
      <div className="flex h-full min-h-0 flex-col" data-testid="chat-page">
        <div className="shrink-0 border-b border-border px-6 pt-6 pb-3 md:px-10">
          <div className="flex items-center gap-3">
            {/* plan-page.json: on the Plan tab the Conversation tab is the ONE
                way back. */}
            {tab.kind !== `plan` && <BackStackButton onPop={nav.pop} className="-ml-2" />}
            <div className="min-w-0 flex-1">{title}</div>
            <ChatGuideMerge pr={guidePr} canMerge={permissions.isMember} />
            {headerMenu(`shrink-0 text-muted-foreground`)}
          </div>
          {tabs && <div className="mt-4">{tabs}</div>}
        </div>
        {agent?.overlay}
        <CardHighlightContext.Provider value={highlight}>{body}</CardHighlightContext.Provider>
      </div>
    </MenuGestureHost>
    {chatActions.dialog}
    </IssueRefOutlet>
  )
}

/** #EXP-1662: the plan tab's body (plan-page.json). */
function ChatPlanBody({
  row,
  mobile,
  canDecide,
  topInset,
  onDecided,
}: {
  row: Extract<SessionChatRow, { type: `plan` }>
  mobile: boolean
  canDecide: boolean
  topInset: number
  onDecided: () => void
}) {
  const { answer, pending } = useAnswerMessage()
  return (
    <PlanPage
      key={row.id}
      className="min-h-0 flex-1"
      row={row}
      mobile={mobile}
      renderText={renderChatText}
      busy={pending.has(row.id)}
      onDecide={
        canDecide
          ? (input) =>
              void answer(row.id, input).then((answered) => {
                if (answered) onDecided()
              })
          : undefined
      }
      topInset={topInset}
    />
  )
}

function ConversationBody({
  teamId,
  view,
  root,
  users,
  canReply,
  busy,
  onOpenSession,
  onOpenXref,
  onOpenPlan,
  agentFor,
  tree,
  topInset = 0,
  messagePage,
}: {
  teamId: string
  view: SessionChatView
  /** The root's row (the live tail reads its relay state). */
  root: SessionRow
  users: User[]
  canReply: boolean
  /** The ROOT's busy: Stop stops the root, so a busy child never shows it. */
  busy: boolean
  /** #EXP-1662: a plan row opens the plan tab on that plan. */
  onOpenPlan: (messageId: string) => void
  /** A session card opens that session in place (EXP-1316). */
  onOpenSession: (sessionId: string) => void
  /** S-3: an xref opens its session on the back stack. */
  onOpenXref: (sessionId: string) => void
  /** The agent CLI of a session of the tree (the suggestions' models). */
  agentFor: (sessionId: string) => string | null | undefined
  /** #EXP-1545: every session of the tree (a Live child's Queue docks). */
  tree: readonly SessionRow[]
  /** Phones: the overlay header band's height (the scroller pads by it). */
  topInset?: number
  /** #EXP-1380: the loaded newest messages (Load earlier pages more). */
  messagePage: MessagePage
}) {
  const messages = useSessionMessages(view.rootId)
  const messagesById = useMemo(
    () => new Map(messages.map((message: MessageRow) => [message.id, message])),
    [messages]
  )
  const usersById = useMemo(() => new Map(users.map((user) => [user.id, user])), [users])
  // #EXP-1736 (transcript-grouping.json): the conversation's own sender
  // (an agent's chat = the agent's name; else the chat's title) and its
  // members' names.
  const { agent: agentRow } = useAgentRow(root.agentId)
  const chatSender = useMemo(
    () => ({ sessionId: root.id, title: (root.agentId ? agentRow?.name : null) ?? view.title, agentId: root.agentId ?? null }),
    [root.id, root.agentId, agentRow?.name, view.title]
  )
  const memberOf = useCallback((id: string) => usersById.get(id) ?? null, [usersById])
  const { answer, pending } = useAnswerMessage()
  const unqueue = useUnqueueMessage()
  const { data: session } = useSession()
  // EXP-1316 (1n): the session feed's scroll — opens at the newest row,
  // follows while pinned, THE jump button once scrolled up.
  // Keyed on the row count + the last row's id, not `view.rows`: the view is
  // rebuilt every second (useNow) and an identity key would re-run the
  // follow effect each tick.
  const lastRowId = view.rows.at(-1)?.id ?? null
  const follow = useFollowScroll([view.rows.length, lastRowId])
  // #EXP-1380 (chats.json WINDOW): the newest rows, `Load earlier` widens.
  const transcript = useTranscriptWindow(view.rows, follow.scrollRef, { resetKey: view.rootId, ...messagePage })
  const isMobile = useIsMobile()
  const xrefAgent = useXrefAgent()
  // S-3 (session-chat.json `chatModeCases`, chats.json CHAT MODE): an
  // agent conversation paints chat bubbles on every form factor; a chat
  // keeps its transcript, on every form factor too (EXP-1676).
  const bubbles = chatMode(root) === `bubbles`
  const suggestions = useSuggestionActions({ teamId, enabled: canReply, agentFor })
  // The Dock holds the open suggestions; S-3: a waiting plan or question is
  // never docked and stays inline answerable.
  const dockedIds = useMemo(
    () => (canReply ? chatDockedIds(view.rows, () => null) : new Set<string>()),
    [canReply, view.rows]
  )
  const sessionName = useMemo(() => chatSessionNames(view.tabs), [view.tabs])
  // N-12: THE Dock over the Conversation's bottom edge: the Queue (the
  // root's, else a Live child's, #EXP-1545), the root's
  // queued messages, banners and open suggestions, then the composer
  // (a phone's floats in the work bar); the scroller pads by its height.
  const dockChildren = useMemo(
    () =>
      tree
        .filter((row) => row.id !== root.id)
        .map((row) => ({ session: row, name: sessionName(row.id) ?? row.title ?? `` })),
    [tree, root.id, sessionName]
  )
  const sessionDock = useSessionDock(root, dockChildren)
  const sendSurfaceReply = (text: string) => {
    sendSessionMessage({ teamId, sessionId: view.composer.sessionId, text }, SKIP_ERROR_TOAST)
      .catch((error) => failedAction(`send`, `the message`, error))
  }
  const [dockHeight, setDockHeight] = useState(0)
  // FEED-140: the dock's band grows the transcript's bottom padding AFTER
  // the first pin to the newest row; a pinned reader stays pinned, so the
  // last card's actions (an inline plan's decision) never sit under it.
  const { atBottom, jumpToBottom } = follow
  useEffect(() => {
    if (atBottom) jumpToBottom()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dockHeight])
  const composer = canReply ? (
    <SessionComposer
      teamId={root.teamId}
      browseTeamId={teamId}
      sessionId={view.composer.sessionId}
      placeholder={view.composer.placeholder}
      users={users}
      busy={busy}
      canStop
      opaque={isMobile}
      hideReplies={isMobile}
      testId="chat-composer"
      surface={root.agentId ? `agent` : `session`}
    />
  ) : null

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className="relative min-h-0 flex-1">
      <div
        ref={follow.scrollRef}
        onScroll={follow.onScroll}
        data-testid="chat-scroll"
        className="h-full overflow-y-auto overscroll-contain"
        style={topInset ? { paddingTop: topInset } : undefined}
      >
        <div
          ref={follow.contentRef}
          className={cn(CHAT_COLUMN_CLASS, `px-3 pt-8 md:px-5`)}
          style={{ paddingBottom: dockHeight + 16 }}
        >
          {transcript.canLoadEarlier ? <LoadEarlierButton onClick={transcript.loadEarlier} /> : null}
          <ChatConversation
            rows={transcript.rows}
            renderText={renderChatText}
            attachmentSrc={attachmentSrc}
            chat={chatSender}
            member={memberOf}
            fromStart={!transcript.canLoadEarlier}
            onOpenSession={onOpenSession}
            onOpenXref={onOpenXref}
            xrefAgent={xrefAgent}
            rootId={root.id}
            WaitingCard={SessionWaitingCard}
            origin={view.origin}
            onAnswer={canReply ? (id, input) => void answer(id, input) : undefined}
            onDecide={canReply ? (id, input) => void answer(id, input) : undefined}
            onDismiss={canReply ? (id) => void answer(id, { dismissed: true }) : undefined}
            onOpenPlan={(plan) => onOpenPlan(plan.id)}
            pending={pending}
            docked={dockedIds}
            viewerId={session?.user?.id ?? null}
            onUnqueue={(id) => void unqueue(id)}
            // Chat 58cb32df (chats.json TRANSCRIPT): phones paint chat
            // bubbles; a surface card's reply posts as the next message.
            layout={bubbles ? `bubbles` : `transcript`}
            messageTime={(row) => {
              const message = messagesById.get(row.id)
              return message ? userMessageTime(new Date(message.createdAt)) : null
            }}
            onSurfaceReply={canReply ? sendSurfaceReply : undefined}
            suggestions={suggestions}
            renderIssueRef={(_issueId, identifier) =>
              identifier ? <FiledIssueRef identifier={identifier} /> : null
            }
            tail={
              view.liveSessionId === root.id ? <SessionLiveTail session={root} /> : undefined
            }
          />
        </div>
      </div>
      <JumpToBottomButton
        visible={!follow.atBottom && view.rows.length > 0}
        className="absolute left-1/2 z-30 -translate-x-1/2"
        style={{ bottom: dockHeight + JUMP_TO_BOTTOM_GAP }}
        onClick={follow.jumpToBottom}
      />
      {canReply && (
        <ChatDock
          rows={view.rows}
          sessionName={sessionName}
          renderText={renderChatText}
          pending={pending}
          onAnswer={(id, input) => void answer(id, input)}
          onDecide={(id, input) => void answer(id, input)}
          sources={sessionDock.sources}
          renderSheet={sessionDock.renderSheet}
          renderAction={sessionDock.renderAction}
          suggestions={suggestions}
          onOpenSession={onOpenSession}
          renderIssueRef={(_issueId, identifier) =>
            identifier ? <FiledIssueRef identifier={identifier} /> : null
          }
          composer={isMobile ? undefined : composer}
          onHeight={setDockHeight}
          phoneClearance={isMobile}
        />
      )}
      </div>
      {/* EXP-1316: phones reply through THE floating composer capsule. */}
      {composer && isMobile && (
        <FloatingComposer
          placeholder={view.composer.placeholder}
          above={<SessionComposerReplies sessionId={view.composer.sessionId} />}
        >
          {composer}
        </FloatingComposer>
      )}
    </div>
  )
}

/** #EXP-1464: an expanded session row's waiting card off the synced data. */
function SessionWaitingCard({
  sessionId,
  children,
}: {
  sessionId: string
  children: (row: WaitingCardRow | null | undefined) => ReactNode
}) {
  return <>{children(useSessionWaitingCard(sessionId))}</>
}

/** EXP-1323: a filed suggestion's issue = THE issue chip once it resolves
 *  (same team, synced), else its identifier. */
function FiledIssueRef({ identifier }: { identifier: string }) {
  const issueRefs = useIssueRefs()
  const resolved = issueRefs?.resolve(identifier) ?? null
  if (!resolved) return <>{identifier}</>
  return <IssueRefChip issue={resolved} onClick={() => issueRefs?.open(resolved.identifier)} />
}
