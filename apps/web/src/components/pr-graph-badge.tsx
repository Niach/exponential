import { useMemo, useState, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
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
  useIsMobile,
} from "@exp/ui"
import type { Board, CodingSession, Issue } from "@/db/schema"
import {
  boardCollection,
  issueCollection,
  issueRelationCollection,
} from "@/lib/collections"
import {
  badgeChip,
  badgeShape,
  type BadgeShape,
  batchPartners,
  overlaySections,
  PR_GRAPH_OVERLAY_COPY,
  prGraph,
  RELATED_WORK_TITLE,
  stackOthers,
  type OverlaySection,
  type PrGraphEntry,
} from "@/lib/pr-graph"
import {
  RELATIONS_BAND_CAP,
  RELATIONS_VIEW_COPY,
  relationRowIsOpen,
  relationsShowMore,
} from "@/lib/issue-relations-view"
import { useTeamBoardIds } from "@/hooks/use-team-issue-graph"
import { useTeamUsers } from "@/hooks/use-team-data"
import { PrStateBadge } from "@/components/issue-coding-rows"
import {
  RelationBandFrame,
  RelationIssueRow,
  type RelationIssueRowLinkProps,
} from "@/components/issue-relations-card"
import { cn } from "@/lib/utils"

// EXP-897 Part 4: the ONE "Related work" badge. A piece of work can be related
// to other work three ways: open BLOCKERS (`blocks` rows), a BATCH (issues
// sharing one `pr_url`), a PR STACK (`pr_base_branch`).
//
// SLOP-16: in the work header (the Issue, Run and Changes faces share
// `WorkHeader`, EXP-877) it is a quiet ICON BUTTON: the glyph names the shape
// (`badgeShape`: stack, batch, open blockers; face-independent), a muted `+N`
// counts the rest (`badgeChip`); in the Reviews queue, the glyph on a batch
// row.
//
// SLOP-16 r5: click opens THE "Related work" view: the standard `Dialog`
// (its bottom-sheet arm on phones) whose body is EXACTLY the relations card's
// foldable bands (`RelationBandFrame`, 3 rows then "Show N more") over its
// rows: Blocked by · Same pull request · Pull request stack. Nothing else -
// no graph, no runs, no merge control.
// Same bands, same copy (`PR_GRAPH_OVERLAY_COPY`) on all four clients
// (`pr_graph.rs`, `PrGraphBadge.swift`, `PrGraphBadge.kt`).
//
// Everything it draws is already synced: `lib/pr-graph.ts` is the pure model.

const StackIcon = conceptIcon(`pr-stack`)
const BatchIcon = conceptIcon(`pr-batch`)
const BlockedIcon = conceptIcon(`relation-blocked-by`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)

/** SLOP-16: the header button's glyph per badge shape. */
const BADGE_GLYPH: Record<NonNullable<BadgeShape>, typeof StackIcon> = {
  stack: StackIcon,
  [`stack+batch`]: StackIcon,
  batch: BatchIcon,
  blocked: BlockedIcon,
}

/** SLOP-16 r5: each band's side icon, as the relations card's bands. */
const BAND_ICON: Record<OverlaySection, typeof StackIcon> = {
  blocked: BlockedIcon,
  batch: BatchIcon,
  stack: StackIcon,
}

/** EXP-1097: the `blocked` shape's name: the relations band's own title. */
export const BLOCKED_BADGE_NAME = RELATIONS_VIEW_COPY.blockedBy

export function PrGraphBadge({
  teamId,
  teamSlug,
  issue = null,
  session = null,
  variant = `chip`,
  fallback = null,
  className,
}: {
  teamId: string
  teamSlug: string
  issue?: Issue | null
  session?: CodingSession | null
  /** `chip` = the work header's icon button (glyph + `+N`); `glyph` = a list
   *  row's lead icon (the Reviews queue's batch rows). */
  variant?: `chip` | `glyph`
  /** EXP-916: what to draw when the graph has NO badge (a batch row whose
   *  siblings have not synced yet). A `glyph` badge sits in a fixed lead cell
   *  of a grid row, and returning nothing shifted the whole row one column. */
  fallback?: ReactNode
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
  // The team's `blocks` rows: the subject's direct open blockers. EXP-1097:
  // fetched for a run subject too: its issue's open blockers earn the badge.
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

  // EXP-930: a batch's issue rows are only useful if they OPEN: and an issue
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
  const relations = useMemo(
    () =>
      (relationRows ?? []) as {
        type: string
        issueId: string
        relatedIssueId: string
      }[],
    [relationRows]
  )
  const graph = useMemo(
    () => prGraph({ issue, session, issues, relations }),
    [issue, session, issues, relations]
  )

  const kind = badgeShape(graph)
  const chipSpec = badgeChip(graph)
  if (!kind || !chipSpec) return fallback
  const name =
    kind === `blocked`
      ? BLOCKED_BADGE_NAME
      : kind === `stack+batch`
        ? `Stack and batch`
        : kind === `stack`
          ? `Pull request stack`
          : `Batch pull request`
  // SLOP-16: the header trigger is a quiet ICON BUTTON, not a chip: a chip
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

  // SLOP-16 r3: ONE surface at every size: the standard dialog, which drops
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
            graph={graph}
            boardSlugById={boardSlugById}
            teamId={teamId}
            teamSlug={teamSlug}
            onClose={() => setOpen(false)}
          />
        </DialogBody>
      </DialogContent>
    </Dialog>
  )
}

