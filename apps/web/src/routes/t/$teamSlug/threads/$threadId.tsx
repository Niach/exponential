import { threadPendingAsks, threadSessionNames } from "@exp/domain-contract/ask-dock"
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { createFileRoute, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import {
  AgentRunMark,
  conceptIcon,
  EmptyState,
  FloatingComposer,
  JumpToBottomButton,
  Menu,
  MenuGestureHost,
  menuProps,
  sessionTabRunMark,
  SkeletonRows,
  StatusGlyph,
  ThreadChipSubtree,
  ThreadDock,
  ThreadConversation,
  ThreadIssueTab,
  ThreadIssueTree,
  ThreadSessionTab,
  ThreadSessionTree,
  ThreadTabs,
  useIsMobile,
  userMessageCaption,
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
import { parseGuideSection, type GuideSectionKey } from "@/components/guide-section-diff"
import {
  ThreadGuide,
  ThreadGuideMenu,
  ThreadGuideMerge,
  threadGuidePullRequest,
} from "@/components/thread/thread-guide"
import { useAnswerMessage, useUnqueueMessage } from "@/components/thread/agent-channel"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useThreadMenu } from "@/components/thread/thread-menu"
import { InPlaceSession, SessionStopPill } from "@/components/thread/session-body"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import { primeTabEnter, TAB_BODY_TOUCH_CLASS, useFaceSwipe } from "@/hooks/use-face-swipe"
import { useMarkRead, useNewestAgentMessageAt } from "@/hooks/use-mark-read"
import { useSuggestionActions } from "@/hooks/use-suggestion-actions"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useSession } from "@/hooks/use-session"
import { useSessionMessages, useSessionThread } from "@/hooks/use-session-tree"
import { useTeamBoards, useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { threadPageOf, threadPages, threadPageSearch, type ThreadPage } from "@/lib/thread-pages"
import { cn } from "@/lib/utils"
import {
  sessionCard,
  sessionTabMarkState,
  THREAD_COPY,
  THREADS_LIST_COPY,
  toTreeSession,
  treeOrder,
  type MessageRow,
  type PullRequestRow,
  type SessionRow,
  type SessionThreadView,
} from "@/lib/session-tree"

// EXP-1293 Threads v2: a THREAD = a root session (fixture
// `session-thread.json` ×4, `buildSessionThread`). Header: the title
// (EXP-1316: derived, `Implement EXP-12` / `Run <action>` / the agent's
// auto-name; nobody renames it; a lock when private) + ONE tab strip that
// never scrolls (EXP-1334, `ThreadTabs`): "+" (THE thread menu) ·
// Conversation | issue chips … +X | session chips … +Y | Guide (a dot while
// it holds anything). md+: while the Guide's PR is
// open the header's trailing slot carries Merge on EVERY tab, then the `…`
// (Guide tab only).
// `?tab=` names the tab on show:
//
//   (absent)               the conversation = the DISPATCHER (EXP-1334: no
//                          card of its own): its messages, THE session card
//                          per child, question and plan cards (its own and
//                          its children's)
//                          answered in the card, EXP-1323: the suggested
//                          tasks card, report lines and the live tail (Tasks
//                          card + status line), the composer at the bottom
//   issue&issue=<id>       that issue's body, embedded (no header bar, no
//                          Start, no pin, no Threads row)
//   guide[&section=&file=] the root's Guide over its pull request (its stack
//                          rail; phones: THE bottom Merge capsule)
//   session&session=<id>   EXP-1316: that session's RAW WORK in place under
//                          this header + strip, its Stop pill in the header
//                          (EXP-1343: phones too, no own screen). The
//                          session chips = the dispatcher's direct children;
//                          a chip with descendants opens its subtree on a
//                          CLICK. `/sessions/$id` redirects here.
//
// Phones: the strip IS the pager under the header (EXP-1343: every strip
// tab is a page — swipe or tap, in strip order, `lib/thread-pages.ts`). No
// right sidebar, no Show work, no captions. EXP-1347: an issue pill of one
// of this thread's issues selects its tab (`IssueRefOutlet`).

type ThreadSearch = {
  tab?: `issue` | `guide` | `session`
  issue?: string
  /** The session open in place (`tab=session`). */
  session?: string
  section?: GuideSectionKey
  file?: string
  /** Where it was opened from (`TABLESS_ORIGIN` = a Running row: no tab). */
  from?: string
}

/** The issue chips' context-menu kind (`menuProps`). */
const THREAD_ISSUE_MENU = `thread-issue`

/** How long a missing root waits for its row before "not found". */
const SYNC_GRACE_MS = 4_000

const ThreadsIcon = conceptIcon(`nav-threads`)
const PrivateIcon = conceptIcon(`ui-private`)

export const Route = createFileRoute(`/t/$teamSlug/threads/$threadId`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.title) }] }),
  validateSearch: (search: Record<string, unknown>): ThreadSearch => {
    const pick = (value: unknown) => (typeof value === `string` && value ? value : undefined)
    const tab =
      search.tab === `issue` || search.tab === `guide` || search.tab === `session`
        ? search.tab
        : undefined
    const section = tab === `guide` ? parseGuideSection(search.section) : undefined
    return {
      ...(tab ? { tab } : {}),
      ...(tab === `issue` && pick(search.issue) ? { issue: pick(search.issue) } : {}),
      ...(tab === `session` && pick(search.session) ? { session: pick(search.session) } : {}),
      ...(section !== undefined ? { section } : {}),
      ...(section !== undefined && pick(search.file) ? { file: pick(search.file) } : {}),
      ...(pick(search.from) ? { from: pick(search.from) } : {}),
    }
  },
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ThreadPage,
})

