import { useMemo, useState, type ReactNode } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  Button,
  conceptIcon,
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  GlassSectionHeader,
  Pill,
  TreeGuides,
  treeGuides,
  useIsMobile,
  type TreeGuide,
} from "@exp/ui"
import type { Board, CodingSession, Issue } from "@/db/schema"
import {
  boardCollection,
  codingSessionCollection,
  issueCollection,
  issueRelationCollection,
} from "@/lib/collections"
import {
  badgeChip,
  badgeShape,
  type BadgeShape,
  batchBandTitle,
  overlaySections,
  PR_GRAPH_OVERLAY_COPY,
  prGraph,
  RELATED_WORK_TITLE,
  type OverlaySection,
  type PrGraphFace,
} from "@/lib/pr-graph"
import {
  RELATIONS_VIEW_COPY,
  relationRowIsOpen,
} from "@/lib/issue-relations-view"
import { blockGraph, type GraphRelation } from "@/lib/issue-graph"
import { IssueGraphView } from "@/components/issue-graph"
import { useTeamBoardIds } from "@/hooks/use-team-issue-graph"
import { useTeamUsers } from "@/hooks/use-team-data"
import { sessionIdentity } from "@/lib/session-identity"
import { runHasEnded } from "@/lib/past-runs"
import { useSessionListRows } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { IssueChip } from "@/components/issue-chip"
import { PrStateBadge } from "@/components/issue-coding-rows"
import { pastRunRowByline } from "@/components/agent-session-row"
import {
  PastSessionRow,
  RunningSessionRow,
} from "@/components/session-list-rows"
import {
  RelationIssueRow,
  RowList,
  type RelationIssueRowLinkProps,
} from "@/components/issue-relations-card"
import { PrStackRow } from "@/components/pr-stack-row"
import { cn } from "@/lib/utils"

// EXP-897 Part 4: the ONE stack/batch badge. A piece of work can be related to
// other work three ways — a PR STACK (`pr_base_branch`), a BATCH (issues
// sharing one `pr_url`), a session TREE (`parent_session_id`) — and until now
// each of those was visible on a different screen, if at all.
//
// SLOP-16: in the work header (the Issue, Run and Changes faces share
// `WorkHeader`, EXP-877) it is a quiet ICON BUTTON — the glyph names the shape
// (`badgeShape`: stack, batch, run family, open blockers; face-independent,
// EXP-1097), a muted `+N` counts the rest (`badgeChip`); in the Reviews queue,
// the glyph on a batch row.
//
// SLOP-16 r3: click opens THE "Related work" view — the standard `Dialog`
// (its bottom-sheet arm on phones), one GROUP BAND per section in
// `overlaySections` order over rows the product already draws elsewhere: the
// relations card's issue row, the Reviews queue's stack row, the session
// tree's run row, and the compact mini-graph under "Blocked by". No new
// layout, a batch's issues listed once. Same bands, same rows, same copy
// (`PR_GRAPH_OVERLAY_COPY`) on all four clients (`pr_graph.rs`,
// `PrGraphBadge.swift`, `PrGraphBadge.kt`).
//
// Everything it draws is already synced — `lib/pr-graph.ts` is the pure model.

const NO_RELATIONS: readonly GraphRelation[] = []

const StackIcon = conceptIcon(`pr-stack`)
const BatchIcon = conceptIcon(`pr-batch`)
// EXP-1079: the Run face of a run with a family but no PR relation wears the
// session tree's own concept (the desktop's `BadgeGlyph::Runs`).
const TreeIcon = conceptIcon(`session-tree`)
const BlockedIcon = conceptIcon(`relation-blocked-by`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)

/** SLOP-16: the header button's glyph per badge shape. */
const BADGE_GLYPH: Record<NonNullable<BadgeShape>, typeof StackIcon> = {
  stack: StackIcon,
  [`stack+batch`]: StackIcon,
  batch: BatchIcon,
  runs: TreeIcon,
  blocked: BlockedIcon,
}

export type { PrGraphFace } from "@/lib/pr-graph"

/** Byte-identical with the desktop tooltip (`pr_graph::badge_tooltip`). */
export const RUNS_BADGE_NAME = `The runs around this one`
/** EXP-1097: the `blocked` shape's name — the relations band's own title. */
export const BLOCKED_BADGE_NAME = RELATIONS_VIEW_COPY.blockedBy