/** THE "Related work" body: the relations card's foldable bands over its
 *  rows, in `overlaySections` order. Exported for the component test. */
export function PrGraphOverlay({
  graph,
  boardSlugById,
  teamId,
  teamSlug,
  onClose,
}: {
  graph: ReturnType<typeof prGraph<Issue, CodingSession>>
  /** EXP-930: board slug per board id: what turns an issue into a link.
   *  Absent (or missing the issue's board) = an inert row. */
  boardSlugById?: ReadonlyMap<string, string>
  /** The team: the issue rows' assignees. */
  teamId?: string
  teamSlug: string
  onClose: () => void
}) {
  const phone = useIsMobile()
  const { userMap } = useTeamUsers(teamId)
  // Every band opens unfolded; the header folds it, as on the relations card.
  const [folded, setFolded] = useState<OverlaySection[]>([])
  const [showAll, setShowAll] = useState<OverlaySection[]>([])
  const flip = (list: OverlaySection[], key: OverlaySection) =>
    list.includes(key) ? list.filter((row) => row !== key) : [...list, key]

  // EXP-930: EVERY issue the view lists opens: a real `<Link>`, so ⌘-click
  // and middle-click work like anywhere else; the dialog closes behind it.
  // EXP-1154: a PR row opens the issue on its Changes face (`?view=diff`),
  // the review of that PR; the Reviews detail page is gone.
  const issueLink = (row: Issue, view?: `diff`) => {
    const boardSlug = boardSlugById?.get(row.boardId)
    if (!boardSlug) return undefined
    return ({ className, children }: RelationIssueRowLinkProps) => (
      <Link
        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
        params={{ teamSlug, boardSlug, issueIdentifier: row.identifier }}
        search={view ? { view } : {}}
        onClick={onClose}
        className={className}
      >
        {children}
      </Link>
    )
  }

  /** The relations card's row, verbatim. */
  const issueRow = (row: Issue) => (
    <RelationIssueRow
      key={row.id}
      issue={row}
      open={relationRowIsOpen(row.status)}
      assignee={row.assigneeId ? userMap.get(row.assigneeId) : undefined}
      phone={phone}
      link={issueLink(row)}
    />
  )

  /** A pull request as the relations card's row: PR glyph · `#n` · the
   *  representative issue's title · its state pill; opens the issue's
   *  Changes face (EXP-1154). */
  const prRow = (entry: PrGraphEntry<Issue>) => {
    const row = entry.issue
    const glyphClass = phone ? `size-4 shrink-0` : `size-3.5 shrink-0`
    const glyph =
      row.prState === `merged` ? (
        <PrMergedIcon className={cn(glyphClass, `text-purple-400`)} />
      ) : (
        <PrOpenIcon
          className={cn(
            glyphClass,
            row.prState === `open` ? `text-emerald-500` : `text-muted-foreground`
          )}
        />
      )
    return (
      <RelationIssueRow
        key={entry.key}
        issue={row}
        open={row.prState !== `merged` && row.prState !== `closed`}
        phone={phone}
        glyph={glyph}
        code={row.prNumber ? `#${row.prNumber}` : row.identifier}
        noAssignee
        link={issueLink(row, `diff`)}
        trailing={
          <span className="flex shrink-0 items-center">
            <PrStateBadge state={row.prState} />
          </span>
        }
      />
    )
  }

  const rowsOf: Record<OverlaySection, ReactNode[]> = {
    blocked: graph.blockedBy.map(issueRow),
    batch: batchPartners(graph).map(issueRow),
    stack: stackOthers(graph).map(prRow),
  }
  const titleOf: Record<OverlaySection, string> = {
    blocked: PR_GRAPH_OVERLAY_COPY.blocked,
    batch: PR_GRAPH_OVERLAY_COPY.batch,
    stack: PR_GRAPH_OVERLAY_COPY.stack,
  }
  const sections = overlaySections(graph)

  return (
    <div className="flex flex-col gap-1.5">
      {sections.map((section) => {
        const all = rowsOf[section]
        const everything = showAll.includes(section)
        const overflow = all.length > RELATIONS_BAND_CAP
        const tail = !overflow
          ? null
          : everything
            ? RELATIONS_VIEW_COPY.showLess
            : relationsShowMore(all.length - RELATIONS_BAND_CAP)
        return (
          <RelationBandFrame
            key={section}
            testId={`pr-graph-band-${section}`}
            icon={BAND_ICON[section]}
            title={titleOf[section]}
            count={all.length}
            expanded={!folded.includes(section)}
            onToggle={() => setFolded((list) => flip(list, section))}
            tail={tail}
            onTail={() => setShowAll((list) => flip(list, section))}
            phone={phone}
          >
            {everything || !overflow ? all : all.slice(0, RELATIONS_BAND_CAP)}
          </RelationBandFrame>
        )
      })}
      {sections.length === 0 && (
        <div className="text-sm text-muted-foreground">
          {PR_GRAPH_OVERLAY_COPY.empty}
        </div>
      )}
    </div>
  )
}
