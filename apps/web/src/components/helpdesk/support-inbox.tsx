import { useCallback, useEffect, useRef, useState } from "react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  ArrowLeft,
  Check,
  ExternalLink,
  Info,
  LifeBuoy,
  Loader2,
  Lock,
  Mail,
  RotateCcw,
  Send,
  StickyNote,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { issueCollection, boardCollection } from "@/lib/collections"
import { relativeTime } from "@/components/comment-rows/format"
import { displayUserName } from "@/lib/user-display"
import { useTeamUsers } from "@/hooks/use-team-data"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
import { Textarea } from "@/components/ui/textarea"

type ThreadRow = Awaited<
  ReturnType<typeof trpc.helpdesk.listThreads.query>
>[number]
type ThreadDetail = Awaited<ReturnType<typeof trpc.helpdesk.getThread.query>>
type WidgetSubmissionRow = Awaited<
  ReturnType<typeof trpc.widgets.submissionForThread.query>
>

const LIST_POLL_MS = 30_000
const THREAD_POLL_MS = 15_000

function reporterLabel(row: {
  reporterName: string | null
  reporterEmail: string
}): string {
  return row.reporterName || row.reporterEmail
}

// Featurebase-style 3-pane support inbox (EXP-128; EXP-180 made threads
// standalone). Threads/messages are server-only tables (no Electric shape),
// so the list and the open thread poll tRPC. A thread carries its own
// open/resolved status; an issue exists only once a member escalates the
// ticket — the escalated issue IS synced, so the details rail resolves it
// live from the issues collection. On small screens the list and the
// conversation stack (back button) and the details rail becomes a sheet
// behind the header's info button — escalate/linked-issue stay reachable
// on every viewport.
export function SupportInbox({
  teamId,
  teamSlug,
}: {
  teamId: string
  teamSlug: string
}) {
  const [filter, setFilter] = useState<`open` | `resolved`>(`open`)
  const [threads, setThreads] = useState<ThreadRow[] | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const loadThreads = useCallback(async () => {
    try {
      const rows = await trpc.helpdesk.listThreads.query({
        teamId,
        filter,
      })
      setThreads(rows)
    } catch (err) {
      console.error(`helpdesk list failed`, err)
    }
  }, [teamId, filter])

  useEffect(() => {
    setThreads(null)
    void loadThreads()
    const timer = setInterval(() => void loadThreads(), LIST_POLL_MS)
    return () => clearInterval(timer)
  }, [loadThreads])

  // Keep the selection valid as rows move between the Open/Resolved tabs.
  const selected =
    threads?.find((thread) => thread.id === selectedId) ?? null

  return (
    <div className="flex h-full min-h-0">
      {/* Left — conversation list */}
      <div
        className={`w-full shrink-0 flex-col border-r md:flex md:w-80 ${
          selected ? `hidden` : `flex`
        }`}
      >
        <div className="flex items-center gap-1 border-b px-3 py-2.5">
          <h1 className="mr-auto text-sm font-medium">Support</h1>
          {([`open`, `resolved`] as const).map((tab) => (
            <Button
              key={tab}
              variant="ghost"
              size="sm"
              onClick={() => setFilter(tab)}
              className={`h-7 rounded-full px-3 text-xs capitalize ${
                filter === tab
                  ? `bg-accent font-medium text-foreground`
                  : `text-muted-foreground hover:text-foreground`
              }`}
            >
              {tab}
            </Button>
          ))}
        </div>
        <div className="flex-1 overflow-y-auto">
          {threads === null ? (
            <div className="flex items-center justify-center py-10">
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            </div>
          ) : threads.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-10 text-center">
              <LifeBuoy className="h-6 w-6 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">
                {filter === `open`
                  ? `No open conversations.`
                  : `No resolved conversations yet.`}
              </p>
            </div>
          ) : (
            threads.map((thread) => (
              <button
                key={thread.id}
                type="button"
                onClick={() => setSelectedId(thread.id)}
                className={`block w-full border-b px-3 py-2.5 text-left transition-colors hover:bg-accent/50 ${
                  thread.id === selectedId ? `bg-accent/60` : ``
                }`}
              >
                <div className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium">
                    {reporterLabel(thread)}
                  </span>
                  <span className="shrink-0 text-[0.65rem] text-muted-foreground">
                    {relativeTime(thread.updatedAt)}
                  </span>
                  {thread.unread && (
                    <span
                      className="h-2 w-2 shrink-0 rounded-full bg-indigo-500"
                      aria-label="Awaiting reply"
                    />
                  )}
                </div>
                <p className="mt-0.5 truncate text-xs text-muted-foreground">
                  {thread.lastMessage?.body ?? thread.title}
                </p>
              </button>
            ))
          )}
        </div>
      </div>

      {/* Middle + right */}
      {selected ? (
        <ConversationPane
          key={selected.id}
          thread={selected}
          teamId={teamId}
          teamSlug={teamSlug}
          onBack={() => setSelectedId(null)}
          onChanged={loadThreads}
        />
      ) : (
        <div className="hidden flex-1 items-center justify-center md:flex">
          <div className="flex flex-col items-center gap-2 text-center">
            <LifeBuoy className="h-8 w-8 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">
              Select a conversation
            </p>
          </div>
        </div>
      )}
    </div>
  )
}

