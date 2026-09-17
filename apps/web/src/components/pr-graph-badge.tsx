import { useMemo, useState, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import {
  conceptIcon,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  Pill,
  useIsMobile,
} from "@exp/ui"
import type { Board, CodingSession, Issue } from "@/db/schema"
import {
  boardCollection,
  codingSessionCollection,
  issueCollection,
  issueRelationCollection,
} from "@/lib/collections"
import { badgeKind, badgeLabel, prGraph } from "@/lib/pr-graph"
import { MERGE_STACK_LABEL } from "@/lib/pr-stack"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { sessionIdentity } from "@/lib/session-identity"
import { rowPrState } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { IssueChip } from "@/components/issue-chip"
import { PrStateBadge } from "@/components/issue-coding-rows"
import { RunningIndicator } from "@/components/agent-session-row"
import { cn } from "@/lib/utils"

// EXP-897 Part 4: the ONE stack/batch badge. A piece of work can be related to
// other work three ways — a PR STACK (`pr_base_branch`), a BATCH (issues
// sharing one `pr_url`), a session TREE (`parent_session_id`) — and until now
// each of those was visible on a different screen, if at all.
//
// The badge is a small pill in the work header (the Issue, Run and Changes
// faces share `WorkHeader`, EXP-877) plus, in the Reviews queue, the glyph on
// a batch row. Hover on ≥md, tap everywhere: the SAME overlay opens, with the
// section that belongs to the face showing. Same rows, same copy, on all four
// clients (`pr_graph.rs`, `PrGraphBadge.swift`, `PrGraphBadge.kt`).
//
// Everything it draws is already synced — `lib/pr-graph.ts` is the pure model.

const StackIcon = conceptIcon(`pr-stack`)
const BatchIcon = conceptIcon(`pr-batch`)

/** Which face the overlay's sections belong to. */
export type PrGraphFace = `issue` | `run` | `changes`

export function PrGraphBadge({
  teamId,
  teamSlug,
  face,
  issue = null,
  session = null,
  variant = `pill`,
  fallback = null,
  onMergeStack,
  className,
}: {
  teamId: string
  teamSlug: string
  face: PrGraphFace
  issue?: Issue | null
  session?: CodingSession | null
  /** `pill` = the work header's glyph + `2 of 3`; `glyph` = a list row's
   *  lead icon (the Reviews queue's batch rows). */
  variant?: `pill` | `glyph`
  /** EXP-916: what to draw when the graph has NO badge (a batch row whose
   *  siblings have not synced yet). A `glyph` badge sits in a fixed lead cell
   *  of a grid row, and returning nothing shifted the whole row one column. */
  fallback?: ReactNode
  /** The Changes face's bottom entry offers it; absent = no control. */
  onMergeStack?: (topIssueId: string) => void
  className?: string
}) {
  const isMobile = useIsMobile()
  const [open, setOpen] = useState(false)

  const { data: issueRows } = useLiveQuery(
    (query) =>
      query.from({ i: issueCollection }).where(({ i }) => eq(i.teamId, teamId)),
    [teamId]
  )
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) => eq(s.teamId, teamId)),
    [teamId]
  )
  // The blocked side only — `issue_id` blocks `related_issue_id` (EXP-736).
  const subjectId = issue?.id ?? ``
  const { data: relationRows } = useLiveQuery(
    (query) =>
      subjectId
        ? query
            .from({ r: issueRelationCollection })
            .where(({ r }) =>
              and(eq(r.relatedIssueId, subjectId), eq(r.type, `blocks`))
            )
        : undefined,
    [subjectId]
  )

  // EXP-930: a batch's issue rows are only useful if they OPEN — and an issue
  // URL is board-scoped, so the badge resolves the team's board slugs once.
  const { data: boardRows } = useLiveQuery(
    (query) =>
      query.from({ b: boardCollection }).where(({ b }) => eq(b.teamId, teamId)),
    [teamId]
  )
  const boardSlugById = useMemo(
    () =>
      new Map(((boardRows ?? []) as Board[]).map((row) => [row.id, row.slug])),
    [boardRows]
  )

  const issues = useMemo(() => (issueRows ?? []) as Issue[], [issueRows])
  const sessions = useMemo(
    () => (sessionRows ?? []) as CodingSession[],
    [sessionRows]
  )
  const graph = useMemo(
    () =>
      prGraph({
        issue,
        session,
        issues,
        sessions,
        relations: (relationRows ?? []) as {
          type: string
          issueId: string
          relatedIssueId: string
        }[],
      }),
    [issue, session, issues, sessions, relationRows]
  )

  const kind = badgeKind(graph)
  if (!kind) return fallback
  const label = badgeLabel(graph)
  const name =
    kind === `stack+batch`
      ? `Stack and batch`
      : kind === `stack`
        ? `Pull request stack`
        : `Batch pull request`

  const trigger =
    variant === `glyph` ? (
      <button
        type="button"
        aria-label={name}
        title={name}
        data-testid="pr-graph-badge"
        className={cn(
          `flex size-4 items-center justify-center text-muted-foreground hover:text-foreground`,
          className
        )}
        // The Radix trigger owns the toggle (it wraps this node); this
        // handler only keeps the click off the list row underneath.
        onClick={(event) => event.stopPropagation()}
        onMouseEnter={() => {
          if (!isMobile) setOpen(true)
        }}
      >
        <BatchIcon className="size-4" />
      </button>
    ) : (
      <Pill
        mode="action"
        aria-label={name}
        title={name}
        data-testid="pr-graph-badge"
        className={className}
        onClick={(event) => event.stopPropagation()}
        onMouseEnter={() => {
          if (!isMobile) setOpen(true)
        }}
      >
        {(kind === `stack` || kind === `stack+batch`) && (
          <StackIcon className="size-3" />
        )}
        {(kind === `batch` || kind === `stack+batch`) && (
          <BatchIcon className="size-3" />
        )}
        {label}
      </Pill>
    )

  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild>{trigger}</MobilePopoverTrigger>
      <MobilePopoverContent
        align="end"
        mobileTitle={name}
        className="w-80 p-3"
        data-testid="pr-graph-overlay"
        onMouseLeave={() => {
          if (!isMobile) setOpen(false)
        }}
      >
        <PrGraphOverlay
          face={face}
          graph={graph}
          issues={issues}
          boardSlugById={boardSlugById}
          subjectIssue={issue}
          teamSlug={teamSlug}
          onMergeStack={onMergeStack}
          onClose={() => setOpen(false)}
        />
      </MobilePopoverContent>
    </MobilePopover>
  )
}

