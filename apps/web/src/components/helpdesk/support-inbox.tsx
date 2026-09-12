import { useCallback, useEffect, useRef, useState } from "react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  Check,
  ExternalLink,
  Info,
  LifeBuoy,
  LoaderCircle,
  Lock,
  Mail,
  MailWarning,
  RotateCcw,
  StickyNote,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { issueCollection, boardCollection } from "@/lib/collections"
import { relativeTime } from "@/components/comment-rows/format"
import { isReporterActivelyViewing } from "@/lib/helpdesk/presence"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { displayUserName } from "@/lib/user-display"
import { useTeamUsers } from "@/hooks/use-team-data"
import { Button } from "@/components/ui/button"
import { Pill } from "@/components/ui/pill"
import {
  Composer,
  ComposerSubmit,
} from "@/components/composer"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import {
  SEGMENTED_ROW,
  SEGMENTED_ROW_COMPACT,
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"

// EXP-525: the Open/Resolved pills carry the shared registry's support glyphs,
// so the tabs read the same here as in the desktop IDE.
// EXP-698: the composer's send glyph is the shared concept, not a raw lucide
// import — the natives draw the same one.

const TAB_ICON = {
  open: conceptIcon(`support-open`),
  resolved: conceptIcon(`support-resolved`),
} as const

type ThreadRow = Awaited<
  ReturnType<typeof trpc.helpdesk.listThreads.query>
>[number]
type ThreadDetail = Awaited<ReturnType<typeof trpc.helpdesk.getThread.query>>
/** What the details rail and the header need off a ticket — satisfied by both
 *  a list row and the loaded conversation's own thread row. */
type ThreadHeader = ThreadDetail[`thread`]
type WidgetSubmissionRow = Awaited<
  ReturnType<typeof trpc.widgets.submissionForThread.query>
>

const LIST_POLL_MS = 30_000
const THREAD_POLL_MS = 15_000
// One page of threads. The poll only ever refreshes the FIRST page; older
// pages are loaded on demand and merged (REV2-40 — nothing purges resolved
// threads, so the Resolved tab grows without bound).
const PAGE_SIZE = 50

function reporterLabel(row: {
  reporterName: string | null
  reporterEmail: string
}): string {
  return row.reporterName || row.reporterEmail
}

// EXP-851: the helpdesk is TWO surfaces now, not a 3-pane view — a LIST
// (`/t/$teamSlug/support`, and the same rows in the sidebar's list nav) and a
// CONVERSATION on its own route (`/t/$teamSlug/support/$threadId`), so a
// ticket is a URL you can share, refresh and come back to. Threads/messages
// are server-only tables (no Electric shape), so both poll tRPC. The ticket's
// details rail stays beside the conversation on lg+ and behind the header's
// info button below it.

type SupportFilter = `open` | `resolved`

/** The team's threads in one tab, polled — the list view and the sidebar's
 *  list nav render the SAME component, only narrower. */
export function SupportThreadList({
  teamId,
  teamSlug,
  activeThreadId = null,
  compact = false,
}: {
  teamId: string
  teamSlug: string
  /** The conversation that is open, for the highlighted row. */
  activeThreadId?: string | null
  /** The sidebar's 16rem slot: no reading column, tighter strip padding. */
  compact?: boolean
}) {
  const [filter, setFilter] = useState<SupportFilter>(`open`)
  const [page, setPage] = useState<ThreadRow[] | null>(null)
  const [olderPages, setOlderPages] = useState<ThreadRow[]>([])
  // `pageFull` says the newest page filled up, `exhausted` that a "load
  // older" fetch hit the end — kept apart so the 30s poll (which always
  // refetches a full first page) can't resurrect the button.
  const [pageFull, setPageFull] = useState(false)
  const [exhausted, setExhausted] = useState(false)
  const [loadingMore, setLoadingMore] = useState(false)

  const loadThreads = useCallback(async () => {
    try {
      const rows = await trpc.helpdesk.listThreads.query({
        teamId,
        filter,
        limit: PAGE_SIZE,
      })
      setPage(rows)
      setPageFull(rows.length === PAGE_SIZE)
    } catch (err) {
      console.error(`helpdesk list failed`, err)
    }
  }, [teamId, filter])

  useEffect(() => {
    setPage(null)
    setOlderPages([])
    setPageFull(false)
    setExhausted(false)
    void loadThreads()
    const timer = setInterval(() => void loadThreads(), LIST_POLL_MS)
    return () => clearInterval(timer)
  }, [loadThreads])

  // Opening the Support surface clears the team's unread support_reply
  // notifications (REV2-13). The badge sits ON this entry and nothing else can
  // clear it: those rows have no issue, so markReadByIssue never matches them.
  // Fire-and-forget; a failure just leaves the badge lit.
  useEffect(() => {
    void trpc.notifications.markReadSupport
      .mutate({ teamId })
      .catch(() => {})
  }, [teamId])

  const pageRows = page ?? []
  const pageIds = new Set(pageRows.map((row) => row.id))
  const threads = page
    ? [...pageRows, ...olderPages.filter((row) => !pageIds.has(row.id))]
    : null

  const hasMore = pageFull && !exhausted

  const loadMore = async () => {
    const last = threads?.[threads.length - 1]
    if (!last || loadingMore) return
    setLoadingMore(true)
    try {
      const rows = await trpc.helpdesk.listThreads.query({
        teamId,
        filter,
        limit: PAGE_SIZE,
        cursor: new Date(last.updatedAt).toISOString(),
      })
      setOlderPages((current) => [...current, ...rows])
      if (rows.length < PAGE_SIZE) setExhausted(true)
    } catch (err) {
      console.error(`helpdesk list failed`, err)
    } finally {
      setLoadingMore(false)
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* EXP-851: the Open/Resolved strip and the Inbox / My issues strip are
          ONE control — same trigger sizing, same row padding. */}
      <div className={compact ? SEGMENTED_ROW_COMPACT : SEGMENTED_ROW}>
        <Tabs
          value={filter}
          onValueChange={(value) => setFilter(value as SupportFilter)}
          className="w-fit shrink-0"
        >
          <TabsList>
            {([`open`, `resolved`] as const).map((tab) => {
              const TabIcon = TAB_ICON[tab]
              return (
                <TabsTrigger key={tab} value={tab} className={SEGMENTED_TAB}>
                  <TabIcon />
                  {tab === `open` ? `Open` : `Resolved`}
                </TabsTrigger>
              )
            })}
          </TabsList>
        </Tabs>
      </div>
      <div className={cn(`min-h-0 flex-1 overflow-y-auto`, TAB_BAR_CLEARANCE)}>
        <div
          className={cn(
            `flex w-full flex-col`,
            compact ? `p-2` : `mx-auto max-w-3xl px-4 py-2`
          )}
        >
          {threads === null ? (
            <div className="flex items-center justify-center py-10">
              <LoaderCircle className="h-4 w-4 animate-spin text-muted-foreground" />
            </div>
          ) : threads.length === 0 ? (
            compact ? (
              <div className="px-3 py-2 text-xs text-muted-foreground">
                {filter === `open`
                  ? `No open conversations.`
                  : `No resolved conversations yet.`}
              </div>
            ) : (
              <div className="flex flex-col items-center gap-2 px-4 py-10 text-center">
                <LifeBuoy className="h-6 w-6 text-muted-foreground" />
                <p className="text-sm text-muted-foreground">
                  {filter === `open`
                    ? `No open conversations.`
                    : `No resolved conversations yet.`}
                </p>
              </div>
            )
          ) : (
            threads.map((thread) => (
              <ListRow
                key={thread.id}
                asChild
                active={thread.id === activeThreadId}
                interactive={thread.id !== activeThreadId}
                className="w-full flex-col items-stretch gap-0 px-3 py-2.5 text-left"
                data-testid={`support-row-${thread.id}`}
              >
                <Link
                  to="/t/$teamSlug/support/$threadId"
                  params={{ teamSlug, threadId: thread.id }}
                  search={{ from: `support` }}
                >
                  {/* EXP-715: the ticket SUBJECT leads (every client); the
                      reporter + latest public message sit under it. */}
                  <div className="flex items-center gap-2">
                    <span
                      className={cn(
                        `min-w-0 flex-1 truncate text-sm`,
                        thread.unread && `font-medium`
                      )}
                    >
                      {thread.title}
                    </span>
                    {/* EXP-698: fixed trailing columns — the stamp is
                        right-aligned in its own slot and the unread dot keeps
                        its 8px slot whether or not it is lit, so read and
                        unread rows line up exactly. 6rem, not the inbox's 4:
                        helpdesk prints the long form ("2 minutes ago"). */}
                    <span className="w-24 shrink-0 truncate text-right text-[0.65rem] text-muted-foreground">
                      {relativeTime(thread.updatedAt)}
                    </span>
                    <span className="w-2 shrink-0">
                      {thread.unread && (
                        <span
                          className="block h-2 w-2 rounded-full bg-primary"
                          aria-label="Awaiting reply"
                        />
                      )}
                    </span>
                  </div>
                  <p className="mt-0.5 truncate text-xs text-muted-foreground">
                    {thread.lastMessage?.body
                      ? `${reporterLabel(thread)} · ${thread.lastMessage.body}`
                      : reporterLabel(thread)}
                  </p>
                </Link>
              </ListRow>
            ))
          )}
          {threads !== null && threads.length > 0 && hasMore && (
            <div className="px-2 pb-2 pt-2">
              <Pill
                size="sm"
                mode="action"
                className="w-full"
                leading={
                  loadingMore ? (
                    <LoaderCircle className="size-3 animate-spin" />
                  ) : undefined
                }
                disabled={loadingMore}
                onClick={() => void loadMore()}
              >
                Load older conversations
              </Pill>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

/** EXP-851: one ticket on its own route — the conversation in the main
 *  column, the sidebar showing the Support list nav beside it. The header is
 *  the shared `MobileDetailHeader` (round back, centred subject, round info
 *  button), the same bar the issue, review and session details wear. */
export function SupportConversation({
  threadId,
  teamId,
  teamSlug,
  onBack,
  onChanged,
}: {
  threadId: string
  teamId: string
  teamSlug: string
  onBack: () => void
  /** The list behind this conversation, when there is one to refresh. */
  onChanged?: () => Promise<void>
}) {
  const [detail, setDetail] = useState<ThreadDetail | null>(null)
  const [draft, setDraft] = useState(``)
  const [mode, setMode] = useState<`reply` | `note`>(`reply`)
  const [sending, setSending] = useState(false)
  const [statusBusy, setStatusBusy] = useState(false)
  // Below lg the details rail has no room — the same content opens in a
  // Sheet from the header's info button instead (phone web keeps escalate +
  // linked-issue parity with the native apps).
  const [detailsOpen, setDetailsOpen] = useState(false)
  const bottomRef = useRef<HTMLDivElement | null>(null)
  const { userMap } = useTeamUsers(teamId)

  const loadDetail = useCallback(async () => {
    try {
      setDetail(await trpc.helpdesk.getThread.query({ threadId }))
    } catch (err) {
      console.error(`helpdesk thread load failed`, err)
    }
  }, [threadId])

  useEffect(() => {
    void loadDetail()
    const timer = setInterval(() => void loadDetail(), THREAD_POLL_MS)
    return () => clearInterval(timer)
  }, [loadDetail])

  // Opening a conversation clears the team's unread support notifications
  // (REV2-13) — the list used to do this on selection.
  useEffect(() => {
    void trpc.notifications.markReadSupport
      .mutate({ teamId })
      .catch(() => {})
  }, [teamId, threadId])

  const thread = detail?.thread ?? null

  const messageCount = detail?.messages.length ?? 0
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: `end` })
  }, [messageCount])

  const send = async () => {
    const body = draft.trim()
    if (!body || sending || !detail) return
    setSending(true)
    try {
      if (mode === `reply`) {
        await trpc.helpdesk.reply.mutate({ threadId, body })
      } else {
        await trpc.helpdesk.note.mutate({ threadId, body })
      }
      setDraft(``)
      await Promise.all([loadDetail(), onChanged?.()])
    } catch (err) {
      console.error(`helpdesk send failed`, err)
    } finally {
      setSending(false)
    }
  }

  const isResolved = detail?.thread.status === `resolved`

  const toggleClosed = async () => {
    if (statusBusy) return
    setStatusBusy(true)
    try {
      if (isResolved) {
        await trpc.helpdesk.reopen.mutate({ threadId })
      } else {
        await trpc.helpdesk.close.mutate({ threadId })
      }
      await Promise.all([loadDetail(), onChanged?.()])
    } catch (err) {
      console.error(`helpdesk close/reopen failed`, err)
    } finally {
      setStatusBusy(false)
    }
  }

  return (
    <div className="flex h-full min-h-0">
      {/* Middle — chat thread. On mobile the floating tab bar stays
          visible, so the column reserves clearance to keep the composer
          above it. */}
      <div className={`flex min-w-0 flex-1 flex-col ${TAB_BAR_CLEARANCE}`}>
        {/* EXP-851: the shared detail header — round back, the ticket's
            subject centred, the round info button in the trailing slot. The
            Close/Reopen control sits on the line below, where the actions of
            every other detail live. */}
        <MobileDetailHeader
          title={thread?.title ?? `Ticket`}
          onBack={onBack}
          backLabel="Back to conversations"
          menu={
            <Button
              variant="ghost"
              size="icon"
              className="size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground lg:hidden"
              onClick={() => setDetailsOpen(true)}
              aria-label="Ticket details"
            >
              <Info />
            </Button>
          }
        />
        <div className="flex items-center gap-2 border-b px-3 py-2">
          <p className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
            {thread ? reporterLabel(thread) : ``}
          </p>
          <Pill
            size="sm"
            mode="action"
            className="shrink-0"
            leading={
              statusBusy ? (
                <LoaderCircle className="size-3 animate-spin" />
              ) : isResolved ? (
                <RotateCcw className="size-3" />
              ) : (
                <Check className="size-3" />
              )
            }
            disabled={statusBusy || detail === null}
            onClick={() => void toggleClosed()}
          >
            {isResolved ? `Reopen ticket` : `Close ticket`}
          </Pill>
        </div>

        <div className="flex flex-1 flex-col gap-3 overflow-y-auto px-4 py-4">
          {detail === null ? (
            <div className="flex flex-1 items-center justify-center">
              <LoaderCircle className="h-4 w-4 animate-spin text-muted-foreground" />
            </div>
          ) : (
            detail.messages.map((message) => {
              const isInbound = message.direction === `inbound`
              const isInternal = message.visibility === `internal`
              // REV2-10: the composer promises the reply is "emailed to
              // them", and the emailed magic link is the reporter's only way
              // back into the conversation — so a bounced/refused send has to
              // be visible here. No marker when nothing was attempted (the
              // engagement gate holds replies until the reporter opens the
              // link once).
              const deliveryFailed =
                !isInbound &&
                !isInternal &&
                (message.emailDeliveryStatus === `failed` ||
                  message.emailDeliveryStatus === `suppressed` ||
                  message.emailDeliveryStatus === `bounced` ||
                  message.emailDeliveryStatus === `complained`)
              const author = isInbound
                ? (thread ? reporterLabel(thread) : `Reporter`)
                : displayUserName(
                    message.authorUserId
                      ? userMap.get(message.authorUserId)
                      : undefined,
                    message.authorUserId
                  )
              return (
                <div
                  key={message.id}
                  className={`max-w-[85%] rounded-2xl px-3.5 py-2.5 text-sm ${
                    isInbound
                      ? `self-start rounded-bl-sm border border-glass-stroke bg-glass-row`
                      : isInternal
                        ? `self-end rounded-br-sm border border-amber-500/40 bg-amber-500/10`
                        : `self-end rounded-br-sm bg-primary text-primary-foreground`
                  }`}
                >
                  {isInternal && (
                    <Pill
                      mode="readonly"
                      size="sm"
                      className="mb-1 border-amber-500/50 bg-transparent text-amber-500"
                      leading={<StickyNote className="size-2.5" />}
                    >
                      Internal
                    </Pill>
                  )}
                  <p className="whitespace-pre-wrap break-words">
                    {message.body}
                  </p>
                  {/* EXP-698: the outgoing bubble is filled with `primary`,
                      which is near-WHITE — `text-white/70` on it was invisible.
                      The meta line reads on the fill it actually sits on. */}
                  <p
                    className={`mt-1 text-[0.65rem] ${
                      isInbound || isInternal
                        ? `text-muted-foreground`
                        : `text-black/60`
                    }`}
                  >
                    {author} · {relativeTime(message.createdAt)}
                  </p>
                  {deliveryFailed && (
                    <p className="mt-1 flex items-center gap-1 text-[0.65rem] font-medium text-destructive">
                      <MailWarning className="h-3 w-3" />
                      Email delivery failed. The reporter didn&apos;t receive
                      this.
                    </p>
                  )}
                </div>
              )
            })
          )}
          <div ref={bottomRef} />
        </div>

        <div className="border-t px-3 py-2.5">
          {/* EXP-698: the ONE composer card, with the Reply/Note toggle as its
              leading row. Note mode tints only the card's hairline. */}
          <Composer
            className={cn(mode === `note` && `border-amber-500/40`)}
            leading={
              <>
                <Pill
                  size="sm"
                  mode="select"
                  selected={mode === `reply`}
                  leading={<Mail className="size-3" />}
                  onClick={() => setMode(`reply`)}
                >
                  Reply
                </Pill>
                <Pill
                  size="sm"
                  mode="select"
                  selected={mode === `note`}
                  leading={<StickyNote className="size-3" />}
                  className={cn(
                    mode === `note` &&
                      `border-amber-500/40 bg-amber-500/15 text-amber-500`
                  )}
                  onClick={() => setMode(`note`)}
                >
                  Internal note
                </Pill>
              </>
            }
            submit={
              <ComposerSubmit
                disabled={sending || draft.trim().length === 0}
                onClick={() => void send()}
                aria-label={mode === `reply` ? `Send reply` : `Save note`}
              >
                {sending ? (
                  <LoaderCircle className="size-5 animate-spin" />
                ) : undefined}
              </ComposerSubmit>
            }
          >
            <Textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === `Enter` && !event.shiftKey) {
                  event.preventDefault()
                  void send()
                }
              }}
              placeholder={
                mode === `reply`
                  ? `Reply to ${thread ? reporterLabel(thread) : `the reporter`}… (emailed to them)`
                  : `Add an internal note… (never sent to the reporter)`
              }
              rows={2}
              className="min-h-16 border-none bg-transparent text-sm shadow-none focus-visible:border-transparent dark:bg-transparent"
            />
          </Composer>
        </div>
      </div>

      {/* Right — details rail (≥lg) */}
      <div className="hidden w-72 shrink-0 flex-col gap-4 overflow-y-auto border-l px-4 py-4 lg:flex">
        {thread && (
          <ThreadDetails
            thread={thread}
            teamId={teamId}
            teamSlug={teamSlug}
            onEscalated={async () => {
              await Promise.all([loadDetail(), onChanged?.()])
            }}
          />
        )}
      </div>

      {/* Below lg the same details open in a sheet from the header. */}
      <Sheet open={detailsOpen} onOpenChange={setDetailsOpen}>
        <SheetContent
          side="right"
          className="flex w-80 flex-col gap-4 overflow-y-auto px-4 py-4"
        >
          <SheetHeader className="p-0">
            <SheetTitle className="text-sm">Ticket details</SheetTitle>
          </SheetHeader>
          {thread && (
            <ThreadDetails
              thread={thread}
              teamId={teamId}
              teamSlug={teamSlug}
              onEscalated={async () => {
                await Promise.all([loadDetail(), onChanged?.()])
              }}
            />
          )}
        </SheetContent>
      </Sheet>
    </div>
  )
}

