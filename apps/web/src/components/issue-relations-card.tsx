import { useMemo, useState } from "react"
import { eq, or, useLiveQuery } from "@tanstack/react-db"
import type { IssueRelationType } from "@/lib/domain"
import { issueRelationCollection } from "@/lib/collections"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import {
  relationLabel,
  type RelationDirection,
} from "@/lib/issue-relations"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { GlassSectionHeader } from "@/components/ui/glass-rows"
import { Pill } from "@/components/ui/pill"

// EXP-736 — the issue's relation graph, both sides in one card. Rows come off
// the `issue_relations` shape (never a fetch): the shape carries a row as long
// as EITHER issue's board is visible, and the far issue is resolved through
// the already-synced issues shape, so a row whose other side isn't visible to
// this viewer is simply dropped rather than rendered half-blank.
//
// Every pick is stored in ONE canonical direction (lib/issue-relations.ts);
// the inverse halves of the picker just pass `inverse: true`. "Duplicate of"
// is the exception: it is the dual-write of issues.duplicate_of_id, so it goes
// through issues.update and comes back as a mirrored row.

const RelationSectionIcon = conceptIcon(`relation-section`)
const RelationParentIcon = conceptIcon(`relation-parent`)
const RelationSubIssueIcon = conceptIcon(`relation-sub-issue`)
const RelationBlocksIcon = conceptIcon(`relation-blocks`)
const RelationBlockedByIcon = conceptIcon(`relation-blocked-by`)
const RelationDuplicateIcon = conceptIcon(`relation-duplicate`)
const RelationRelatedIcon = conceptIcon(`relation-related`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiCloseIcon = conceptIcon(`ui-close`)

type RelationSide = `${IssueRelationType}:${RelationDirection}`

// One ordered table: the picker entries, the row glyphs, the group labels and
// the sort order all read off it, so a new relation type is one edit here.
const RELATION_SIDES: Array<{
  side: RelationSide
  type: IssueRelationType
  direction: RelationDirection
  icon: ReturnType<typeof conceptIcon>
  /** Offered in the "Add relation" menu. `duplicated by` is not: it would
   * mark the OTHER issue as a duplicate, which belongs on that issue. */
  pickable: boolean
}> = [
  {
    side: `parent:forward`,
    type: `parent`,
    direction: `forward`,
    icon: RelationParentIcon,
    pickable: true,
  },
  {
    side: `parent:inverse`,
    type: `parent`,
    direction: `inverse`,
    icon: RelationSubIssueIcon,
    pickable: true,
  },
  {
    side: `blocks:forward`,
    type: `blocks`,
    direction: `forward`,
    icon: RelationBlocksIcon,
    pickable: true,
  },
  {
    side: `blocks:inverse`,
    type: `blocks`,
    direction: `inverse`,
    icon: RelationBlockedByIcon,
    pickable: true,
  },
  {
    side: `duplicate:forward`,
    type: `duplicate`,
    direction: `forward`,
    icon: RelationDuplicateIcon,
    pickable: true,
  },
  {
    side: `duplicate:inverse`,
    type: `duplicate`,
    direction: `inverse`,
    icon: RelationDuplicateIcon,
    pickable: false,
  },
  {
    side: `related:forward`,
    type: `related`,
    direction: `forward`,
    icon: RelationRelatedIcon,
    pickable: true,
  },
  {
    side: `related:inverse`,
    type: `related`,
    direction: `inverse`,
    icon: RelationRelatedIcon,
    pickable: false,
  },
]

const SIDE_ORDER = new Map(
  RELATION_SIDES.map((entry, index) => [entry.side, index])
)
const SIDE_BY_KEY = new Map(
  RELATION_SIDES.map((entry) => [entry.side, entry])
)

/** "blocked by" → "Blocked by" — the label table stays lowercase so the
 * activity phrases read as sentences. */
function sideLabel(
  type: IssueRelationType,
  direction: RelationDirection
): string {
  const label = relationLabel(type, direction)
  return label.charAt(0).toUpperCase() + label.slice(1)
}

export interface IssueRelationRow {
  id: string
  type: IssueRelationType
  source: string
  direction: RelationDirection
  other: ResolvedIssueRef
}

/**
 * This issue's relation rows, folded to its point of view and ordered by the
 * picker's own order. Rows whose far issue is not synced (another team's
 * board, a trashed board) are dropped — the card must never show a blank row.
 */
export function useIssueRelations(issueId: string): IssueRelationRow[] {
  const issueRefs = useIssueRefs()
  const { data: rows } = useLiveQuery(
    (query) =>
      query
        .from({ relations: issueRelationCollection })
        .where(({ relations }) =>
          // `or()` from @tanstack/react-db, never `||` — a JS operator here
          // silently evaluates to a constant and the filter disappears.
          or(
            eq(relations.issueId, issueId),
            eq(relations.relatedIssueId, issueId)
          )
        ),
    [issueId]
  )

  const resolveById = issueRefs?.resolveById
  return useMemo(() => {
    if (!resolveById) return []
    const mapped: IssueRelationRow[] = []
    for (const row of rows ?? []) {
      const direction: RelationDirection =
        row.issueId === issueId ? `forward` : `inverse`
      const otherId =
        direction === `forward` ? row.relatedIssueId : row.issueId
      const other = resolveById(otherId)
      if (!other) continue
      mapped.push({
        id: row.id,
        type: row.type,
        source: row.source,
        direction,
        other,
      })
    }
    mapped.sort((a, b) => {
      const order =
        (SIDE_ORDER.get(`${a.type}:${a.direction}`) ?? 0) -
        (SIDE_ORDER.get(`${b.type}:${b.direction}`) ?? 0)
      return order !== 0
        ? order
        : a.other.identifier.localeCompare(b.other.identifier)
    })
    return mapped
  }, [rows, issueId, resolveById])
}

function removeRelation(row: IssueRelationRow) {
  void trpc.relations.delete.mutate({ id: row.id })
}

/** The bare row list — the card below and the phone properties sheet share it. */
export function IssueRelationsList({
  rows,
  readOnly = false,
}: {
  rows: IssueRelationRow[]
  readOnly?: boolean
}) {
  const issueRefs = useIssueRefs()
  if (rows.length === 0) return null

  return (
    <div className="flex flex-col">
      {rows.map((row) => {
        const entry = SIDE_BY_KEY.get(`${row.type}:${row.direction}`)
        const Icon = entry?.icon ?? RelationRelatedIcon
        return (
          <div
            key={row.id}
            className="group flex min-w-0 items-center gap-2 py-1"
          >
            <Icon className="size-3.5 shrink-0 text-muted-foreground" />
            <button
              type="button"
              onClick={() => issueRefs?.open(row.other.identifier)}
              className="shrink-0 font-mono text-xs text-muted-foreground hover:text-foreground"
            >
              {`#${row.other.identifier}`}
            </button>
            <IssueStatusIcon issue={row.other} className="size-3.5 shrink-0" />
            <span className="min-w-0 flex-1 truncate text-sm">
              {row.other.title}
            </span>
            <span className="shrink-0 text-xs text-muted-foreground">
              {sideLabel(row.type, row.direction)}
            </span>
            {!readOnly && (
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Remove relation to ${row.other.identifier}`}
                className="shrink-0 text-muted-foreground opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
                onClick={() => removeRelation(row)}
              >
                <UiCloseIcon className="size-3.5" />
              </Button>
            )}
          </div>
        )
      })}
    </div>
  )
}

interface PendingPick {
  type: IssueRelationType
  direction: RelationDirection
}

/**
 * The "Add relation" control: a menu of the six offered sides, then the shared
 * issue picker. Rendered by both the desktop card and the phone sheet, so the
 * two can never offer different picks.
 */
export function IssueRelationsAdd({
  issueId,
  trigger,
}: {
  issueId: string
  trigger: React.ReactNode
}) {
  const [pending, setPending] = useState<PendingPick | null>(null)

  const handlePick = (issue: ResolvedIssueRef) => {
    if (!pending) return
    const pick = pending
    setPending(null)
    if (pick.type === `duplicate`) {
      // Dual-written: the mirrored relation row is produced server-side by
      // syncDuplicateMirror, and the status/duplicateOfId lockstep stays in
      // issues.update.
      void trpc.issues.update.mutate({
        id: issueId,
        duplicateOfId: issue.id,
      })
      return
    }
    void trpc.relations.create.mutate({
      issueId,
      relatedIssueId: issue.id,
      type: pick.type,
      inverse: pick.direction === `inverse`,
    })
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-[12rem]">
          {RELATION_SIDES.filter((entry) => entry.pickable).map((entry) => {
            const Icon = entry.icon
            return (
              <DropdownMenuItem
                key={entry.side}
                onSelect={() => {
                  // Defer past the menu close + focus restore so the picker's
                  // focus trap doesn't fight Radix.
                  setTimeout(
                    () =>
                      setPending({
                        type: entry.type,
                        direction: entry.direction,
                      }),
                    0
                  )
                }}
              >
                <Icon />
                {sideLabel(entry.type, entry.direction)}
              </DropdownMenuItem>
            )
          })}
        </DropdownMenuContent>
      </DropdownMenu>

      <IssuePickerDialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open) setPending(null)
        }}
        onPick={handlePick}
        excludeIssueIds={[issueId]}
        title={
          pending
            ? `${sideLabel(pending.type, pending.direction)}…`
            : `Select issue`
        }
        placeholder="Search issues…"
      />
    </>
  )
}

/**
 * The desktop/tablet card, mounted directly beneath the properties band on the
 * issue detail page. Hidden entirely when there is nothing to show and nothing
 * to add; otherwise the header and "Add relation" stand alone above an empty
 * list, which is what makes the affordance discoverable.
 */
export function IssueRelationsCard({
  issueId,
  readOnly = false,
}: {
  issueId: string
  readOnly?: boolean
}) {
  const rows = useIssueRelations(issueId)
  if (readOnly && rows.length === 0) return null

  return (
    <div className="mx-auto w-full max-w-3xl px-4 pt-3">
      <div className="rounded-xl border border-glass-stroke-card bg-popover/40 px-3 py-2">
        <GlassSectionHeader
          label="Relations"
          leading={
            <RelationSectionIcon className="size-3.5 text-muted-foreground" />
          }
          className="pt-0 pb-1"
          trailing={
            readOnly ? undefined : (
              <IssueRelationsAdd
                issueId={issueId}
                trigger={
                  <Pill size="sm" mode="action" leading={<UiAddIcon />}>
                    Add relation
                  </Pill>
                }
              />
            )
          }
        />
        <IssueRelationsList rows={rows} readOnly={readOnly} />
      </div>
    </div>
  )
}
