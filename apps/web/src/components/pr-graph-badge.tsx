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
  ListEmpty,
} from "@exp/ui"
import type { Board, Issue } from "@/db/schema"
import {
  boardCollection,
  issueCollection,
  issueRelationCollection,
  pullRequestCollection,
  pullRequestIssueCollection,
} from "@/lib/collections"
import {
  badgeChip,
  badgeShape,
  type BadgeShape,
  batchPartners,
  issuePrRows,
  overlaySections,
  PR_GRAPH_OVERLAY_COPY,
  prGraph,
  RELATED_WORK_TITLE,
  stackOthers,
  type OverlaySection,
  type PrGraph,
  type PrGraphEntry,
  type PrIssueLinkLike,
  type PrLike,
} from "@/lib/pr-graph"
import {
  RELATIONS_BAND_CAP,
  RELATIONS_VIEW_COPY,
  relationRowIsOpen,
  relationsShowMore,
} from "@/lib/issue-relations-view"
import { useTeamBoardIds } from "@/hooks/use-team-issue-graph"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useAgentRows } from "@/hooks/use-agent-row"
import { PrStateBadge } from "@/components/issue-coding-rows"
import {
  RelationBandFrame,
  RelationIssueRow,
  type RelationIssueRowLinkProps,
} from "@/components/issue-relations-card"
import { cn } from "@/lib/utils"

// EXP-897 Part 4: the ONE "Related work" badge. A piece of work can be related
// to other work three ways: open BLOCKERS (`blocks` rows), a BATCH (issues
// sharing one pull request), a PR STACK (pull requests based on each other).
// EXP-1293: the PR facts come off `pull_requests` + `pull_request_issues`
// (`issuePrRows`), never the legacy issue columns.
//
// SLOP-16: in the issue header it is a quiet ICON BUTTON: the glyph names the
// shape (`badgeShape`: stack, batch, open blockers), a muted `+N` counts the
// rest (`badgeChip`).
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

/** A team issue joined to its pull request (`issuePrRows`). */
export type IssuePrRow = ReturnType<typeof issuePrRows<Issue>>[number]

/**
 * The team's issues, each joined to ITS pull request off the synced
 * `pull_requests` + `pull_request_issues` rows. EXP-980: an issue row
 * carries no `team_id` on the client, so the team's issues = the issues of
 * the team's boards.
 */
export function useTeamIssuePrRows(teamId: string): IssuePrRow[] {
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
  const { data: prRows } = useLiveQuery(
    (query) =>
      query
        .from({ prs: pullRequestCollection })
        .where(({ prs }) => eq(prs.teamId, teamId)),
    [teamId]
  )
  const { data: linkRows } = useLiveQuery(
    (query) =>
      query
        .from({ links: pullRequestIssueCollection })
        .where(({ links }) => eq(links.teamId, teamId)),
    [teamId]
  )
  return useMemo(
    () =>
      issuePrRows(
        (issueRows ?? []) as Issue[],
        (prRows ?? []) as PrLike[],
        (linkRows ?? []) as PrIssueLinkLike[]
      ),
    [issueRows, prRows, linkRows]
  )
}

/** EXP-1097: the `blocked` shape's name: the relations band's own title. */
export const BLOCKED_BADGE_NAME = RELATIONS_VIEW_COPY.blockedBy

export function PrGraphBadge({
  teamId,
  teamSlug,
  issue = null,
  className,
}: {
  teamId: string
  teamSlug: string
  issue?: Issue | null
  className?: string
}) {
  const [open, setOpen] = useState(false)

  const issues = useTeamIssuePrRows(teamId)
  // The team's `blocks` rows: the subject's direct open blockers (EXP-1097).
  const subjectIssueId = issue?.id ?? null
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
    () =>
      prGraph({
        issue: issue
          ? (issues.find((row) => row.id === issue.id) ?? null)
          : null,
        issues,
        relations,
      }),
    [issue, issues, relations]
  )

  const kind = badgeShape(graph)
  const chipSpec = badgeChip(graph)
  if (!kind || !chipSpec) return null
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
  const Glyph = BADGE_GLYPH[kind]
  const trigger = (
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
        // trigger (a list row): keep clicks inside the dialog there.
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
  graph: PrGraph<IssuePrRow>
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
  // #EXP-1383: an agent assignee draws THE AGENT MARK.
  const agents = useAgentRows(teamId)
  // Every band opens unfolded; the header folds it, as on the relations card.
  const [folded, setFolded] = useState<OverlaySection[]>([])
  const [showAll, setShowAll] = useState<OverlaySection[]>([])
  const flip = (list: OverlaySection[], key: OverlaySection) =>
    list.includes(key) ? list.filter((row) => row !== key) : [...list, key]

  // EXP-930: EVERY issue the view lists opens: a real `<Link>`, so ⌘-click
  // and middle-click work like anywhere else; the dialog closes behind it.
  const issueLink = (row: IssuePrRow) => {
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
  const issueRow = (row: IssuePrRow) => (
    <RelationIssueRow
      key={row.id}
      issue={row}
      open={relationRowIsOpen(row.status)}
      assignee={row.assigneeId ? userMap.get(row.assigneeId) : undefined}
      assigneeAgent={
        row.assigneeAgentId
          ? (agents.find((agent) => agent.id === row.assigneeAgentId) ?? null)
          : null
      }
      phone={phone}
      link={issueLink(row)}
    />
  )

  /** A pull request as the relations card's row: PR glyph · `#n` · the
   *  representative issue's title · its state pill; opens the issue. */
  const prRow = (entry: PrGraphEntry<IssuePrRow>) => {
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
        link={issueLink(row)}
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
        <ListEmpty>{PR_GRAPH_OVERLAY_COPY.empty}</ListEmpty>
      )}
    </div>
  )
}