export function PrGraphBadge({
  teamId,
  teamSlug,
  face,
  issue = null,
  session = null,
  variant = `chip`,
  fallback = null,
  onMergeStack,
  className,
}: {
  teamId: string
  teamSlug: string
  face: PrGraphFace
  issue?: Issue | null
  session?: CodingSession | null
  /** `chip` = the work header's icon button (glyph + `+N`); `glyph` = a list
   *  row's lead icon (the Reviews queue's batch rows). */
  variant?: `chip` | `glyph`
  /** EXP-916: what to draw when the graph has NO badge (a batch row whose
   *  siblings have not synced yet). A `glyph` badge sits in a fixed lead cell
   *  of a grid row, and returning nothing shifted the whole row one column. */
  fallback?: ReactNode
  /** The Changes face's bottom entry offers it; absent = no control. */
  onMergeStack?: (topIssueId: string) => void
  className?: string
}) {
  const [open, setOpen] = useState(false)

  // EXP-980: an issue row carries no `team_id` on the client (the issues shape
  // drops its scoping columns), so `eq(i.teamId, …)` matched NOTHING and the
  // overlay never resolved a batch partner or a blocker. The team's issues =
  // the issues of the team's boards.
  const boardIds = useTeamBoardIds(teamId)
  const { data: issueRows } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.boardId, boardIds))
        : undefined,
    [boardIds.join(`,`)]
  )
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) => eq(s.teamId, teamId)),
    [teamId]
  )
  // EXP-980: the team's `blocks` rows — the blocked-by section is the
  // transitive mini-graph now, so the direct rows alone no longer do.
  // EXP-1097: fetched for a run subject too — its issue's open blockers earn
  // the chip on every face now.
  const subjectIssueId = issue?.id ?? session?.issueId ?? null
  const { data: relationRows } = useLiveQuery(
    (query) =>
      subjectIssueId
        ? query
            .from({ r: issueRelationCollection })
            .where(({ r }) => and(eq(r.teamId, teamId), eq(r.type, `blocks`)))
        : undefined,
    [subjectIssueId, teamId]
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
  const relations = useMemo(
    () => (relationRows ?? []) as GraphRelation[],
    [relationRows]
  )
  const graph = useMemo(
    () => prGraph({ issue, session, issues, sessions, relations }),
    [issue, session, issues, sessions, relations]
  )

  const kind = badgeShape(graph)
  const chipSpec = badgeChip(graph)
  if (!kind || !chipSpec) return fallback
  const name =
    kind === `runs`
      ? RUNS_BADGE_NAME
      : kind === `blocked`
        ? BLOCKED_BADGE_NAME
        : kind === `stack+batch`
        ? `Stack and batch`
        : kind === `stack`
          ? `Pull request stack`
          : `Batch pull request`
  // SLOP-16: the header trigger is a quiet ICON BUTTON, not a chip — a chip
  // restated the title right beside it. The glyph names the SHAPE, `+N` the
  // rest of it; the name rides the tooltip.
  const Glyph = variant === `glyph` ? BatchIcon : BADGE_GLYPH[kind]
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
      >
        <Glyph className="size-4" />
      </button>
    ) : (
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={name}
        title={name}
        data-testid="pr-graph-badge"
        data-shape={kind}
        className={cn(
          `shrink-0 text-muted-foreground hover:text-foreground`,
          chipSpec.count > 0 && `w-auto gap-1 px-2`,
          className
        )}
        onClick={(event) => event.stopPropagation()}
      >
        <Glyph className="size-4" data-testid="pr-graph-badge-glyph" />
        {chipSpec.count > 0 && (
          <span
            className="font-mono text-xs text-muted-foreground"
            data-testid="pr-graph-badge-count"
          >
            +{chipSpec.count}
          </span>
        )}
      </Button>
    )

  // SLOP-16 r3: ONE surface at every size — the standard dialog, which drops
  // to its bottom-sheet arm (16px gutter) on a phone by itself.
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent
        data-testid="pr-graph-overlay"
        aria-describedby={undefined}
        // A portal still bubbles React events to the row that hosts the
        // trigger (the Reviews queue): keep clicks inside the dialog there.
        onClick={(event) => event.stopPropagation()}
      >
        <DialogHeader>
          <DialogTitle>{RELATED_WORK_TITLE}</DialogTitle>
        </DialogHeader>
        <DialogBody>
          <PrGraphOverlay
            face={face}
            graph={graph}
            issues={issues}
            relations={relations}
            boardSlugById={boardSlugById}
            subjectIssue={issue}
            teamId={teamId}
            teamSlug={teamSlug}
            onMergeStack={onMergeStack}
            onClose={() => setOpen(false)}
          />
        </DialogBody>
      </DialogContent>
    </Dialog>
  )
}

/** One section: the GROUP BAND over its flat rows — the relations card's and
 *  the Reviews queue's own composition. Never folds. */