function ThreadPage() {
  const { teamSlug, threadId } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const search = Route.useSearch()
  const thread = useSessionThread(
    threadId,
    search.tab === `session` ? (search.session ?? null) : null
  )
  usePageTitle(
    thread.view && thread.titleReady
      ? pageTitle(thread.view.title, THREADS_LIST_COPY.title)
      : undefined
  )

  // A thread opened right after its start may land before its row: wait a
  // beat before saying it is gone.
  const [graceOver, setGraceOver] = useState(false)
  useEffect(() => {
    setGraceOver(false)
    const timer = setTimeout(() => setGraceOver(true), SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [threadId])

  // An agent's conversation (NEW-2: a root with `agentId`) is no thread:
  // its page is the agent's.
  if (thread.root?.agentId) {
    return (
      <Navigate
        to="/t/$teamSlug/agents/$agentId"
        params={{ teamSlug, agentId: thread.root.agentId }}
        replace
      />
    )
  }

  // EXP-1316: until the issue shapes reported ready the title could still
  // flip from the request text to `Implement EXP-12` (and the chips pop in):
  // hold the page on its placeholder — but only for the sync grace: a shape
  // that never reports ready must not leave a permanent skeleton, so past it
  // the page renders with whatever it has (the title settles in place).
  const titleHeld = !thread.titleReady && !graceOver
  if (!team || !thread.view || !thread.root || titleHeld) {
    if (team && thread.isReady && !thread.root && graceOver) {
      return (
        <div className="flex h-full flex-col items-center justify-center">
          <EmptyState
            icon={ThreadsIcon}
            title="Not found"
            description="It may still be syncing, or it was deleted."
            size="page"
          />
        </div>
      )
    }
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }
  return (
    <ThreadScreen
      key={thread.root.id}
      team={team}
      teamSlug={teamSlug}
      view={thread.view}
      root={thread.root}
      rows={thread.rows}
      titles={thread.titles}
      pullRequests={thread.pullRequests}
    />
  )
}

function ThreadScreen({
  team,
  teamSlug,
  view,
  root,
  rows,
  titles,
  pullRequests,
}: {
  team: Team
  teamSlug: string
  view: SessionThreadView
  root: SessionRow
  /** Every session of the tree (the chips' marks, the tree popover). */
  rows: SessionRow[]
  /** EXP-1341: every tree session's `sessionTitle` (the chips' names). */
  titles: ReadonlyMap<string, string>
  pullRequests: PullRequestRow[]
}) {
  const navigate = useNavigate()
  const search = Route.useSearch()
  const isMobile = useIsMobile()
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
  const guideTab = view.tabs.find((tab) => tab.kind === `guide`)
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
  const mainSession = view.mainSessionId ? (rowsById.get(view.mainSessionId) ?? null) : null
  const tab =
    search.tab === `issue` && search.issue && issueIds.includes(search.issue)
      ? { kind: `issue` as const, issueId: search.issue }
      : search.tab === `guide`
        ? { kind: `guide` as const }
        : search.tab === `session` && focused && focused.id !== root.id
          ? { kind: `session` as const, sessionId: focused.id }
          : mainSession
            ? { kind: `session` as const, sessionId: mainSession.id }
            : { kind: `conversation` as const }
  const sessionInPlace = tab.kind === `session` ? (focused?.id === tab.sessionId ? focused : mainSession) : null

  const go = (next: ThreadSearch) =>
    void navigate({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug, threadId: root.id },
      search: next,
      replace: true,
    })

  /** A session opens IN PLACE (a pushed history entry: Back returns). */
  const openSession = (sessionId: string) =>
    void navigate({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug, threadId: root.id },
      search: { tab: `session`, session: sessionId },
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

  const issueStatus = (issue: Issue | undefined) => {
    if (!issue) return { icon: `circle-dashed` as const, colorClass: `text-muted-foreground` }
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
    const status = issueStatus(issue)
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
      guide={{
        label: THREAD_COPY.guide,
        active: tab.kind === `guide`,
        dot: guideTab?.kind === `guide` && guideTab.dot,
        onSelect: () => showPage(`guide`),
      }}
    />
  )

  // ── The bodies ─────────────────────────────────────────────────────────────
  let body: ReactNode
  if (tab.kind === `session` && sessionInPlace) {
    body = <InPlaceSession row={sessionInPlace} topInset={topInset} floating={isMobile} />
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
  } else if (tab.kind === `guide`) {
    body = (
      <div className="flex min-h-0 flex-1 flex-col">
        <div
          className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
          style={topInset ? { paddingTop: topInset } : undefined}
        >
          <div className="mx-auto w-full max-w-3xl pb-8">
            <ThreadGuide
              root={root}
              pullRequest={guidePr}
              teamSlug={teamSlug}
              section={search.section}
              file={search.file ?? null}
              onSection={(section, file) =>
                go({
                  tab: `guide`,
                  ...(section !== undefined ? { section } : {}),
                  ...(section !== undefined && file ? { file } : {}),
                })
              }
              canMerge={permissions.isMember}
            />
          </div>
        </div>
        {isMobile && search.section === undefined && (
          <ThreadGuideMerge pr={guidePr} canMerge={permissions.isMember} />
        )}
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
        agentFor={agentFor}
        topInset={topInset}
      />
    )
  }

  // EXP-1307/1316: the header's `…` (Close PR, EXP-1338) on EVERY tab
  // while the Guide has an open PR (the fixture's header merge; iOS and desktop
  // already do), not only on the Guide.
  const guideMenu = (className?: string) =>
    guidePr ? (
      <ThreadGuideMenu pr={guidePr} canClose={permissions.isMember} className={className} />
    ) : null

  // EXP-1316: the title is derived (`sessionTitle`), never renamed.
  const title = (
    <div className="flex min-w-0 items-center gap-2">
      <h1
        data-testid="thread-title"
        className="min-w-0 truncate text-2xl font-semibold tracking-tight"
      >
        {view.title}
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

  const stopPill =
    sessionInPlace && sessionInPlace.status !== `ended` && tab.kind === `session` ? (
      <SessionStopPill sessionId={sessionInPlace.id} />
    ) : null

  if (isMobile) {
    const trailing = guideMenu(HEADER_BUTTON_CLASS)
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
              <span className="flex min-w-0 items-center gap-1.5">
                <span className="truncate">{view.title}</span>
                {view.private && (
                  <PrivateIcon
                    aria-label="Private"
                    data-testid="thread-private"
                    className="size-3.5 shrink-0 text-muted-foreground"
                  />
                )}
              </span>
            }
            onBack={() => void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } })}
            menu={
              stopPill ? (
                <span className="flex shrink-0 items-center gap-1">
                  {stopPill}
                  {trailing}
                </span>
              ) : (
                trailing ?? undefined
              )
            }
            below={<div className="px-3 pb-2">{tabs}</div>}
          />
          {/* The page under the band: only the visible one renders. */}
          <div
            data-face-body=""
            data-testid="thread-page-body"
            className={cn(`flex min-h-0 flex-1 flex-col`, TAB_BODY_TOUCH_CLASS)}
          >
            {body}
          </div>
        </div>
      </MenuGestureHost>
      </IssueRefOutlet>
    )
  }

  return (
    <IssueRefOutlet issueIds={issueIds} onOpen={pickIssue}>
    <MenuGestureHost menus={issueMenus}>
      <div className="flex h-full min-h-0 flex-col" data-testid="thread-page">
        <div className="shrink-0 border-b border-border px-6 pt-6 pb-3 md:px-10">
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1">{title}</div>
            <ThreadGuideMerge pr={guidePr} canMerge={permissions.isMember} placement="header" />
            {stopPill}
            {guideMenu(`shrink-0 text-muted-foreground`)}
          </div>
          <div className="mt-4">{tabs}</div>
        </div>
        {body}
      </div>
    </MenuGestureHost>
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
  agentFor,
  topInset = 0,
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
  /** The agent CLI of a session of the tree (the suggestions' models). */
  agentFor: (sessionId: string) => string | null | undefined
  /** Phones: the overlay header band's height (the scroller pads by it). */
  topInset?: number
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
  const isMobile = useIsMobile()
  const suggestions = useSuggestionActions({ teamId, enabled: canReply, agentFor })
  // THE ask dock (ask-dock.json ×4): the tree's waiting cards take the
  // composer's slot, exactly like a session's card; read-only viewers keep
  // them inline.
  const dockedIds = useMemo(
    () =>
      canReply
        ? new Set(threadPendingAsks(view.rows, () => null).map((pending) => pending.id))
        : new Set<string>(),
    [canReply, view.rows]
  )
  const sessionName = useMemo(() => threadSessionNames(view.tabs), [view.tabs])
  // N-12: THE Dock over the Conversation's bottom edge: the tree's waiting
  // cards, the root's Queue, queued messages and banners, then the composer
  // (a phone's floats in the work bar); the scroller pads by its height.
  const sessionDock = useSessionDock(root)
  const [dockHeight, setDockHeight] = useState(0)
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
          <ThreadConversation
            rows={view.rows}
            renderText={renderThreadText}
            attachmentSrc={(id) => `/api/attachments/${id}`}
            messageCaption={(row) => {
              const message = messagesById.get(row.id)
              const caption = userMessageCaption({
                name: row.authorId ? usersById.get(row.authorId)?.name : null,
                at: message ? new Date(message.createdAt) : null,
              })
              return row.queued ? [caption, `queued`].filter(Boolean).join(` · `) : caption
            }}
            onOpenSession={onOpenSession}
            onAnswer={canReply ? (id, input) => void answer(id, input) : undefined}
            onDecide={canReply ? (id, input) => void answer(id, input) : undefined}
            pending={pending}
            docked={dockedIds}
            viewerId={session?.user?.id ?? null}
            onUnqueue={(id) => void unqueue(id)}
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
        style={{ bottom: dockHeight + 12 }}
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
          composer={isMobile ? undefined : composer}
          onHeight={setDockHeight}
          phoneClearance={isMobile}
          autoOpenReady={view.rows.length > 0}
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
