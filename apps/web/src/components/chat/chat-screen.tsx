import { chatPendingAsks, chatSessionNames } from "@exp/domain-contract/ask-dock"
import { WAITING_CARDS_CONSTANTS } from "@exp/domain-contract/waiting-cards"
import type { ChatLead } from "@exp/domain-contract/chats"
import {
  SESSION_SCREEN_COPY,
  sessionScreen,
  sessionScreenKind,
  sessionScreenStatus,
  sessionScreenSubtitle,
  sessionScreenTone,
  type SessionScreenOpen,
  type SessionScreenTab,
} from "@exp/domain-contract/session-screen"
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import {
  CardHighlightContext,
  ChatLeadDisc,
  conceptIcon,
  Menu,
  MenuGestureHost,
  SkeletonRows,
  chatLeadPrState,
  useIsMobile,
  type MenuSessionProps,
  PlanPage,
} from "@exp/ui"
import type { Issue, User } from "@/db/schema"
import { IssueChip as IssueRefChip } from "@/components/issue-chip"
import { IssueRefOutlet, useIssueRefs } from "@/components/issue-ref-provider"
import { useSuggestionActions } from "@/hooks/use-suggestion-actions"
import { IssueDetailView } from "@/components/issue-detail-view"
import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { ChatGuideMerge, chatGuidePullRequest } from "@/components/chat/chat-guide"
import { useResultsTab } from "@/hooks/use-results-tab"
import { useAnswerMessage } from "@/hooks/use-answer-message"
import { renderChatText } from "@/components/chat/chat-markdown"
import { ChatScreenHeaderMenu, useChatTabMenu } from "@/components/chat/chat-tab-menu"
import { useChatActions } from "@/components/chat/chat-menu"
import { CHAT_HEADER_TONE_CLASS, HEADER_DISC_CLASS } from "@/components/chat/chat-header"
import { SessionScreenFrame } from "@/components/chat/session-screen-frame"
import {
  SESSION_ISSUE_MENU,
  SessionIssuesTab,
  SessionResultsTab,
  SessionSessionsTab,
  useSessionTreeRows,
  type SessionIssueEntry,
} from "@/components/chat/session-screen-bodies"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { InPlaceSession } from "@/components/chat/session-body"
import { useSessionNav } from "@/hooks/use-session-nav"
import type { ChatSearch } from "@/lib/session-screen-search"
import { useMarkRead, useNewestAgentMessageAt } from "@/hooks/use-mark-read"
import type { MessagePage } from "@/hooks/use-session-tree"
import { useTeamBoards, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import {
  type PullRequestRow,
  type SessionRow,
  type SessionChatView,
} from "@/lib/session-tree"
import type { SessionChatRow } from "@exp/domain-contract/session-tree"
import { PLAN_PAGE_COPY, planTabTarget } from "@exp/domain-contract/plan-page"

// #EXP-1732 (fixtures/session-screen.json): THE session screen for a chat
// (a root session), a team agent's conversation and the Assistant's. The
// frame (`session-screen-frame.tsx`) draws the header (the lead avatar,
// the title over ONE subtitle line, the merge pill, THE gear, the `…`) and
// THE SegmentedControl of `sessionScreen().tabs` (Chat · Issues · Sessions
// · Results with their counts); this host builds the bodies:
//
//   chat       the ROOT's own session view (`InPlaceSession`: its raw
//              work + traces, its cards inline, THE Dock, THE composer),
//              for every kind
//   issues     the tree's issues (nested indented), led by the `+`; a row
//              opens that issue IN PLACE (`open=issue&issue=`)
//   sessions   THE tree rows of every session under the root; a row opens
//              that session IN PLACE (`open=session&session=`)
//   results    results-tab.json's body
//   settings   the gear (agents, the Assistant): the agent settings body
//
// Wide (≥ 768px) = the beside rule (the chat pane : the tab pane); phones =
// one pane. The chat route (`/t/$teamSlug/sessions/$sessionId`) and an
// agent's (`/t/$teamSlug/agents/$agentId`) both render it; the route owns
// the search and how it navigates.

export { validateChatSearch, type ChatSearch } from "@/lib/session-screen-search"

const PrivateIcon = conceptIcon(`ui-private`)

/** #EXP-1732: an agent's (or the Assistant's) header parts and settings,
 *  the route's. */
export interface ChatScreenAgent {
  name: string
  assistant: boolean
  /** The agent's disc (the Assistant: THE brand mark). */
  avatar: ReactNode
  /** The agent's first enabled trigger's `triggerSummary`, else null. */
  trigger: string | null
  /** Before the gear (the call button). */
  trailing?: ReactNode
  /** THE agent menu (the `…`). */
  menu: (className: string) => ReactNode
  /** Over the body (the voice call card). */
  overlay?: ReactNode
  /** The gear's body: the agent settings page body. */
  settings: ReactNode
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
  /** The route's search (the chat's or the agent chat's). */
  search: ChatSearch
  /** Navigate the SAME screen to `next` (replace = no history entry). */
  onNavigate: (next: ChatSearch, options?: { replace?: boolean }) => void
  /** An agent conversation's header parts + settings; absent = a chat. */
  agent?: ChatScreenAgent
  team: Team
  teamSlug: string
  view: SessionChatView
  root: SessionRow
  /** Every session of the tree (the Sessions tab, the counts). */
  rows: SessionRow[]
  /** EXP-1341: every tree session's `sessionTitle` (the rows' names). */
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

  const issueTabs = view.tabs.filter(
    (tab): tab is Extract<SessionChatView[`tabs`][number], { kind: `issue` }> =>
      tab.kind === `issue`
  )
  // EXP-1334: every linked issue, the top-level ones AND their folded
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
  // EXP-1323: opening the chat marks its root read (again whenever a newer
  // agent reply lands anywhere in the tree while it is open); an opened
  // session marks itself (`session-body.tsx`).
  const treeIds = useMemo(() => rows.map((row) => row.id), [rows])
  useMarkRead(root.id, useNewestAgentMessageAt(treeIds))
  // #EXP-1662 (plan-page.json): the chat's plans; a plan row opens one.
  const planRows = useMemo(
    () => view.rows.filter((row): row is Extract<SessionChatRow, { type: `plan` }> => row.type === `plan`),
    [view.rows]
  )

  // RESULTS TAB (results-tab.json): every result of the tree.
  const results = useResultsTab(team.id, root.id)
  const sessionRows = useSessionTreeRows(rows, titles, root.id)

  // ── THE screen (session-screen.json) ──────────────────────────────────────
  const openIssueId =
    search.open === `issue` && search.issue && issueIds.includes(search.issue) ? search.issue : null
  // #EXP-1732: the root's own work IS the Chat tab; `open=session` naming
  // the root is just that.
  const openSessionRow = search.open === `session` && focused && focused.id !== root.id ? focused : null
  const openPlan =
    search.open === `plan`
      ? (planRows.find((row) => row.id === search.plan) ??
        planRows.find((row) => row.id === planTabTarget(planRows, null)) ??
        null)
      : null
  const requestedOpen: SessionScreenOpen | null = openIssueId
    ? `issue`
    : openSessionRow
      ? `session`
      : openPlan
        ? `plan`
        : search.open === `settings` && agent
          ? `settings`
          : null
  const kind = sessionScreenKind({ agentId: agent ? root.agentId : null, appId: null, assistant: agent?.assistant ?? false })
  const screen = sessionScreen({
    kind,
    wide: !isMobile,
    requested: search.tab ?? null,
    open: requestedOpen,
    counts: { issues: issueIds.length, sessions: sessionRows.length, results: results.results.length },
  })

  // S-3: the back stack rides along every move; an xref pushes it.
  const keepBack = search.back ? { back: search.back } : {}
  const pickTab = (tab: SessionScreenTab) => onNavigate({ tab, ...keepBack }, { replace: true })
  /** An item opens over the tab (a pushed history entry: Back returns). */
  const openItem = (next: ChatSearch) => onNavigate({ tab: screen.active, ...next, ...keepBack })
  /** The opened item's `‹`: back to the tab it came from. */
  const closeItem = () => onNavigate({ tab: screen.active, ...keepBack }, { replace: true })
  // session-screen.json ACTIVE FOLLOWS THE OPENED ITEM: `sessionScreen`
  // makes Sessions (Issues) active while a session (an issue) is open.
  const openSession = (sessionId: string) => openItem({ open: `session`, session: sessionId })
  const openIssue = (issueId: string) => openItem({ open: `issue`, issue: issueId })
  /** S-3: the session on show (an xref pushes it; `‹` pops back to it). */
  const nav = useSessionNav({
    teamSlug,
    here: openSessionRow?.id ?? root.id,
    back: search.back,
  })
  /** #EXP-1464 (chat-row-action.json `open`): a conversation row's session
   *  of THIS tree opens in place; anything else (a chat the agent started)
   *  on its own route. */
  const openRowSession = (sessionId: string) =>
    sessionId !== root.id && rowsById.has(sessionId) ? openSession(sessionId) : nav.open(sessionId)

  // S-3: an issue's status resolves through the team's statuses (statusId
  // first); an issue row that has not synced draws NO glyph, never Backlog.
  const issueStatus = (issue: Issue | undefined) => {
    if (!issue) return null
    const status = resolve(issue)
    return {
      icon: status.icon,
      colorClass: statusColorClass(status),
      colorHex: status.builtinKey ? undefined : status.colorHex,
    }
  }
  const issueEntries: SessionIssueEntry[] = issueTabs.flatMap((entry) =>
    [{ issueId: entry.issueId, identifier: entry.identifier, nested: false }, ...entry.nested.map((n) => ({ ...n, nested: true }))].map(
      (ref) => {
        const issue = issuesById.get(ref.issueId)
        return {
          issueId: ref.issueId,
          identifier: ref.identifier,
          title: issue?.title ?? null,
          status: issueStatus(issue),
          nested: ref.nested,
        }
      }
    )
  )

  // #EXP-1562 / #FEED-191 (needs-you.json): `?card=` scrolls the Chat tab
  // (the root's session view: its cards carry `data-card-id`) to that card
  // and rings it (phones: the Chat tab first).
  const [scrollTo, setScrollTo] = useState<{ id: string; at: number } | null>(null)
  const [highlight, setHighlight] = useState<string | null>(null)
  useEffect(() => {
    if (!scrollTo) return
    // The session view may still be mounting: look for the card a few frames.
    let frames = 0
    let raf = 0
    const find = () => {
      const card = document.querySelector(`[data-card-id="${CSS.escape(scrollTo.id)}"]`)
      if (card) {
        card.scrollIntoView({ block: `center`, behavior: `smooth` })
        setHighlight(scrollTo.id)
      } else if (frames++ < 120) raf = requestAnimationFrame(find)
    }
    raf = requestAnimationFrame(find)
    return () => cancelAnimationFrame(raf)
  }, [scrollTo])
  // A card (or `?plan=`) older than the loaded page pages it in.
  const deepLink = search.card ?? (search.open === `plan` ? search.plan : undefined)
  const deepLinkHere = deepLink ? view.rows.some((row) => row.id === deepLink) : false
  useEffect(() => {
    if (!search.card || !deepLinkHere) return
    setScrollTo({ id: search.card, at: Date.now() })
    const { card: _card, ...rest } = search
    onNavigate(isMobile && screen.layout !== `chat` ? { tab: `chat`, ...keepBack } : rest, { replace: true })
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

  // EXP-1323 / #EXP-1545: the Chat tab's suggested tasks (Start, Start all,
  // Do it here, Start on… / with…, File, Dismiss), a Live child's Queue in
  // THE Dock, a filed suggestion's issue chip.
  const agentFor = useCallback(
    (sessionId: string) => rowsById.get(sessionId)?.cli ?? root.cli,
    [rowsById, root.cli]
  )
  const suggestions = useSuggestionActions({ teamId: team.id, enabled: permissions.isMember, agentFor })
  const sessionName = useMemo(() => chatSessionNames(view.tabs), [view.tabs])
  const dockChildren = useMemo(
    () =>
      rows
        .filter((row) => row.id !== root.id)
        .map((row) => ({ session: row, name: sessionName(row.id) ?? row.title ?? `` })),
    [rows, root.id, sessionName]
  )
  const treeIssues = useMemo(
    () => [...issuesById.values()].map((issue) => ({ id: issue.id, identifier: issue.identifier, title: issue.title })),
    [issuesById]
  )

  const firstIssue = issueTabs[0] ? issuesById.get(issueTabs[0].issueId) : undefined
  const menu = useChatTabMenu({
    rootId: root.id,
    teamSlug,
    issueIds,
    newIssueBoardId: firstIssue?.boardId,
    canEdit: permissions.isMember,
  })

  // ── The bodies ─────────────────────────────────────────────────────────────
  // #EXP-1732 (the owner's correction): the Chat tab IS the ROOT's own
  // session view (its raw work + trace rows, its cards inline, THE Dock,
  // THE composer) for every kind; descendants are Sessions rows.
  const chat = (
    <InPlaceSession
      key={root.id}
      row={root}
      floating={isMobile}
      origin={view.origin}
      onOpenXref={openRowSession}
      surface={root.agentId ? `agent` : `session`}
      browseTeamId={team.id}
      chat={{
        tree: rows,
        issues: treeIssues,
        dockChildren,
        suggestions,
        renderIssueRef: renderFiledIssue,
      }}
    />
  )

  let pane: ReactNode = null
  if (screen.active === `issues`) {
    pane = (
      <SessionIssuesTab issues={issueEntries} addMenu={menu.addMenu} activeId={openIssueId} onOpen={openIssue} />
    )
  } else if (screen.active === `sessions`) {
    pane = <SessionSessionsTab rows={sessionRows} activeId={openSessionRow?.id ?? null} onOpen={openSession} />
  } else if (screen.active === `results`) {
    pane = (
      <SessionResultsTab
        teamId={team.id}
        teamSlug={teamSlug}
        results={results}
        pullRequest={guidePr}
        canMerge={permissions.isMember}
        review={search.section === `all`}
        file={search.file ?? null}
        onReview={(open, file) =>
          onNavigate(
            { tab: `results`, ...(open ? { section: `all` as const } : {}), ...(open && file ? { file } : {}), ...keepBack },
            { replace: true }
          )
        }
      />
    )
  }

  let item: ReactNode = null
  /** Phones: the pushed item page's title. */
  let itemTitle = ``
  if (screen.open === `session` && openSessionRow) {
    itemTitle = titles.get(openSessionRow.id) ?? ``
    item = (
      <InPlaceSession
        key={openSessionRow.id}
        row={openSessionRow}
        floating={isMobile}
        origin={view.origin}
        onOpenXref={nav.open}
      />
    )
  } else if (screen.open === `plan` && openPlan) {
    itemTitle = PLAN_PAGE_COPY.title
    item = (
      <ChatPlanBody
        row={openPlan}
        mobile={isMobile}
        canDecide={permissions.isMember}
        // plan-page.json: a decision taken on the plan returns to where it
        // was opened from, where the agent works on.
        onDecided={closeItem}
      />
    )
  } else if (screen.open === `issue` && openIssueId) {
    const issue = issuesById.get(openIssueId)
    const board = issue ? boards.find((row) => row.id === issue.boardId) : undefined
    itemTitle = issue?.identifier ?? ``
    item =
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
          />
        </div>
      ) : (
        <div className="p-6">
          <SkeletonRows />
        </div>
      )
  } else if (screen.open === `settings` && agent) {
    itemTitle = SESSION_SCREEN_COPY.settings
    item = (
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain" data-testid="session-settings">
        <div className="mx-auto w-full max-w-3xl px-4 pt-4 pb-10">{agent.settings}</div>
      </div>
    )
  }

  // EXP-1307/1316: the header's `…` (Close PR, EXP-1338) while the chat has
  // an open PR, THE chat menu (Stop · Stop and delete | Delete); an agent's
  // = THE agent menu.
  const chatActions = useChatActions(teamSlug)
  const headerMenu = (className: string) =>
    agent ? (
      agent.menu(className)
    ) : (
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

  const needsYou =
    rows.some((row) => row.needsInput && row.status !== `ended`) || chatPendingAsks(view.rows, () => null).length > 0
  const lead: ChatLead = needsYou
    ? `needs_you`
    : rows.some((row) => row.busy)
      ? `working`
      : (chatLeadPrState(pullRequests) ?? `waiting`)
  const avatar = agent ? agent.avatar : <ChatLeadDisc lead={lead} agent={root.cli ?? null} className={HEADER_DISC_CLASS} />
  // session-screen.json STATUS WORD: THE tree's word, never dropped.
  const status = sessionScreenStatus(
    (rows.some((row) => row.id === root.id) ? rows : [root, ...rows]).map((row) => ({
      status: row.status,
      busy: row.busy,
      needsInput: row.needsInput,
      root: row.id === root.id,
    }))
  )
  const subtitle = sessionScreenSubtitle({
    status,
    device: deviceGlyph(root.deviceId)?.name ?? null,
    trigger: agent?.trigger ?? null,
  })

  const issueMenus = {
    [SESSION_ISSUE_MENU]: ({ target, open, onOpenChange }: MenuSessionProps) => {
      const entry = issueEntries.find((row) => row.issueId === target.id)
      if (!entry) return null
      const entries = menu.entries({ id: entry.issueId, identifier: entry.identifier }, () => onOpenChange(false))
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

  return (
    // EXP-1347: an issue pill of one of this chat's issues opens it in place.
    <IssueRefOutlet issueIds={issueIds} onOpen={openIssue}>
      <MenuGestureHost menus={issueMenus}>
          <CardHighlightContext.Provider value={highlight}>
          <SessionScreenFrame
            screen={screen}
            testId="chat-page"
            avatar={avatar}
            title={agent?.name ?? view.title}
            titleTrailing={
              view.private && !agent ? (
                <PrivateIcon
                  role="img"
                  aria-label="Private"
                  data-testid="chat-private"
                  className="size-3.5 shrink-0 text-muted-foreground"
                />
              ) : undefined
            }
            subtitle={subtitle}
            subtitleClass={CHAT_HEADER_TONE_CLASS[sessionScreenTone(status)]}
            trailing={
              <>
                {/* S-3: the Merge pill sits in the header on every width. */}
                <ChatGuideMerge pr={guidePr} canMerge={permissions.isMember} />
                {agent?.trailing}
              </>
            }
            onGear={agent ? () => openItem({ open: `settings` }) : undefined}
            menu={headerMenu}
            onBack={nav.pop ?? (() => void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug } }))}
            onPop={nav.pop}
            onTab={pickTab}
            chat={chat}
            pane={pane}
            item={item}
            onCloseItem={closeItem}
            itemTitle={itemTitle}
            overlay={agent?.overlay}
          />
          </CardHighlightContext.Provider>
      </MenuGestureHost>
      {chatActions.dialog}
    </IssueRefOutlet>
  )
}

/** #EXP-1662: an opened plan's body (plan-page.json). */
function ChatPlanBody({
  row,
  mobile,
  canDecide,
  onDecided,
}: {
  row: Extract<SessionChatRow, { type: `plan` }>
  mobile: boolean
  canDecide: boolean
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
    />
  )
}

/** EXP-1323: a filed suggestion's issue = THE issue chip once it resolves
 *  (same team, synced), else its identifier. */
const renderFiledIssue = (_issueId: string, identifier: string | null) =>
  identifier ? <FiledIssueRef identifier={identifier} /> : null

function FiledIssueRef({ identifier }: { identifier: string }) {
  const issueRefs = useIssueRefs()
  const resolved = issueRefs?.resolve(identifier) ?? null
  if (!resolved) return <>{identifier}</>
  return <IssueRefChip issue={resolved} onClick={() => issueRefs?.open(resolved.identifier)} />
}
