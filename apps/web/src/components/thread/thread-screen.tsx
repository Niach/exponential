import { threadPendingAsks, threadSessionNames } from "@exp/domain-contract/ask-dock"
import type { ChatLead } from "@exp/domain-contract/chats"
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import {
  AgentRunMark,
  conceptIcon,
  FloatingComposer,
  JumpToBottomButton,
  JUMP_TO_BOTTOM_GAP,
  LoadEarlierButton,
  useTranscriptWindow,
  Menu,
  MenuGestureHost,
  menuProps,
  sessionTabRunMark,
  SkeletonRows,
  StatusGlyph,
  threadLeadPrState,
  ThreadChipSubtree,
  ThreadDock,
  threadDockedIds,
  ThreadConversation,
  ThreadIssueTab,
  ThreadIssueTree,
  ThreadSessionTab,
  ThreadSessionTree,
  ThreadTabs,
  useIsMobile,
  userMessageCaption,
  userMessageTime,
  type MenuSessionProps,
  type ThreadIssueTreeRow,
  type ThreadStripItem,
  type ThreadTreeRow,
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
import { ThreadGuideMerge, threadGuidePullRequest } from "@/components/thread/thread-guide"
import { ThreadStories } from "@/components/thread/thread-stories"
import { useStoryTab } from "@/hooks/use-stories"
import { STORY_TAB } from "@exp/domain-contract/stories"
import { useAnswerMessage, useUnqueueMessage } from "@/hooks/use-answer-message"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { ThreadHeaderMenu, useThreadMenu } from "@/components/thread/thread-menu"
import { useChatActions } from "@/components/thread/chat-menu"
import { ChatHeaderTitle } from "@/components/thread/chat-header"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { BackStackButton, InPlaceSession } from "@/components/thread/session-body"
import { useSessionNav, useXrefAgent } from "@/hooks/use-session-nav"
import { backSearch } from "@/lib/session-back"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import { primeTabEnter, TAB_BODY_TOUCH_CLASS, useFaceSwipe } from "@/hooks/use-face-swipe"
import { useMarkRead, useNewestAgentMessageAt } from "@/hooks/use-mark-read"
import { useSuggestionActions } from "@/hooks/use-suggestion-actions"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useSession } from "@/hooks/use-session"
import { useSessionMessages, type MessagePage } from "@/hooks/use-session-tree"
import { useTeamBoards, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { threadPageOf, threadPages, threadPageSearch, type ThreadPage } from "@/lib/thread-pages"
import { cn } from "@/lib/utils"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import {
  sessionCard,
  sessionTabMarkState,
  THREAD_COPY,
  THREAD_ROW_WORDS,
  toTreeSession,
  treeOrder,
  type MessageRow,
  type PullRequestRow,
  type SessionRow,
  type SessionThreadView,
} from "@/lib/session-tree"
import { threadChatMode } from "@exp/domain-contract/session-tree"

// EXP-1293 Threads v2 / S-3: THE thread screen (fixture session-thread.json
// ×4): the header (title + ONE tab strip), the Conversation (THE Dock, the
// composer, the jump button, selection), the issue / session tabs and the
// Stories tab (stories.json STORIES TAB, pinned right).
// The thread route (`/t/$teamSlug/threads/$threadId`) and an agent's chat
// (`/t/$teamSlug/agents/$agentId`, chats.json CHAT MODE: `threadChatMode`
// = bubbles, the agent's name, call button and agent menu in the header)
// both render it; the route owns the `?tab=` search and how it navigates.

export type ThreadSearch = {
  tab?: `issue` | `session` | `stories`
  issue?: string
  /** The session open in place (`tab=session`). */
  session?: string
  /** The Stories tab's review pane (`tab=stories`, the complete diff). */
  section?: `all`
  /** The file in focus on the review pane. */
  file?: string
  /** Where it was opened from (`TABLESS_ORIGIN` = a Running row: no tab). */
  from?: string
  /** S-3: the inter-session back stack (`lib/session-back.ts`); the
   *  header's `‹` pops it. */
  back?: string
}

/** The `?tab=` search the thread screen reads (the agent chat's too). */
export function validateThreadSearch(search: Record<string, unknown>): ThreadSearch {
  const pick = (value: unknown) => (typeof value === `string` && value ? value : undefined)
  // The retired `?tab=guide` lands on its successor, the Stories tab.
  const tab =
    search.tab === `issue` || search.tab === `session` || search.tab === `stories`
      ? search.tab
      : search.tab === `guide`
        ? `stories`
        : undefined
  const section = tab === `stories` && search.section === `all` ? (`all` as const) : undefined
  return {
    ...(tab ? { tab } : {}),
    ...(section ? { section } : {}),
    ...(section && pick(search.file) ? { file: pick(search.file) } : {}),
    ...(tab === `issue` && pick(search.issue) ? { issue: pick(search.issue) } : {}),
    ...(tab === `session` && pick(search.session) ? { session: pick(search.session) } : {}),
    ...(pick(search.from) ? { from: pick(search.from) } : {}),
    ...backSearch(search.back),
  }
}

/** The issue chips' context-menu kind (`menuProps`). */
const THREAD_ISSUE_MENU = `thread-issue`

const PrivateIcon = conceptIcon(`ui-private`)
// #EXP-1380 (chats.json PERF): a STABLE callback, or every row re-renders
// with the page (`StableRow` compares it by identity).
const attachmentSrc = (id: string) => `/api/attachments/${id}`

/** S-3: an agent conversation's header parts (the route's). */
export interface ThreadScreenAgent {
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

export function ThreadScreen({
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
  /** The route's `?tab=` state (the thread's or the agent chat's). */
  search: ThreadSearch
  /** Navigate the SAME screen to `next` (replace = no history entry). */
  onNavigate: (next: ThreadSearch, options?: { replace?: boolean }) => void
  /** S-3: an agent conversation's header (its name, the call button and
   *  THE agent menu); absent = the thread's. */
  agent?: ThreadScreenAgent
  team: Team
  teamSlug: string
  view: SessionThreadView
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
    (tab): tab is Extract<SessionThreadView[`tabs`][number], { kind: `issue` }> =>
      tab.kind === `issue`
  )
  const sessionTabs = view.tabs.filter(
    (tab): tab is Extract<SessionThreadView[`tabs`][number], { kind: `session` }> =>
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
  const guidePr = useMemo(() => threadGuidePullRequest(pullRequests), [pullRequests])

  const rowsById = useMemo(() => new Map(rows.map((row) => [row.id, row])), [rows])
  const focused = view.focusId ? (rowsById.get(view.focusId) ?? null) : null
  /** EXP-1323: a suggested-tasks card's Start with… lists the POSTING
   *  session's agent's models (a session of the tree; the root's as the
   *  fallback). */
  const agentFor = useCallback(
    (sessionId: string) => rowsById.get(sessionId)?.cli ?? root.cli,
    [rowsById, root.cli]
  )
  // EXP-1323: opening the thread marks its root read (again whenever a newer
  // agent reply lands anywhere in the tree while it is open); an opened
  // session marks itself (`session-body.tsx`).
  const treeIds = useMemo(() => rows.map((row) => row.id), [rows])
  useMarkRead(root.id, useNewestAgentMessageAt(treeIds))
  // NEW-6 (N-1) / NEW-46: on a single-session thread the Conversation tab
  // IS the root's raw work in place (the default body).
  // S-3 (chats.json CHAT MODE): an agent conversation always paints its
  // bubbles, never the root's raw work in place.
  const mainSession =
    view.mainSessionId && threadChatMode(root) !== `bubbles` ? (rowsById.get(view.mainSessionId) ?? null) : null
  const tab =
    search.tab === `issue` && search.issue && issueIds.includes(search.issue)
      ? { kind: `issue` as const, issueId: search.issue }
      : search.tab === `stories`
        ? { kind: `stories` as const }
        : search.tab === `session` && focused && focused.id !== root.id
        ? { kind: `session` as const, sessionId: focused.id }
        : mainSession
          ? { kind: `session` as const, sessionId: mainSession.id }
          : { kind: `conversation` as const }
  const sessionInPlace = tab.kind === `session` ? (focused?.id === tab.sessionId ? focused : mainSession) : null

  // S-3: the back stack rides along every tab switch; an xref pushes it.
  const keepBack = search.back ? { back: search.back } : {}
  const go = (next: ThreadSearch) => onNavigate({ ...next, ...keepBack }, { replace: true })

  /** A session opens IN PLACE (a pushed history entry: Back returns). */
  const openSession = (sessionId: string) =>
    onNavigate({ tab: `session`, session: sessionId, ...keepBack })
  /** S-3: the session on show (an xref pushes it; `‹` pops back to it). */
  const nav = useSessionNav({
    teamSlug,
    here: tab.kind === `session` ? tab.sessionId : root.id,
    back: search.back,
  })
  // EXP-1343: phones page through every strip tab (swipe AND tap, in strip
  // order); a tap slides the new page in from the side it sits on.
  const pages = useMemo(() => threadPages(view.tabs), [view.tabs])
  const page = threadPageOf(tab)
  const showPage = (next: ThreadPage) => {
    if (isMobile) primeTabEnter(pages, page, next)
    go(threadPageSearch(next))
  }
  const swipe = useFaceSwipe(pages, page, showPage)
  /** A chip's pick swaps the tab in place (a page on phones). */
  const pickSession = (sessionId: string) => showPage(`session:${sessionId}`)
  const pickIssue = (issueId: string) => showPage(`issue:${issueId}`)

  const firstIssue = issueTabs[0] ? issuesById.get(issueTabs[0].issueId) : undefined
  const menu = useThreadMenu({
    rootId: root.id,
    teamSlug,
    issueIds,
    newIssueBoardId: firstIssue?.boardId,
    canEdit: permissions.isMember,
  })

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

  // EXP-1334: the issue chips = the TOP-LEVEL linked issues; a chip with
  // sub-issues wears the chevron and a CLICK opens its subtree (THE Menu,
  // the bottom sheet on phones); active while it or a nested one is open,
  // reading `<chip> › <open one>`.
  const issueItems: ThreadStripItem[] = issueTabs.flatMap((entry) => {
    const issue = issuesById.get(entry.issueId)
    if (!issue) return []
    const openId = tab.kind === `issue` ? tab.issueId : null
    const openNested = entry.nested.find((nested) => nested.issueId === openId) ?? null
    const active = openId === entry.issueId || openNested !== null
    const folded = entry.nested.length > 0
    const status = issueStatus(issue)!
    const subtree: ThreadIssueTreeRow[] = [
      { issueId: entry.issueId, identifier: entry.identifier, title: issue.title, status, depth: 0 },
      ...entry.nested.map((nested) => {
        const row = issuesById.get(nested.issueId)
        return {
          issueId: nested.issueId,
          identifier: nested.identifier,
          title: row?.title ?? ``,
          status: issueStatus(row),
          depth: nested.depth,
        }
      }),
    ]
    return [
      {
        key: entry.issueId,
        active,
        entry: {
          kind: `item` as const,
          id: entry.issueId,
          icon: <StatusGlyph {...status} className="size-3.5" />,
          label: (
            <>
              <span className="mr-1.5 font-mono text-xs text-muted-foreground">
                {entry.identifier}
              </span>
              {issue.title}
            </>
          ),
          onSelect: () => pickIssue(entry.issueId),
        },
        render: ({ short, measure }) => {
          const chip = (
            <ThreadIssueTab
              identifier={entry.identifier}
              title={issue.title}
              status={status}
              active={active}
              short={short}
              path={openNested?.identifier ?? null}
              chevron={folded}
              data-testid={measure ? undefined : `thread-tab-${entry.identifier}`}
              boxProps={measure ? undefined : { ...menuProps(THREAD_ISSUE_MENU, entry.issueId) }}
              onSelect={measure || folded ? undefined : () => pickIssue(entry.issueId)}
            />
          )
          if (measure || !folded) return chip
          return (
            <ThreadChipSubtree title={entry.identifier} trigger={chip} testId="thread-issue-subtree">
              {(close) => (
                <ThreadIssueTree
                  menu
                  rows={subtree}
                  activeId={openId}
                  onOpen={(issueId) => {
                    close()
                    pickIssue(issueId)
                  }}
                />
              )}
            </ThreadChipSubtree>
          )
        },
      },
    ]
  })

  // EXP-1334: the session chips = the dispatcher's DIRECT children; deeper
  // sessions fold under their chip (SessionRow small + the tree guides).
  const treeRows = useMemo<ThreadTreeRow[]>(() => {
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
  const treeRowsById = useMemo(
    () => new Map(treeRows.map((row) => [row.sessionId, row])),
    [treeRows]
  )
  const sessionItems: ThreadStripItem[] = sessionTabs.map((entry) => {
    const openId = tab.kind === `session` ? tab.sessionId : null
    const openNested = entry.nested.find((nested) => nested.sessionId === openId) ?? null
    const active = openId === entry.sessionId || openNested !== null
    const folded = entry.nested.length > 0
    const agent = rowsById.get(entry.sessionId)?.cli ?? null
    const ids = new Set([entry.sessionId, ...entry.nested.map((nested) => nested.sessionId)])
    const subtree = treeRows
      .filter((row) => ids.has(row.sessionId))
      .map((row) => (row.sessionId === entry.sessionId ? { ...row, parentId: null } : row))
    return {
      key: entry.sessionId,
      active,
      entry: {
        kind: `item` as const,
        id: entry.sessionId,
        icon: (
          <AgentRunMark
            agent={agent}
            state={sessionTabRunMark(entry.markState)}
            ringClassName="ring-popover"
            className="size-3.5"
          />
        ),
        label: entry.name,
        value: treeRowsById.get(entry.sessionId)?.stateWord,
        onSelect: () => pickSession(entry.sessionId),
      },
      render: ({ measure }) => {
        const chip = (
          <ThreadSessionTab
            agent={agent}
            markState={entry.markState}
            name={entry.name}
            active={active}
            path={openNested?.name ?? null}
            chevron={folded}
            data-testid={measure ? undefined : `thread-tab-session`}
            onSelect={measure || folded ? undefined : () => pickSession(entry.sessionId)}
          />
        )
        if (measure || !folded) return chip
        return (
          <ThreadChipSubtree title={entry.name} trigger={chip} testId="thread-session-subtree">
            {(close) => (
              <ThreadSessionTree
                menu
                rows={subtree}
                activeId={openId}
                onOpen={(sessionId) => {
                  close()
                  pickSession(sessionId)
                }}
              />
            )}
          </ThreadChipSubtree>
        )
      },
    }
  })

  // STORIES TAB: every story of the tree (the chip's dot, the body).
  const storyTab = useStoryTab(team.id, root.id)
  const tabs = (
    <ThreadTabs
      variant={isMobile ? `pager` : `bar`}
      conversation={{
        label: THREAD_COPY.conversation,
        // NEW-46: on a single-session thread the Conversation tab IS that
        // session's work in place (`mainSessionId`), active while it shows.
        active:
          tab.kind === `conversation` ||
          (mainSession !== null && tab.kind === `session` && tab.sessionId === mainSession.id),
        onSelect: () => (mainSession ? go({}) : showPage(`conversation`)),
      }}
      issues={issueItems}
      add={menu.addMenu}
      sessions={sessionItems}
      stories={{
        label: STORY_TAB.copy.label,
        active: tab.kind === `stories`,
        dot: storyTab.ring?.unseen ?? false,
        onSelect: () => showPage(`stories`),
      }}
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
  } else if (tab.kind === `stories`) {
    body = (
      <div
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
        data-testid="thread-stories-scroll"
        style={topInset ? { paddingTop: topInset } : undefined}
      >
        <div className="mx-auto w-full max-w-3xl px-4 pt-6 pb-10 md:px-5">
          <ThreadStories
            stories={storyTab}
            teamSlug={teamSlug}
            pullRequest={guidePr}
            canMerge={permissions.isMember}
            canReply={permissions.isMember}
            review={search.section === `all`}
            file={search.file ?? null}
            onReview={(open, file) =>
              go({ tab: `stories`, ...(open ? { section: `all` as const } : {}), ...(open && file ? { file } : {}) })
            }
            here={root.id}
            back={search.back}
          />
        </div>
      </div>
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
        onOpenSession={openSession}
        onOpenXref={nav.open}
        agentFor={agentFor}
        topInset={topInset}
        messagePage={messagePage}
      />
    )
  }

  // EXP-1307/1316: the header's `…` (Close PR, EXP-1338) on EVERY tab
  // while the thread has an open PR. Thread 58cb32df: the same `…` holds
  // THE chat menu (Stop · Stop and delete | Delete, chats.json MENUS).
  const chatActions = useChatActions(teamSlug)
  const headerMenu = (className?: string) =>
    agent ? agent.headerMenu(className) : (
    <ThreadHeaderMenu
      root={root}
      title={view.title}
      pr={guidePr}
      canClosePr={permissions.isMember}
      actions={chatActions}
      className={className}
    />
  )

  // EXP-1316: the title is derived (`sessionTitle`), never renamed.
  const title = (
    <div className="flex min-w-0 items-center gap-2">
      {agent?.glyph}
      <h1
        data-testid="thread-title"
        className="min-w-0 truncate text-2xl font-semibold tracking-tight"
      >
        {agent?.name ?? view.title}
      </h1>
      {view.private && (
        <PrivateIcon
          role="img"
          aria-label="Private"
          data-testid="thread-private"
          className="size-4 shrink-0 text-muted-foreground"
        />
      )}
    </div>
  )

  const issueMenus = {
    [THREAD_ISSUE_MENU]: ({ target, open, onOpenChange }: MenuSessionProps) => {
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
      threadPendingAsks(view.rows, () => null).length > 0
    const headerBusy = rows.some((row) => row.busy)
    const headerLead: ChatLead = headerNeedsYou
      ? `needs_you`
      : headerBusy
        ? `working`
        : (threadLeadPrState(pullRequests) ?? `waiting`)
    return (
      <IssueRefOutlet issueIds={issueIds} onOpen={pickIssue}>
      <MenuGestureHost menus={issueMenus}>
        {/* EXP-1316: THE detail chrome — the header band (blur, scrim, edge
            fade) floats OVER the body, which scrolls under it; every body's
            scroller pads its top by the band's measured height. */}
        <div
          className={MOBILE_DETAIL_SCREEN_CLASS}
          data-testid="thread-page"
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
                      data-testid="thread-private"
                      className="size-3.5 shrink-0 text-muted-foreground"
                    />
                  ) : undefined
                }
                device={deviceGlyph(root.deviceId)?.name ?? null}
                needsYou={headerNeedsYou}
                busy={headerBusy}
                needsYouWord={THREAD_ROW_WORDS.needsInput ?? ``}
              />
            }
            onBack={nav.pop ?? (() => void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } }))}
            menu={
              <span className="flex shrink-0 items-center gap-1">
                {/* S-3: the Merge pill moved up from the retired bottom
                    capsule into the header, like md+. */}
                <ThreadGuideMerge pr={guidePr} canMerge={permissions.isMember} />
                {trailing}
              </span>
            }
            below={<div className="px-3 pb-2">{tabs}</div>}
          />
          {/* The page under the band: only the visible one renders. */}
          <div
            data-face-body=""
            data-testid="thread-page-body"
            className={cn(`flex min-h-0 flex-1 flex-col`, TAB_BODY_TOUCH_CLASS)}
          >
            {agent?.overlay}
            {body}
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
      <div className="flex h-full min-h-0 flex-col" data-testid="thread-page">
        <div className="shrink-0 border-b border-border px-6 pt-6 pb-3 md:px-10">
          <div className="flex items-center gap-3">
            <BackStackButton onPop={nav.pop} className="-ml-2" />
            <div className="min-w-0 flex-1">{title}</div>
            <ThreadGuideMerge pr={guidePr} canMerge={permissions.isMember} />
            {headerMenu(`shrink-0 text-muted-foreground`)}
          </div>
          <div className="mt-4">{tabs}</div>
        </div>
        {agent?.overlay}
        {body}
      </div>
    </MenuGestureHost>
    {chatActions.dialog}
    </IssueRefOutlet>
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
  agentFor,
  topInset = 0,
  messagePage,
}: {
  teamId: string
  view: SessionThreadView
  /** The root's row (the live tail reads its relay state). */
  root: SessionRow
  users: User[]
  canReply: boolean
  /** The ROOT's busy: Stop stops the root, so a busy child never shows it. */
  busy: boolean
  /** A session card opens that session in place (EXP-1316). */
  onOpenSession: (sessionId: string) => void
  /** S-3: an xref opens its session on the back stack. */
  onOpenXref: (sessionId: string) => void
  /** The agent CLI of a session of the tree (the suggestions' models). */
  agentFor: (sessionId: string) => string | null | undefined
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
  // S-3 (session-thread.json `chatModeCases`, chats.json CHAT MODE): an
  // agent conversation paints chat bubbles on every form factor; a thread
  // keeps its transcript (phones: the bubbles of thread 58cb32df).
  const bubbles = threadChatMode(root) === `bubbles` || isMobile
  const suggestions = useSuggestionActions({ teamId, enabled: canReply, agentFor })
  // The Dock holds the open suggestions; S-3: a waiting plan or question is
  // a story (stories.json), never docked, and stays inline answerable.
  const dockedIds = useMemo(
    () => (canReply ? threadDockedIds(view.rows, () => null) : new Set<string>()),
    [canReply, view.rows]
  )
  const sessionName = useMemo(() => threadSessionNames(view.tabs), [view.tabs])
  // N-12: THE Dock over the Conversation's bottom edge: the root's Queue,
  // queued messages, banners and open suggestions, then the composer
  // (a phone's floats in the work bar); the scroller pads by its height.
  const sessionDock = useSessionDock(root)
  const sendSurfaceReply = (text: string) => {
    trpc.sessions.send
      .mutate({ teamId, sessionId: view.composer.sessionId, text }, SKIP_ERROR_TOAST)
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
      teamId={teamId}
      sessionId={view.composer.sessionId}
      placeholder={view.composer.placeholder}
      users={users}
      busy={busy}
      canStop
      opaque={isMobile}
      hideReplies={isMobile}
      testId="thread-composer"
      surface={root.agentId ? `agent` : `session`}
    />
  ) : null

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className="relative min-h-0 flex-1">
      <div
        ref={follow.scrollRef}
        onScroll={follow.onScroll}
        data-testid="thread-scroll"
        className="h-full overflow-y-auto overscroll-contain"
        style={topInset ? { paddingTop: topInset } : undefined}
      >
        <div
          ref={follow.contentRef}
          className="mx-auto w-full max-w-4xl px-3 pt-8 md:px-5"
          style={{ paddingBottom: dockHeight + 16 }}
        >
          {transcript.canLoadEarlier ? <LoadEarlierButton onClick={transcript.loadEarlier} /> : null}
          <ThreadConversation
            rows={transcript.rows}
            renderText={renderThreadText}
            attachmentSrc={attachmentSrc}
            messageCaption={(row) => {
              const message = messagesById.get(row.id)
              const caption = userMessageCaption({
                name: row.authorId ? usersById.get(row.authorId)?.name : null,
                at: message ? new Date(message.createdAt) : null,
              })
              return row.queued ? [caption, `queued`].filter(Boolean).join(` · `) : caption
            }}
            onOpenSession={onOpenSession}
            onOpenXref={onOpenXref}
            xrefAgent={xrefAgent}
            origin={view.origin}
            onAnswer={canReply ? (id, input) => void answer(id, input) : undefined}
            onDecide={canReply ? (id, input) => void answer(id, input) : undefined}
            pending={pending}
            docked={dockedIds}
            viewerId={session?.user?.id ?? null}
            onUnqueue={(id) => void unqueue(id)}
            // Thread 58cb32df (chats.json TRANSCRIPT): phones paint chat
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
        <ThreadDock
          rows={view.rows}
          sessionName={sessionName}
          renderText={renderThreadText}
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

/** EXP-1323: a filed suggestion's issue = THE issue chip once it resolves
 *  (same team, synced), else its identifier. */
function FiledIssueRef({ identifier }: { identifier: string }) {
  const issueRefs = useIssueRefs()
  const resolved = issueRefs?.resolve(identifier) ?? null
  if (!resolved) return <>{identifier}</>
  return <IssueRefChip issue={resolved} onClick={() => issueRefs?.open(resolved.identifier)} />
}
