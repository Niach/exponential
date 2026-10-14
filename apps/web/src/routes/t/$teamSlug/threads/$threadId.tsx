import { useEffect, useMemo, useState, type ReactNode } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { createFileRoute, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import {
  conceptIcon,
  EmptyState,
  IssueChip,
  Menu,
  MenuGestureHost,
  menuProps,
  SkeletonRows,
  ThreadConversation,
  ThreadTabs,
  ThreadTabsAddButton,
  threadTabChipClass,
  useIsMobile,
  userMessageCaption,
  type MenuSessionProps,
} from "@exp/ui"
import type { Issue, User } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"
import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { SessionComposer } from "@/components/session-composer"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { parseGuideSection, type GuideSectionKey } from "@/components/guide-section-diff"
import {
  ThreadGuide,
  ThreadGuideMerge,
  threadGuidePullRequest,
} from "@/components/thread/thread-guide"
import { useAnswerMessage } from "@/components/thread/agent-channel"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useThreadMenu } from "@/components/thread/thread-menu"
import { ThreadTitle } from "@/components/thread/thread-title"
import { useSessionMessages, useSessionThread } from "@/hooks/use-session-tree"
import { useTeamBoards, useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import {
  THREAD_COPY,
  THREADS_LIST_COPY,
  type MessageRow,
  type PullRequestRow,
  type SessionRow,
  type SessionThreadView,
} from "@/lib/session-tree"

// EXP-1293 Threads v2: a THREAD = a root session (fixture
// `session-thread.json` ×4, `buildSessionThread`). Header: the title (rename
// in place, a lock when private) + ONE tab strip: Conversation · one issue
// chip per linked issue · "+" · Guide (a dot while it holds anything).
// `?tab=` names the tab on show:
//
//   (absent)               the conversation: the root's messages and THE
//                          session card per session, question and plan cards
//                          answered in the card, the composer at the bottom
//   issue&issue=<id>       that issue's detail, embedded
//   guide[&section=&file=] the root's Guide over its pull request (its stack
//                          rail, THE bottom Merge capsule)
//
// Phones: the strip IS the pager under the header. No right sidebar, no
// Show work, no captions.

type ThreadSearch = {
  tab?: `issue` | `guide`
  issue?: string
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
    const tab = search.tab === `issue` || search.tab === `guide` ? search.tab : undefined
    const section = tab === `guide` ? parseGuideSection(search.section) : undefined
    return {
      ...(tab ? { tab } : {}),
      ...(tab === `issue` && pick(search.issue) ? { issue: pick(search.issue) } : {}),
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
  const thread = useSessionThread(threadId)
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
      pullRequests={thread.pullRequests}
    />
  )
}

function ThreadScreen({
  team,
  teamSlug,
  view,
  root,
  pullRequests,
}: {
  team: Team
  teamSlug: string
  view: SessionThreadView
  root: SessionRow
  pullRequests: PullRequestRow[]
}) {
  const navigate = useNavigate()
  const search = Route.useSearch()
  const isMobile = useIsMobile()
  const permissions = useTeamPermissions(team)
  const { users } = useTeamUsers(team.id)
  const boards = useTeamBoards(team.id)
  const { resolve } = useTeamStatusesContext()

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

  const tab =
    search.tab === `issue` && search.issue && issueIds.includes(search.issue)
      ? { kind: `issue` as const, issueId: search.issue }
      : search.tab === `guide`
        ? { kind: `guide` as const }
        : { kind: `conversation` as const }

  const go = (next: ThreadSearch) =>
    void navigate({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug, threadId: root.id },
      search: next,
      replace: true,
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
        size={isMobile ? `sm` : `md`}
        status={{
          icon: status.icon,
          colorClass: statusColorClass(status),
          colorHex: status.builtinKey ? undefined : status.colorHex,
        }}
        testId={`thread-tab-${entry.identifier}`}
        className={threadTabChipClass(active)}
        onClick={() => go({ tab: `issue`, issue: entry.issueId })}
      />,
    ]
  })

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
  if (tab.kind === `issue`) {
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
            showMobileHeader={false}
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
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
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
        {search.section === undefined && (
          <ThreadGuideMerge pr={guidePr} canMerge={permissions.isMember} />
        )}
      </div>
    )
  } else {
    body = (
      <ConversationBody
        teamId={team.id}
        teamSlug={teamSlug}
        view={view}
        users={users as User[]}
        canReply={permissions.isMember}
        busy={root.busy}
      />
    )
  }

  const title = (
    <ThreadTitle
      sessionId={root.id}
      title={view.title}
      stored={root.title}
      isPrivate={view.private}
      canRename={permissions.isMember}
    />
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

  if (isMobile) {
    return (
      <MenuGestureHost menus={issueMenus}>
        <div className="flex h-dvh min-h-0 flex-col" data-testid="thread-page">
          <MobileDetailHeader
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
          {title}
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
  teamSlug,
  view,
  users,
  canReply,
  busy,
}: {
  teamId: string
  teamSlug: string
  view: SessionThreadView
  users: User[]
  canReply: boolean
  /** The ROOT's busy: Stop stops the root, so a busy child never shows it. */
  busy: boolean
}) {
  const navigate = useNavigate()
  const messages = useSessionMessages(view.rootId)
  const messagesById = useMemo(
    () => new Map(messages.map((message: MessageRow) => [message.id, message])),
    [messages]
  )
  const usersById = useMemo(() => new Map(users.map((user) => [user.id, user])), [users])
  const { answer, pending } = useAnswerMessage()

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        <div className="mx-auto w-full max-w-3xl px-4 pt-8 pb-8 md:px-6">
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
            onOpenSession={(sessionId) =>
              void navigate({
                to: `/t/$teamSlug/sessions/$sessionId`,
                params: { teamSlug, sessionId },
              })
            }
            onAnswer={canReply ? (id, value) => void answer(id, { answer: value }) : undefined}
            onDecide={canReply ? (id, decision) => void answer(id, { decision }) : undefined}
            pending={pending}
          />
        </div>
      </div>
      {canReply && (
        <div className="shrink-0 px-4 pt-2 pb-4 md:px-6">
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
        </div>
      )}
    </div>
  )
}
