import { useEffect, useMemo, useState, type ReactNode } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { createFileRoute, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import {
  conceptIcon,
  EmptyState,
  FLOATING_COMPOSER_CLEARANCE,
  FloatingComposer,
  IssueChip,
  JumpToBottomButton,
  Menu,
  MenuGestureHost,
  menuProps,
  SkeletonRows,
  ThreadConversation,
  ThreadSessionGroup,
  ThreadSessionTab,
  ThreadSessionTree,
  ThreadTabs,
  ThreadTabsAddButton,
  useIsMobile,
  userMessageCaption,
  type MenuSessionProps,
  type ThreadTreeRow,
} from "@exp/ui"
import type { Issue, User } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"
import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { SessionComposer } from "@/components/session-composer"
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
import { InPlaceSession, SessionScreen, SessionStopPill } from "@/components/thread/session-body"
import { useMeasuredSize } from "@/hooks/use-detail-chrome"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useSession } from "@/hooks/use-session"
import { useSessionMessages, useSessionThread } from "@/hooks/use-session-tree"
import { useTeamBoards, useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { pageTitle, usePageTitle } from "@/lib/page-title"
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
// auto-name; nobody renames it; a lock when private) + ONE tab strip:
// Conversation · one issue chip per linked issue (`IssueChip size="tab"`) ·
// "+" · Guide (a dot while it holds anything). md+: while the Guide's PR is
// open the header's trailing slot carries Merge on EVERY tab, then the `…`
// (Guide tab only).
// `?tab=` names the tab on show:
//
//   (absent)               the conversation: the root's messages and THE
//                          session card per session, question and plan cards
//                          answered in the card, the composer at the bottom
//   issue&issue=<id>       that issue's body, embedded (no header bar, no
//                          Start, no pin, no Threads row)
//   guide[&section=&file=] the root's Guide over its pull request (its stack
//                          rail; phones: THE bottom Merge capsule)
//   session&session=<id>   EXP-1316: that session's RAW WORK in place (md+:
//                          under this header + strip, its Stop pill in the
//                          header; phones: the session's own screen). The
//                          strip's session chips = that level
//                          (`buildSessionThread` focus); hovering them opens
//                          the whole tree. `/sessions/$id` redirects here.
//
// Phones: the strip IS the pager under the header. No right sidebar, no
// Show work, no captions.

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
  usePageTitle(thread.view ? pageTitle(thread.view.title, THREADS_LIST_COPY.title) : undefined)

  // A thread opened right after its start may land before its row: wait a
  // beat before saying it is gone.
  const [graceOver, setGraceOver] = useState(false)
  useEffect(() => {
    setGraceOver(false)
    const timer = setTimeout(() => setGraceOver(true), SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [threadId])

  // A team agent's root is no thread: its page is the agent's channel.
  if (thread.root?.kind === `agent`) {
    return (
      <Navigate
        to="/t/$teamSlug/agents/$agentId"
        params={{ teamSlug, agentId: thread.root.id }}
        replace
      />
    )
  }

  if (!team || !thread.view || !thread.root) {
    if (team && thread.isReady && graceOver) {
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
  pullRequests,
}: {
  team: Team
  teamSlug: string
  view: SessionThreadView
  root: SessionRow
  /** Every session of the tree (the chips' marks, the tree popover). */
  rows: SessionRow[]
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
  const issueIds = issueTabs.map((tab) => tab.issueId)
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
  const tab =
    search.tab === `issue` && search.issue && issueIds.includes(search.issue)
      ? { kind: `issue` as const, issueId: search.issue }
      : search.tab === `guide`
        ? { kind: `guide` as const }
        : search.tab === `session` && focused
          ? { kind: `session` as const, sessionId: focused.id }
          : { kind: `conversation` as const }

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

  const menu = useThreadMenu({ rootId: root.id, issueIds, canEdit: permissions.isMember })

  const chips = issueTabs.flatMap((entry) => {
    const issue = issuesById.get(entry.issueId)
    if (!issue) return []
    const active = tab.kind === `issue` && tab.issueId === entry.issueId
    const status = resolve(issue)
    return [
      <IssueChip
        key={entry.issueId}
        {...menuProps(THREAD_ISSUE_MENU, entry.issueId)}
        role="tab"
        aria-selected={active}
        identifier={entry.identifier}
        title={issue.title}
        size={isMobile ? `pager` : `tab`}
        active={active}
        status={{
          icon: status.icon,
          colorClass: statusColorClass(status),
          colorHex: status.builtinKey ? undefined : status.colorHex,
        }}
        testId={`thread-tab-${entry.identifier}`}
        onClick={() => go({ tab: `issue`, issue: entry.issueId })}
      />,
    ]
  })

  // EXP-1316: the session chips (md+) = the viewed level; hovering the group
  // opens the WHOLE tree (SessionRow small + TreeGuides).
  const treeRows = useMemo<ThreadTreeRow[]>(() => {
    const nowIso = new Date().toISOString()
    return treeOrder(rows.map(toTreeSession)).map((session, index) => ({
      sessionId: session.id,
      parentId: session.parentId,
      name: sessionCard(session, index + 1, nowIso).name,
      agent: rowsById.get(session.id)?.cli ?? null,
      markState: sessionTabMarkState(session),
    }))
  }, [rows, rowsById])
  const sessionChips = view.tabs.flatMap((entry) =>
    entry.kind === `session`
      ? [
          <ThreadSessionTab
            key={entry.sessionId}
            agent={rowsById.get(entry.sessionId)?.cli ?? null}
            markState={entry.markState}
            name={entry.name}
            active={tab.kind === `session` && tab.sessionId === entry.sessionId}
            onSelect={() => go({ tab: `session`, session: entry.sessionId })}
          />,
        ]
      : []
  )
  const sessionGroup =
    !isMobile && sessionChips.length > 0 ? (
      <ThreadSessionGroup
        tree={
          treeRows.length > 1 ? (
            <ThreadSessionTree
              rows={treeRows}
              activeId={tab.kind === `session` ? tab.sessionId : null}
              onOpen={(sessionId) => go({ tab: `session`, session: sessionId })}
            />
          ) : undefined
        }
      >
        {sessionChips}
      </ThreadSessionGroup>
    ) : undefined

  const tabs = (
    <ThreadTabs
      variant={isMobile ? `pager` : `bar`}
      conversation={{
        label: THREAD_COPY.conversation,
        active: tab.kind === `conversation`,
        onSelect: () => go({}),
      }}
      issues={chips}
      add={
        permissions.isMember ? <ThreadTabsAddButton onClick={menu.openPicker} /> : undefined
      }
      sessions={sessionGroup}
      guide={{
        label: THREAD_COPY.guide,
        active: tab.kind === `guide`,
        dot: guideTab?.kind === `guide` && guideTab.dot,
        onSelect: () => go({ tab: `guide` }),
      }}
    />
  )

  // ── The bodies ─────────────────────────────────────────────────────────────
  let body: ReactNode
  if (tab.kind === `session` && focused) {
    body = <InPlaceSession row={focused} />
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
        users={users as User[]}
        canReply={permissions.isMember}
        busy={root.busy}
        onOpenSession={openSession}
        topInset={topInset}
      />
    )
  }

  // EXP-1307: the header's `…` (Open on GitHub, Close PR) on the Guide.
  const guideMenu = (className?: string) =>
    tab.kind === `guide` && guidePr ? (
      <ThreadGuideMenu pr={guidePr} canClose={permissions.isMember} className={className} />
    ) : null

  // EXP-1316: the title is derived (`threadTitle`), never renamed.
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
      const entries = menu.entries({ id: entry.issueId, identifier: entry.identifier })
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

  if (isMobile && tab.kind === `session` && focused) {
    // Phones push the session's own screen (its back header leads to the
    // thread's Conversation).
    return <SessionScreen key={focused.id} teamSlug={teamSlug} row={focused} />
  }

  if (isMobile) {
    return (
      <MenuGestureHost menus={issueMenus}>
        {/* EXP-1316: THE detail chrome — the header band (blur, scrim, edge
            fade) floats OVER the body, which scrolls under it; every body's
            scroller pads its top by the band's measured height. */}
        <div className={MOBILE_DETAIL_SCREEN_CLASS} data-testid="thread-page">
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
            menu={guideMenu(HEADER_BUTTON_CLASS)}
            below={<div className="px-3 pb-2">{tabs}</div>}
          />
          {body}
        </div>
        {menu.dialogs}
      </MenuGestureHost>
    )
  }

  return (
    <MenuGestureHost menus={issueMenus}>
      <div className="flex h-full min-h-0 flex-col" data-testid="thread-page">
        <div className="shrink-0 border-b border-border px-6 pt-6 pb-3 md:px-10">
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1">{title}</div>
            <ThreadGuideMerge pr={guidePr} canMerge={permissions.isMember} placement="header" />
            {focused && focused.status !== `ended` && tab.kind === `session` && (
              <SessionStopPill sessionId={focused.id} />
            )}
            {guideMenu(`shrink-0 text-muted-foreground`)}
          </div>
          <div className="mt-4">{tabs}</div>
        </div>
        {body}
      </div>
      {menu.dialogs}
    </MenuGestureHost>
  )
}

function ConversationBody({
  teamId,
  view,
  users,
  canReply,
  busy,
  onOpenSession,
  topInset = 0,
}: {
  teamId: string
  view: SessionThreadView
  users: User[]
  canReply: boolean
  /** The ROOT's busy: Stop stops the root, so a busy child never shows it. */
  busy: boolean
  /** A session card opens that session in place (EXP-1316). */
  onOpenSession: (sessionId: string) => void
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
  const follow = useFollowScroll([view.rows])
  const isMobile = useIsMobile()
  const composer = canReply ? (
    <SessionComposer
      teamId={teamId}
      sessionId={view.composer.sessionId}
      placeholder={view.composer.placeholder}
      users={users}
      busy={busy}
      canStop
      testId="thread-composer"
      className="mx-auto max-w-3xl"
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
          className={cn(
            `mx-auto w-full max-w-3xl px-4 pt-8 pb-8 md:px-6`,
            isMobile && composer && FLOATING_COMPOSER_CLEARANCE
          )}
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
            viewerId={session?.user?.id ?? null}
            onUnqueue={(id) => void unqueue(id)}
          />
        </div>
      </div>
      <JumpToBottomButton
        visible={!follow.atBottom && view.rows.length > 0}
        className={cn(
          `absolute left-1/2 z-10 -translate-x-1/2`,
          isMobile && composer ? `bottom-[calc(5.5rem+env(safe-area-inset-bottom))]` : `bottom-3`
        )}
        onClick={follow.jumpToBottom}
      />
      </div>
      {/* EXP-1316: phones reply through THE floating composer capsule;
          md+ keeps the in-flow Composer box. */}
      {composer &&
        (isMobile ? (
          <FloatingComposer placeholder={view.composer.placeholder}>{composer}</FloatingComposer>
        ) : (
          <div className="shrink-0 px-4 pt-2 pb-4 md:px-6">{composer}</div>
        ))}
    </div>
  )
}