function Band({
  section,
  label,
  children,
}: {
  section: OverlaySection
  label: string
  children: ReactNode
}) {
  return (
    <div className="flex flex-col" data-testid={`pr-graph-band-${section}`}>
      <GlassSectionHeader label={label} />
      {children}
    </div>
  )
}

/** THE "Related work" body — group bands over the existing rows, in the
 *  face's `overlaySections` order. Exported for the component test. */
export function PrGraphOverlay({
  face,
  graph,
  issues,
  relations = NO_RELATIONS,
  boardSlugById,
  subjectIssue,
  teamId,
  teamSlug,
  onMergeStack,
  onClose,
}: {
  face: PrGraphFace
  graph: ReturnType<typeof prGraph<Issue, CodingSession>>
  /** The team's synced issues — a tree row's own issue, for its identity. */
  issues: readonly Issue[]
  /** EXP-980: the team's synced `blocks` rows, for the Blocked-by graph. */
  relations?: readonly GraphRelation[]
  /** EXP-930: board slug per board id — what turns an issue into a link.
   *  Absent (or missing the issue's board) = an inert row. */
  boardSlugById?: ReadonlyMap<string, string>
  subjectIssue: Issue | null
  /** The team — the run rows' devices and the issue rows' assignees. */
  teamId?: string
  teamSlug: string
  onMergeStack?: (topIssueId: string) => void
  onClose: () => void
}) {
  const phone = useIsMobile()
  const navigate = useNavigate()
  const openSession = useOpenSession()
  const { userMap } = useTeamUsers(teamId)
  const treeSessions = useMemo(
    () => graph.tree.map((row) => row.session),
    [graph.tree]
  )
  const runRows = useSessionListRows(teamId, treeSessions)

  // EXP-930: EVERY issue the view lists opens — a real `<Link>`, so ⌘-click
  // and middle-click work like anywhere else; the dialog closes behind it.
  const issueLink = (row: Issue) => {
    const boardSlug = boardSlugById?.get(row.boardId)
    if (!boardSlug) return undefined
    return ({ className, children }: RelationIssueRowLinkProps) => (
      <Link
        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
        params={{ teamSlug, boardSlug, issueIdentifier: row.identifier }}
        onClick={onClose}
        className={className}
      >
        {children}
      </Link>
    )
  }

  /** The relations card's row, verbatim. */
  const issueRow = (
    row: Issue,
    nest?: { depth: number; guide: TreeGuide | null }
  ) => {
    const assignee = row.assigneeId ? userMap.get(row.assigneeId) : undefined
    return (
      <RelationIssueRow
        key={row.id}
        issue={row}
        open={relationRowIsOpen(row.status)}
        assignee={assignee}
        phone={phone}
        link={issueLink(row)}
        depth={nest?.depth}
        leading={nest ? <TreeGuides guide={nest.guide} /> : undefined}
      />
    )
  }

  // EXP-1097: every relation the subject HAS gets its band on every face;
  // the face only decides which one LEADS (`overlaySections`).
  const sections = overlaySections(graph, face)

  // EXP-980: the transitive chain as THE mini-graph — the COMPACT one (the
  // small chip: glyph · identifier), scrolling sideways past the surface.
  const blockedBand = subjectIssue ? (
    <Band key="blocked" section="blocked" label={PR_GRAPH_OVERLAY_COPY.blocked}>
      <IssueGraphView
        graph={blockGraph([subjectIssue.id], relations, issues)}
        issueById={new Map(issues.map((row) => [row.id, row]))}
        density="compact"
        renderNode={(row) => {
          const boardSlug = boardSlugById?.get(row.boardId)
          return (
            <IssueChip
              key={row.id}
              issue={row}
              size="sm"
              className="w-full"
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
        }}
      />
    </Band>
  ) : null

  // The Issue and Changes faces read the batch from the subject issue, so the
  // band lists its PARTNERS; on a run the covered set IS the run's subject.
  const inBatch = (graph.batch?.issues ?? []).filter(
    (row) => face === `run` || row.id !== subjectIssue?.id
  )
  const batchBand =
    inBatch.length > 0 ? (
      <Band key="batch" section="batch" label={batchBandTitle(face)}>
        <div data-testid="pr-graph-batch-partners">
          <RowList phone={phone}>{inBatch.map((row) => issueRow(row))}</RowList>
        </div>
      </Band>
    ) : null

  // The session tree's own rows: a live run = the running row, an ended one
  // the past row, nested with the EXP-965 connector.
  const runGuides = treeGuides(graph.tree.map((row) => row.depth))
  const runsBand = (
    <Band key="runs" section="runs" label={PR_GRAPH_OVERLAY_COPY.runs}>
      <div className="flex flex-col">
        {graph.tree.map(({ session, depth }, index) => {
          const row = runRows[index]
          if (!row) return null
          const open = () => {
            onClose()
            openSession(session)
          }
          const identity = sessionIdentity(row)
          return runHasEnded(session) ? (
            <PastSessionRow
              key={session.id}
              sessionId={session.id}
              title={identity.subject}
              identifier={identity.identifier}
              byline={pastRunRowByline(row)}
              depth={depth}
              guide={runGuides[index]}
              onOpen={open}
            />
          ) : (
            <RunningSessionRow
              key={session.id}
              row={row}
              depth={depth}
              guide={runGuides[index]}
              onOpen={open}
            />
          )
        })}
      </div>
    </Band>
  )

  // The pull requests: the Reviews queue's stack row, BOTTOM-UP. A batch
  // entry folds its issues underneath (depth + 1) — unless the batch band
  // already lists them in this view. `Merge stack` = the bottom row's
  // trailing control, as on Reviews.
  const top = graph.stack[graph.stack.length - 1]
  const prRows =
    graph.stack.length > 0
      ? graph.stack
      : graph.entry
        ? [{ entry: graph.entry, depth: 0 }]
        : []
  const foldIssues = !sections.includes(`batch`)
  const stackLines = prRows.flatMap(({ entry, depth }, index) => [
    { kind: `pr` as const, entry, depth, index },
    ...(foldIssues && entry.issues.length > 1
      ? entry.issues.map((issue) => ({
          kind: `issue` as const,
          issue,
          depth: depth + 1,
        }))
      : []),
  ])
  const stackGuides = treeGuides(stackLines.map((line) => line.depth))
  const canMergeStack = Boolean(onMergeStack && top && graph.stack.length > 1)
  const stackBand = (
    <Band key="stack" section="stack" label={PR_GRAPH_OVERLAY_COPY.stack}>
      <div className="flex flex-col">
        {stackLines.map((line, lineIndex) => {
          if (line.kind === `issue`) {
            return issueRow(line.issue, {
              depth: line.depth,
              guide: stackGuides[lineIndex],
            })
          }
          const { entry, depth, index } = line
          const isBatch = entry.issues.length > 1
          const bottomOfStack = canMergeStack && index === 0
          return (
            <PrStackRow
              key={entry.key}
              issue={entry.issue}
              issues={entry.issues}
              depth={depth}
              guide={stackGuides[lineIndex]}
              stackedOn={
                depth > 0 ? (prRows[index - 1]?.entry.issue.identifier ?? null) : null
              }
              // Its issues are listed in this view already (band or fold).
              listBatchIdentifiers={false}
              onOpen={() => {
                onClose()
                void navigate({
                  to: `/t/$teamSlug/reviews/$issueIdentifier`,
                  params: { teamSlug, issueIdentifier: entry.issue.identifier },
                })
              }}
              lead={
                isBatch ? (
                  <BatchIcon className="size-4 text-muted-foreground" />
                ) : entry.issue.prState === `merged` ? (
                  <PrMergedIcon className="size-4 text-purple-400" />
                ) : (
                  <PrOpenIcon
                    className={cn(
                      `size-4`,
                      entry.issue.prState === `open`
                        ? `text-emerald-500`
                        : `text-muted-foreground`
                    )}
                  />
                )
              }
              trailing={
                bottomOfStack && top && onMergeStack ? (
                  <Pill
                    size="md"
                    mode="action"
                    data-testid="pr-graph-merge-stack"
                    onClick={(event) => {
                      event.stopPropagation()
                      onClose()
                      onMergeStack(top.entry.issue.id)
                    }}
                  >
                    <StackIcon className="h-3.5 w-3.5" />
                    {PR_GRAPH_OVERLAY_COPY.mergeStack}
                  </Pill>
                ) : (
                  <span className="self-center">
                    <PrStateBadge state={entry.issue.prState} />
                  </span>
                )
              }
            />
          )
        })}
      </div>
    </Band>
  )

  const byKind: Record<OverlaySection, ReactNode> = {
    blocked: blockedBand,
    batch: batchBand,
    runs: runsBand,
    stack: stackBand,
  }
  const drawn = sections.map((section) => byKind[section]).filter(Boolean)

  return (
    <div className="flex flex-col gap-4">
      {drawn}
      {drawn.length === 0 && (
        <div className="text-sm text-muted-foreground">
          {PR_GRAPH_OVERLAY_COPY.empty}
        </div>
      )}
    </div>
  )
}
