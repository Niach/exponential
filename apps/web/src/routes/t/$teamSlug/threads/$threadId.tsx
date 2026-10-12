import { useCallback, useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import {
  Button,
  Input,
  Menu,
  MenuGestureHost,
  SkeletonRows,
  ThreadHeader,
  ThreadStrip,
  WORK_COLUMN_CLASS,
  menuProps,
  useIsMobile,
  type ThreadStripEntry,
} from "@exp/ui"
import { useSteerConfig } from "@/components/agent-session"
import { Empty } from "@/components/empty"
import { IssueDetailView } from "@/components/issue-detail-view"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { ThreadConversation } from "@/components/thread/thread-conversation"
import { ThreadGuide } from "@/components/thread/thread-guide"
import { useThreadMenu } from "@/components/thread/thread-menu"
import { ThreadReplyComposer } from "@/components/thread/thread-reply-composer"
import { useFirstSeen, useThreadBundle, type ThreadBundle } from "@/hooks/use-threads"
import { useSession } from "@/hooks/use-session"
import {
  useTeamBoards,
  useTeamBySlug,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { updateWorkTabs } from "@/hooks/use-work-tabs"
import type { Team, User } from "@/db/schema"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { threadItemsCollection, threadsCollection } from "@/lib/collections"
import { emptyStateCopy } from "@/lib/empty-states"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import {
  isFreshThreadChild,
  iso,
  THREAD_SYNC_GRACE_MS,
  threadPageState,
  type ThreadFetchState,
  parseThreadChild,
  selectedThreadChild,
  threadChildSearch,
  threadReplyTarget,
  threadRunStatus,
} from "@/lib/thread-page"
import type { ThreadChild } from "@/lib/thread-view"
import { parseGuideSection, type GuideSectionKey } from "@/lib/work-faces"
import { openNewRunTab } from "@/lib/work-tabs"
import { trpc } from "@/lib/trpc-client"

// EXP-1284: the THREAD page — the unit of work holding n issues and m runs.
// Header: the title (rename inline) + the muted `thread` word; under it the
// CHILD STRIP (`Conversation` · one tab per issue · "+" · Guide) with the
// header caption trailing. `?child=` names the child on show:
//
//   (absent)      the CONVERSATION (`ThreadConversation`): every run as a
//                 block, the issues its agent linked, the parallel card,
//                 and `Reply in this thread` at the bottom;
//   <IDENTIFIER>  that ISSUE's existing detail, embedded;
//   guide         the thread's GUIDE (`threads.results`), `&section=` a
//                 section's diff page under it.
//
// Phones (and web mobile): the strip IS the pager under the header, the "+"
// its trailing button. The "+" and every issue tab's context menu are the
// thread's ONE menu (`useThreadMenu`). Lists, the issue page's `Threads`
// row, the work tab and a run's own page all open this route.

type ThreadSearch = {
  child?: string
  from?: string
  section?: GuideSectionKey
  file?: string
}

/** The issue tabs' context-menu kind (`menuProps`). */
const THREAD_CHILD_MENU = `thread-child`

export const Route = createFileRoute(`/t/$teamSlug/threads/$threadId`)({
  head: () => ({ meta: [{ title: pageTitle(`Thread`) }] }),
  validateSearch: (search: Record<string, unknown>): ThreadSearch => {
    const pick = (value: unknown) =>
      typeof value === `string` && value ? value : undefined
    const section = parseGuideSection(search.section)
    return {
      child: pick(search.child),
      from: pick(search.from),
      ...(section !== undefined ? { section } : {}),
      ...(section !== undefined && pick(search.file) ? { file: pick(search.file) } : {}),
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
  const search = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id
  const { bundle, isReady, now } = useThreadBundle(team?.id, currentUserId, threadId)
  usePageTitle(bundle ? pageTitle(bundle.view.title, `Thread`) : undefined)

  const origin = useMemo(() => parseOrigin(search.from), [search.from])
  const goBack = useCallback(() => {
    void navigate(
      (originListNavigation(teamSlug, origin) ?? {
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
      }) as never
    )
  }, [navigate, teamSlug, origin])

  // Review T14: never a flash of "Thread not found" right after a start —
  // the shell (`threads`) and its items are separate shapes, so while an
  // item names this id, or the one `threads.get` is out / said "found", the
  // skeleton stays (`threadPageState`).
  const { data: namingItems } = useLiveQuery(
    (query) =>
      bundle
        ? undefined
        : query
            .from({ it: threadItemsCollection })
            .where(({ it }) => eq(it.threadId, threadId)),
    [threadId, Boolean(bundle)]
  )
  const itemNamesIt = ((namingItems ?? []) as unknown[]).length > 0
  const [fetchState, setFetchState] = useState<{ id: string; state: ThreadFetchState }>({
    id: threadId,
    state: `idle`,
  })
  const fetchFor = fetchState.id === threadId ? fetchState.state : `idle`
  const settled = Boolean(team && currentUserId && isReady)
  useEffect(() => {
    if (bundle || !settled || fetchFor !== `idle`) return
    let live = true
    setFetchState({ id: threadId, state: `pending` })
    trpc.threads.get
      .query({ id: threadId })
      .then(() => live && setFetchState({ id: threadId, state: `found` }))
      .catch(() => live && setFetchState({ id: threadId, state: `missing` }))
    return () => {
      live = false
    }
  }, [bundle, settled, fetchFor, threadId])
  const [graceOverFor, setGraceOverFor] = useState<string | null>(null)
  const waiting = !bundle && (fetchFor === `found` || itemNamesIt)
  useEffect(() => {
    if (!waiting) return
    const timer = setTimeout(() => setGraceOverFor(threadId), THREAD_SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [waiting, threadId])
  const pageState = threadPageState({
    hasBundle: Boolean(bundle),
    syncSettled: settled,
    itemNamesIt,
    fetch: fetchFor,
    graceOver: graceOverFor === threadId,
  })

  if (!team || !currentUserId || pageState === `skeleton`) {
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }
  if (!bundle) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <MobileDetailHeader title="Thread" onBack={goBack} />
        <div className="flex flex-1 flex-col items-center justify-center">
          <Empty id="thread-not-found">
            <Button asChild variant="outline" size="sm">
              <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
                {emptyStateCopy(`thread-not-found`).action}
              </Link>
            </Button>
          </Empty>
        </div>
      </div>
    )
  }
  return (
    <ThreadScreen
      key={bundle.thread.id}
      team={team}
      teamSlug={teamSlug}
      bundle={bundle}
      currentUserId={currentUserId}
      search={search}
      now={now}
      onBack={goBack}
    />
  )
}

function ThreadScreen({
  team,
  teamSlug,
  bundle,
  currentUserId,
  search,
  now,
  onBack,
}: {
  team: Team
  teamSlug: string
  bundle: ThreadBundle
  currentUserId: string
  search: ThreadSearch
  now: Date
  onBack: () => void
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const permissions = useTeamPermissions(team)
  const { users } = useTeamUsers(team.id)
  const boards = useTeamBoards(team.id)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(permissions.isMember && steerConfig?.enabled)
  const { view } = bundle

  const childKey = useMemo(() => parseThreadChild(search.child), [search.child])
  const child = selectedThreadChild(view, childKey)

  const go = useCallback(
    (next: { child?: string; section?: GuideSectionKey }) => {
      void navigate({
        to: `/t/$teamSlug/threads/$threadId`,
        params: { teamSlug, threadId: bundle.thread.id },
        search: {
          ...(next.child ? { child: next.child } : {}),
          ...(search.from ? { from: search.from } : {}),
          ...(next.section !== undefined ? { section: next.section } : {}),
        },
        replace: true,
      })
    },
    [navigate, teamSlug, bundle.thread.id, search.from]
  )
  const openChild = useCallback(
    (target: ThreadChild) => go({ child: threadChildSearch(target) }),
    [go]
  )
  const openIssueChild = useCallback(
    (issueId: string) => {
      const issue = bundle.issues.find((row) => row.id === issueId)
      if (issue) go({ child: issue.identifier })
    },
    [bundle.issues, go]
  )
  const openRun = useCallback(
    (runId: string) =>
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: runId },
      }),
    [navigate, teamSlug]
  )

  // ── The thread's ONE menu ──────────────────────────────────────────────────
  const menu = useThreadMenu({
    teamId: team.id,
    teamSlug,
    threadId: bundle.thread.id,
    issueIds: view.issueIds,
    canEdit: permissions.isMember,
    canCreate: permissions.canCreate,
    canStart: steerEnabled,
  })
  const [addOpen, setAddOpen] = useState(false)
  const addEntries = menu.entries()

  // ── The strip ──────────────────────────────────────────────────────────────
  const itemCreatedAt = useMemo(() => {
    const map = new Map<string, string>()
    for (const item of bundle.items) {
      if (item.issueId) map.set(item.issueId, iso(item.createdAt))
    }
    return map
  }, [bundle.items])
  const firstSeen = useFirstSeen(view.issueIds)
  const [seenFresh, setSeenFresh] = useState<ReadonlySet<string>>(() => new Set())
  const issuesById = new Map(bundle.issues.map((issue) => [issue.id, issue]))
  const entries: ThreadStripEntry[] = view.strip.flatMap((entry): ThreadStripEntry[] => {
    switch (entry.kind) {
      case `conversation`:
        return [
          {
            kind: `conversation`,
            active: child.kind === `conversation`,
            onSelect: () => openChild(entry),
          },
        ]
      case `issue`: {
        const issue = issuesById.get(entry.issueId)
        const fresh =
          !seenFresh.has(entry.issueId) &&
          isFreshThreadChild(itemCreatedAt.get(entry.issueId), firstSeen.get(entry.issueId) ?? 0)
        return [
          {
            kind: `issue`,
            key: entry.issueId,
            identifier: entry.identifier,
            title: entry.title,
            lead: issue ? <IssueStatusIcon issue={issue} className="size-3.5!" /> : null,
            active: child.kind === `issue` && child.issueId === entry.issueId,
            fresh,
            onSelect: () => {
              setSeenFresh((current) => new Set([...current, entry.issueId]))
              openChild(entry)
            },
            menu: { ...menuProps(THREAD_CHILD_MENU, entry.issueId) },
          },
        ]
      }
      case `guide`:
        return [
          {
            kind: `guide`,
            active: child.kind === `guide`,
            dot: entry.dot,
            onSelect: () => openChild(entry),
          },
        ]
      default:
        return []
    }
  })
  const strip = (
    <ThreadStrip
      entries={entries}
      caption={view.headerCaption}
      variant={isMobile ? `pager` : `bar`}
      addMenu={
        addEntries.length > 0
          ? (button) => (
              <Menu
                open={addOpen}
                onOpenChange={setAddOpen}
                align="start"
                aria-label="Thread"
                title={view.title}
                contentClassName="w-64"
                entries={addEntries}
                trigger={button}
              />
            )
          : undefined
      }
    />
  )

  // ── The reply box ──────────────────────────────────────────────────────────
  const replyTarget = threadReplyTarget({
    runs: bundle.runs.map((run) => ({
      id: run.id,
      userId: run.userId,
      status: threadRunStatus(run, now),
      startedAt: iso(run.startedAt),
    })),
    viewerId: currentUserId,
    canSteer: steerEnabled,
  })
  const resumeRun =
    replyTarget?.kind === `resume`
      ? (bundle.runs.find((run) => run.id === replyTarget.runId) ?? null)
      : null
  const startWithText = (text: string) => {
    updateWorkTabs(team.id, openNewRunTab)
    void navigate({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug },
      search: { thread: bundle.thread.id, text } as never,
    })
  }

  // ── The children ───────────────────────────────────────────────────────────
  let body: React.ReactNode
  if (child.kind === `issue`) {
    const issue = issuesById.get(child.issueId)
    const board = issue ? boards.find((row) => row.id === issue.boardId) : undefined
    body =
      issue && board ? (
        <IssueDetailView
          key={issue.id}
          issue={issue}
          users={users as User[]}
          board={board}
          teamSlug={teamSlug}
          teamId={team.id}
          readOnly={!permissions.canMutateIssue(issue)}
          origin={search.from}
          mobileWork={isMobile ? { tabs: strip } : undefined}
        />
      ) : (
        <div className="p-6">
          <SkeletonRows />
        </div>
      )
  } else if (child.kind === `guide`) {
    body = (
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        <div className={WORK_COLUMN_CLASS}>
          <ThreadGuide
            bundle={bundle}
            section={search.section}
            file={search.file ?? null}
            onSection={(section) => go({ child: `guide`, section })}
            canMerge={permissions.isMember}
            steerEnabled={steerConfig?.enabled === true}
          />
        </div>
      </div>
    )
  } else {
    body = (
      <div className="relative flex min-h-0 flex-1 flex-col">
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
          <div className="mx-auto w-full max-w-2xl px-4 pt-6 pb-8 md:px-5">
            <ThreadConversation
              bundle={bundle}
              currentUserId={currentUserId}
              now={now}
              onOpenIssue={openIssueChild}
              onOpenRun={openRun}
            />
          </div>
        </div>
        {replyTarget && (
          <div className="shrink-0 px-4 pt-2 pb-4 md:px-5">
            <ThreadReplyComposer
              className="mx-auto w-full max-w-2xl"
              target={replyTarget}
              resumeRun={resumeRun}
              onStart={startWithText}
            />
          </div>
        )}
      </div>
    )
  }

  const title = (
    <ThreadTitle
      threadId={bundle.thread.id}
      title={view.title}
      stored={bundle.thread.title}
      canRename={permissions.isMember}
    />
  )

  // The issue tabs' context menu: the same entries + Unlink <IDENT>.
  const childMenus = {
    [THREAD_CHILD_MENU]: ({
      target,
      open,
      onOpenChange,
    }: {
      target: { id: string; anchor: { x: number; y: number; width: number; height: number }; origin: HTMLElement | null }
      open: boolean
      onOpenChange: (open: boolean) => void
    }) => {
      const issue = issuesById.get(target.id)
      if (!issue) return null
      return (
        <Menu
          mode="pointer"
          anchor={target.anchor}
          open={open}
          onOpenChange={onOpenChange}
          returnFocus={target.origin}
          aria-label={issue.identifier}
          title={issue.identifier}
          contentClassName="w-64"
          entries={menu.entries({ id: issue.id, identifier: issue.identifier })}
        />
      )
    },
  }

  if (isMobile) {
    // The issue child wears the issue's own Work screen header (its back, its
    // `…`), the pager under it.
    if (child.kind === `issue`) {
      return (
        <MenuGestureHost menus={childMenus}>
          {body}
          {menu.dialogs}
        </MenuGestureHost>
      )
    }
    return (
      <MenuGestureHost menus={childMenus}>
        <div className="flex h-dvh min-h-0 flex-col" data-testid="thread-page">
          <MobileDetailHeader
            title={<span className="truncate">{view.title}</span>}
            onBack={onBack}
            below={<div className="px-3 pb-2">{strip}</div>}
          />
          {body}
        </div>
        {menu.dialogs}
      </MenuGestureHost>
    )
  }

  return (
    <MenuGestureHost menus={childMenus}>
      <div className="flex h-full min-h-0 flex-col" data-testid="thread-page">
        <div className="shrink-0 border-b border-border px-6 pt-5 pb-2 md:px-8">
          <ThreadHeader title={title} />
          <div className="mt-3">{strip}</div>
        </div>
        {body}
      </div>
      {menu.dialogs}
    </MenuGestureHost>
  )
}

/** The title, renamed in place (`threads.update`; an empty title clears it
 *  back to the derived one). */
function ThreadTitle({
  threadId,
  title,
  stored,
  canRename,
}: {
  threadId: string
  title: string
  stored: string | null
  canRename: boolean
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(``)
  if (!canRename) return <>{title}</>
  if (!editing) {
    return (
      <button
        type="button"
        className="max-w-full truncate rounded-sm text-left hover:bg-accent/40"
        title="Rename"
        data-testid="thread-title"
        onClick={() => {
          setDraft(stored ?? title)
          setEditing(true)
        }}
      >
        {title}
      </button>
    )
  }
  const save = async () => {
    setEditing(false)
    const next = draft.trim()
    if (next === (stored ?? ``).trim() || (!stored && next === title)) return
    try {
      const result = await trpc.threads.update.mutate(
        { id: threadId, title: next || null },
        SKIP_ERROR_TOAST
      )
      await threadsCollection.utils.awaitTxId(result.txid)
    } catch (error) {
      failedAction(`rename`, `the thread`, error)
    }
  }
  return (
    <Input
      autoFocus
      value={draft}
      aria-label="Thread title"
      className="h-8 text-lg font-semibold md:text-xl"
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void save()}
      onKeyDown={(event) => {
        if (event.key === `Enter`) (event.target as HTMLInputElement).blur()
        if (event.key === `Escape`) setEditing(false)
      }}
    />
  )
}