function Section({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </div>
      {children}
    </div>
  )
}

/** The overlay body — the same row primitives on every face, so the three
 *  read as one thing. Exported for the component test. */
export function PrGraphOverlay({
  face,
  graph,
  issues,
  boardSlugById,
  subjectIssue,
  teamSlug,
  onMergeStack,
  onClose,
}: {
  face: PrGraphFace
  graph: ReturnType<typeof prGraph<Issue, CodingSession>>
  /** The team's synced issues — a tree row's own issue, for its identity. */
  issues: readonly Issue[]
  /** EXP-930: board slug per board id — what turns a chip into a real link.
   *  Absent (or missing the issue's board) = an inert chip, as before. */
  boardSlugById?: ReadonlyMap<string, string>
  subjectIssue: Issue | null
  teamSlug: string
  onMergeStack?: (topIssueId: string) => void
  onClose: () => void
}) {
  const openSession = useOpenSession()

  // EXP-930: EVERY issue the overlay lists opens — a batch's "3 issues" that
  // only prints three names is the bug this fixes. A real `<Link>`, so
  // ⌘-click and middle-click work like anywhere else.
  const chip = (row: Issue) => {
    const boardSlug = boardSlugById?.get(row.boardId)
    return (
      <IssueChip
        key={row.id}
        issue={row}
        preview={false}
        link={
          boardSlug
            ? (props) => (
                <Link
                  to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                  params={{
                    teamSlug,
                    boardSlug,
                    issueIdentifier: row.identifier,
                  }}
                  onClick={onClose}
                  {...props}
                />
              )
            : undefined
        }
      />
    )
  }

  if (face === `issue`) {
    const inBatch = (graph.batch?.issues ?? []).filter(
      (row) => row.id !== subjectIssue?.id
    )
    return (
      <div className="flex flex-col gap-3">
        {graph.blockedBy.length > 0 && (
          <Section label="Blocked by">
            <div className="flex flex-wrap gap-1.5">
              {graph.blockedBy.map(chip)}
            </div>
          </Section>
        )}
        {inBatch.length > 0 && (
          <Section label="In batch with">
            <div className="flex flex-wrap gap-1.5">{inBatch.map(chip)}</div>
          </Section>
        )}
        {graph.blockedBy.length === 0 && inBatch.length === 0 && (
          <div className="text-xs text-muted-foreground">
            Nothing else is linked to this issue.
          </div>
        )}
      </div>
    )
  }

  if (face === `run`) {
    return (
      <div className="flex flex-col gap-3">
        {/* EXP-930: the pill on a BATCH run says `3 issues`, so the first
            thing behind it is those three issues — the run tree alone
            answered a question nobody asked. The Issue face's batch section
            has the same rows; this is the run's own subject, so the whole
            covered set is listed, not "everything but me". */}
        {graph.batch && (
          <Section label="Issues">
            <div className="flex flex-wrap gap-1.5">
              {graph.batch.issues.map(chip)}
            </div>
          </Section>
        )}
        <Section label="Runs">
        <div className="flex flex-col">
          {graph.tree.map(({ session, depth }) => {
            const issue = session.issueId
              ? issues.find((row) => row.id === session.issueId)
              : undefined
            // EXP-876: `issues` carries the overlay's whole synced set, so a
            // batch run in the tree names itself instead of reading "Batch run".
            const identity = sessionIdentity({ session, issue, batchIssues: issues })
            return (
              <button
                key={session.id}
                type="button"
                className="flex min-w-0 items-center gap-1.5 rounded-md py-1 text-left text-xs hover:bg-glass-active"
                style={{ paddingLeft: `${12 + depth * 14}px` }}
                onClick={() => {
                  onClose()
                  openSession(session)
                }}
              >
                <RunningIndicator
                  state={sessionDisplayState(
                    session,
                    rowPrState(session, issue)
                  )}
                />
                {identity.identifier && (
                  <span className="shrink-0 font-mono text-muted-foreground">
                    {identity.identifier}
                  </span>
                )}
                <span className="min-w-0 truncate">{identity.subject}</span>
              </button>
            )
          })}
        </div>
        </Section>
      </div>
    )
  }

  // The Changes / review face: the stack BOTTOM-UP, a batch entry folding its
  // issues underneath, and `Merge stack` on the bottom entry.
  const bottom = graph.stack[0]
  const top = graph.stack[graph.stack.length - 1]
  return (
    <div className="flex flex-col gap-3">
      <Section label="Pull requests">
        <div className="flex flex-col gap-1.5">
          {(graph.stack.length > 0
            ? graph.stack
            : graph.entry
              ? [{ entry: graph.entry, depth: 0 }]
              : []
          ).map(({ entry, depth }) => (
            <div
              key={entry.key}
              className="flex flex-col gap-1"
              style={{ paddingLeft: `${depth * 14}px` }}
            >
              <Link
                to="/t/$teamSlug/reviews/$issueIdentifier"
                params={{
                  teamSlug,
                  issueIdentifier: entry.issue.identifier,
                }}
                onClick={onClose}
                className="flex min-w-0 items-center gap-2 text-xs hover:underline"
              >
                {entry.issues.length > 1 && (
                  <BatchIcon className="size-3 shrink-0 text-muted-foreground" />
                )}
                <span className="shrink-0 font-mono">
                  {entry.issue.prNumber != null
                    ? `#${entry.issue.prNumber}`
                    : entry.issue.identifier}
                </span>
                <PrStateBadge state={entry.issue.prState} />
              </Link>
              {entry.issues.length > 1 && (
                <div className="flex flex-wrap gap-1.5 pl-5">
                  {entry.issues.map(chip)}
                </div>
              )}
            </div>
          ))}
        </div>
      </Section>
      {onMergeStack && bottom && top && graph.stack.length > 1 && (
        <Pill
          mode="action"
          data-testid="pr-graph-merge-stack"
          onClick={() => {
            onClose()
            onMergeStack(top.entry.issue.id)
          }}
        >
          <StackIcon className="size-3" />
          {MERGE_STACK_LABEL}
        </Pill>
      )}
    </div>
  )
}