function ConversationPane({
  thread,
  teamId,
  teamSlug,
  onBack,
  onChanged,
}: {
  thread: ThreadRow
  teamId: string
  teamSlug: string
  onBack: () => void
  onChanged: () => Promise<void>
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
      setDetail(await trpc.helpdesk.getThread.query({ threadId: thread.id }))
    } catch (err) {
      console.error(`helpdesk thread load failed`, err)
    }
  }, [thread.id])

  useEffect(() => {
    void loadDetail()
    const timer = setInterval(() => void loadDetail(), THREAD_POLL_MS)
    return () => clearInterval(timer)
  }, [loadDetail])

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
        await trpc.helpdesk.reply.mutate({ threadId: thread.id, body })
      } else {
        await trpc.helpdesk.note.mutate({ threadId: thread.id, body })
      }
      setDraft(``)
      await Promise.all([loadDetail(), onChanged()])
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
        await trpc.helpdesk.reopen.mutate({ threadId: thread.id })
      } else {
        await trpc.helpdesk.close.mutate({ threadId: thread.id })
      }
      await Promise.all([loadDetail(), onChanged()])
    } catch (err) {
      console.error(`helpdesk close/reopen failed`, err)
    } finally {
      setStatusBusy(false)
    }
  }

  return (
    <>
      {/* Middle — chat thread */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b px-3 py-2">
          <Button
            variant="ghost"
            size="icon-xs"
            className="shrink-0 text-muted-foreground md:hidden"
            onClick={onBack}
            aria-label="Back to conversations"
          >
            <ArrowLeft className="size-4" />
          </Button>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">
              {reporterLabel(thread)}
            </p>
            <p className="truncate text-xs text-muted-foreground">
              {thread.title}
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            className="h-7 shrink-0"
            disabled={statusBusy || detail === null}
            onClick={() => void toggleClosed()}
          >
            {statusBusy ? (
              <Loader2 className="size-3 animate-spin" />
            ) : isResolved ? (
              <RotateCcw className="size-3" />
            ) : (
              <Check className="size-3" />
            )}
            {isResolved ? `Reopen ticket` : `Close ticket`}
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            className="shrink-0 text-muted-foreground lg:hidden"
            onClick={() => setDetailsOpen(true)}
            aria-label="Ticket details"
          >
            <Info className="size-4" />
          </Button>
        </div>

        <div className="flex flex-1 flex-col gap-3 overflow-y-auto px-4 py-4">
          {detail === null ? (
            <div className="flex flex-1 items-center justify-center">
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            </div>
          ) : (
            detail.messages.map((message) => {
              const isInbound = message.direction === `inbound`
              const isInternal = message.visibility === `internal`
              const author = isInbound
                ? reporterLabel(thread)
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
                      ? `self-start rounded-bl-sm bg-muted`
                      : isInternal
                        ? `self-end rounded-br-sm border border-amber-500/40 bg-amber-500/10`
                        : `self-end rounded-br-sm bg-indigo-600 text-white`
                  }`}
                >
                  {isInternal && (
                    <Badge
                      variant="outline"
                      className="mb-1 gap-1 border-amber-500/50 text-[0.6rem] text-amber-500"
                    >
                      <StickyNote className="h-2.5 w-2.5" />
                      Internal
                    </Badge>
                  )}
                  <p className="whitespace-pre-wrap break-words">
                    {message.body}
                  </p>
                  <p
                    className={`mt-1 text-[0.65rem] ${
                      isInbound || isInternal
                        ? `text-muted-foreground`
                        : `text-white/70`
                    }`}
                  >
                    {author} · {relativeTime(message.createdAt)}
                  </p>
                </div>
              )
            })
          )}
          <div ref={bottomRef} />
        </div>

        <div className="border-t px-3 py-2.5">
          <div className="mb-1.5 flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setMode(`reply`)}
              className={`h-6 rounded-full px-2.5 text-xs ${
                mode === `reply`
                  ? `bg-accent font-medium text-foreground`
                  : `text-muted-foreground`
              }`}
            >
              <Mail className="size-3" />
              Reply
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setMode(`note`)}
              className={`h-6 rounded-full px-2.5 text-xs ${
                mode === `note`
                  ? `bg-amber-500/15 font-medium text-amber-500`
                  : `text-muted-foreground`
              }`}
            >
              <StickyNote className="size-3" />
              Internal note
            </Button>
          </div>
          <div className="flex items-end gap-2">
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
                  ? `Reply to ${reporterLabel(thread)}… (emailed to them)`
                  : `Add an internal note… (never sent to the reporter)`
              }
              rows={2}
              className={`min-h-9 flex-1 resize-none ${
                mode === `note` ? `border-amber-500/40` : ``
              }`}
            />
            <Button
              size="icon"
              className="h-9 w-9 shrink-0"
              disabled={sending || draft.trim().length === 0}
              onClick={() => void send()}
              aria-label={mode === `reply` ? `Send reply` : `Save note`}
            >
              {sending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Send className="h-4 w-4" />
              )}
            </Button>
          </div>
        </div>
      </div>

      {/* Right — details rail (≥lg) */}
      <div className="hidden w-72 shrink-0 flex-col gap-4 overflow-y-auto border-l px-4 py-4 lg:flex">
        <ThreadDetails
          thread={thread}
          teamId={teamId}
          teamSlug={teamSlug}
          onEscalated={async () => {
            await Promise.all([loadDetail(), onChanged()])
          }}
        />
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
          <ThreadDetails
            thread={thread}
            teamId={teamId}
            teamSlug={teamSlug}
            onEscalated={async () => {
              await Promise.all([loadDetail(), onChanged()])
            }}
          />
        </SheetContent>
      </Sheet>
    </>
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
  thread: ThreadRow
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

  // Escalation board picker: the team's live (non-archived) boards.
  const { data: allBoards } = useLiveQuery(
    (query) =>
      query
        .from({ boards: boardCollection })
        .where(({ boards }) => eq(boards.teamId, teamId)),
    [teamId]
  )
  const boards = (allBoards ?? []).filter(
    (row) => !row.archivedAt && !row.deletedAt
  )
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
        <h2 className="mb-1.5 text-xs font-medium uppercase text-muted-foreground">
          Reporter
        </h2>
        <p className="text-sm font-medium">{reporterLabel(thread)}</p>
        <p className="truncate text-xs text-muted-foreground">
          {thread.reporterEmail}
        </p>
        {thread.lastReporterSeenAt && (
          <p className="mt-1 text-xs text-muted-foreground">
            Last seen {relativeTime(thread.lastReporterSeenAt)}
          </p>
        )}
      </section>

      {submission && (
        <section>
          <h2 className="mb-1.5 text-xs font-medium uppercase text-muted-foreground">
            Context
          </h2>
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
            <h2 className="mb-1.5 text-xs font-medium uppercase text-muted-foreground">
              Linked issue
            </h2>
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
          <h2 className="mb-1.5 text-xs font-medium uppercase text-muted-foreground">
            Escalate
          </h2>
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
                <Loader2 className="size-3 animate-spin" />
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