// The ticket's metadata + actions: reporter block, widget context, escalate
// board picker / linked-issue chip. Rendered twice — in the ≥lg details rail
// and in the <lg details sheet — so every viewport can escalate and reach
// the linked issue.
function ThreadDetails({
  thread,
  teamId,
  teamSlug,
  onEscalated,
}: {
  thread: ThreadHeader
  teamId: string
  teamSlug: string
  onEscalated: () => Promise<void>
}) {
  // The escalated issue (when one exists) is Electric-synced — resolve
  // identifier/title from the live row so the chip stays fresh.
  const linkedIssueId = thread.linkedIssueId ?? undefined
  const { data: issueRows } = useLiveQuery(
    (query) =>
      linkedIssueId
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.id, linkedIssueId))
        : undefined,
    [linkedIssueId]
  )
  const issue = issueRows?.[0]

  const issueBoardId = issue?.boardId
  const { data: boardRows } = useLiveQuery(
    (query) =>
      issueBoardId
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.id, issueBoardId))
        : undefined,
    [issueBoardId]
  )
  const board = boardRows?.[0]

  // Escalation board picker: the team's live (non-trashed) boards.
  const { data: allBoards } = useLiveQuery(
    (query) =>
      query
        .from({ boards: boardCollection })
        .where(({ boards }) => eq(boards.teamId, teamId)),
    [teamId]
  )
  const boards = (allBoards ?? []).filter((row) => !row.deletedAt)
  const [escalateBoardId, setEscalateBoardId] = useState<string>(``)
  const [escalating, setEscalating] = useState(false)
  const [escalateError, setEscalateError] = useState<string | null>(null)

  const escalate = async () => {
    if (!escalateBoardId || escalating) return
    setEscalating(true)
    setEscalateError(null)
    try {
      await trpc.helpdesk.escalate.mutate({
        threadId: thread.id,
        boardId: escalateBoardId,
      })
      await onEscalated()
    } catch (err) {
      setEscalateError(
        err instanceof Error ? err.message : `Couldn't create the issue`
      )
    } finally {
      setEscalating(false)
    }
  }

  const [submission, setSubmission] = useState<WidgetSubmissionRow | null>(
    null
  )
  useEffect(() => {
    let cancelled = false
    void trpc.widgets.submissionForThread
      .query({ threadId: thread.id })
      .then((row) => {
        if (!cancelled) setSubmission(row)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [thread.id])

  return (
    <>
      <section>
        <GlassSectionHeader label="Reporter" className="px-0 pt-0" />
        <p className="text-sm font-medium">{reporterLabel(thread)}</p>
        <p className="truncate text-xs text-muted-foreground">
          {thread.reporterEmail}
        </p>
        {/* "Viewing now" = the reporter's page is live-polling (their reply
            emails are suppressed while it is). Trails reality by up to the
            15s thread poll — the authoritative gate runs server-side at
            send time. */}
        {isReporterActivelyViewing(thread.lastReporterSeenAt) ? (
          <p className="mt-1 text-xs text-emerald-500">Viewing now</p>
        ) : (
          thread.lastReporterSeenAt && (
            <p className="mt-1 text-xs text-muted-foreground">
              Last seen {relativeTime(thread.lastReporterSeenAt)}
            </p>
          )
        )}
      </section>

      {submission && (
        <section>
          <GlassSectionHeader label="Context" className="px-0 pt-0" />
          {submission.pageUrl && (
            <p
              className="truncate text-xs text-muted-foreground"
              title={submission.pageUrl}
            >
              {submission.pageUrl}
            </p>
          )}
          {submission.userAgent && (
            <p
              className="mt-1 line-clamp-2 text-xs text-muted-foreground"
              title={submission.userAgent}
            >
              {submission.userAgent}
            </p>
          )}
          {submission.viewportWidth && submission.viewportHeight && (
            <p className="mt-1 text-xs text-muted-foreground">
              Viewport {submission.viewportWidth}×{submission.viewportHeight}
            </p>
          )}
        </section>
      )}

      {thread.linkedIssueId ? (
        issue &&
        board && (
          <section>
            <GlassSectionHeader label="Linked issue" className="px-0 pt-0" />
            <Link
              to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
              params={{
                teamSlug,
                boardSlug: board.slug,
                issueIdentifier: issue.identifier,
              }}
              className="inline-flex items-center gap-1 text-sm font-medium hover:underline"
            >
              {issue.identifier}
              <ExternalLink className="h-3 w-3 text-muted-foreground" />
            </Link>
            <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">
              {issue.title}
            </p>
          </section>
        )
      ) : (
        <section>
          <GlassSectionHeader label="Escalate" className="px-0 pt-0" />
          <p className="mb-2 text-xs text-muted-foreground">
            Create an issue from this ticket on one of the team&apos;s boards.
          </p>
          <div className="flex flex-col gap-1.5">
            <Select
              value={escalateBoardId}
              onValueChange={setEscalateBoardId}
            >
              <SelectTrigger className="h-8 text-xs">
                <SelectValue placeholder="Pick a board" />
              </SelectTrigger>
              <SelectContent>
                {boards.map((board) => (
                  <SelectItem key={board.id} value={board.id}>
                    {board.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              size="sm"
              className="h-8"
              disabled={!escalateBoardId || escalating}
              onClick={() => void escalate()}
            >
              {escalating ? (
                <LoaderCircle className="size-3 animate-spin" />
              ) : null}
              Create issue
            </Button>
            {escalateError && (
              <p className="text-xs text-destructive">{escalateError}</p>
            )}
          </div>
        </section>
      )}

      <section className="mt-auto">
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Lock className="h-3 w-3" />
          Replies are emailed to the reporter with a private conversation
          link.
        </p>
      </section>
    </>
  )
}
