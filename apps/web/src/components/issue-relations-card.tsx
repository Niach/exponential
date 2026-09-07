import { useMemo, useState } from "react"
import { eq, or, useLiveQuery } from "@tanstack/react-db"
import type { IssueRelationType } from "@/lib/domain"
import { issueRelationCollection } from "@/lib/collections"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import {
  groupRelationRows,
  relationLabel,
  type RelationDirection,
} from "@/lib/issue-relations"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import { IssuePreviewHoverCard } from "@/components/issue-preview-card"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

// EXP-736 — the issue's relation graph, both sides in one card. Rows come off
// the `issue_relations` shape (never a fetch): the shape is scoped by the row's
// SOURCE issue's board (its `board_id` mirror), so the far issue is resolved
// separately through the already-synced issues shape and a row whose other side
// isn't visible to this viewer is dropped rather than rendered half-blank.
//
// Every pick is stored in ONE canonical direction (lib/issue-relations.ts);
// the inverse halves of the picker just pass `inverse: true`. "Duplicate of"
// is the exception: it is the dual-write of issues.duplicate_of_id, so it goes
// through issues.update and comes back as a mirrored row.

const RelationParentIcon = conceptIcon(`relation-parent`)
const RelationSubIssueIcon = conceptIcon(`relation-sub-issue`)
const RelationBlocksIcon = conceptIcon(`relation-blocks`)
const RelationBlockedByIcon = conceptIcon(`relation-blocked-by`)
const RelationDuplicateIcon = conceptIcon(`relation-duplicate`)
const RelationRelatedIcon = conceptIcon(`relation-related`)
const UiCloseIcon = conceptIcon(`ui-close`)

type RelationSide = `${IssueRelationType}:${RelationDirection}`

// One ordered table: the picker entries, the row glyphs, the group labels and
// the sort order all read off it, so a new relation type is one edit here.
export const RELATION_SIDES: Array<{
  side: RelationSide
  type: IssueRelationType
  direction: RelationDirection
  icon: ReturnType<typeof conceptIcon>
  /** The "Add relation" MENU wording, byte-identical to the natives'
   * picker (iOS `RelationPick.all`, Android `RELATION_PICKS`, desktop
   * `relation_picks`). It is NOT derived from the contract label: `blocks`
   * reads as "Blocking" in a picker ("Blocks…" states a fact about a row that
   * does not exist yet), while the row caption keeps the contract wording.
   * Null on a side that is not offered. */
  pickLabel: string | null
  /** Offered in the "Add relation" menu. `duplicated by` is not: it would
   * mark the OTHER issue as a duplicate, which belongs on that issue. */
  pickable: boolean
}> = [
  {
    side: `parent:forward`,
    type: `parent`,
    direction: `forward`,
    icon: RelationParentIcon,
    pickLabel: `Parent of`,
    pickable: true,
  },
  {
    side: `parent:inverse`,
    type: `parent`,
    direction: `inverse`,
    icon: RelationSubIssueIcon,
    pickLabel: `Sub-issue of`,
    pickable: true,
  },
  {
    side: `blocks:forward`,
    type: `blocks`,
    direction: `forward`,
    icon: RelationBlocksIcon,
    pickLabel: `Blocking`,
    pickable: true,
  },
  {
    side: `blocks:inverse`,
    type: `blocks`,
    direction: `inverse`,
    icon: RelationBlockedByIcon,
    pickLabel: `Blocked by`,
    pickable: true,
  },
  {
    side: `duplicate:forward`,
    type: `duplicate`,
    direction: `forward`,
    icon: RelationDuplicateIcon,
    pickLabel: `Duplicate of`,
    pickable: true,
  },
  {
    side: `duplicate:inverse`,
    type: `duplicate`,
    direction: `inverse`,
    icon: RelationDuplicateIcon,
    pickLabel: null,
    pickable: false,
  },
  {
    side: `related:forward`,
    type: `related`,
    direction: `forward`,
    icon: RelationRelatedIcon,
    pickLabel: `Related to`,
    pickable: true,
  },
  {
    side: `related:inverse`,
    type: `related`,
    direction: `inverse`,
    icon: RelationRelatedIcon,
    pickLabel: null,
    pickable: false,
  },
]

const SIDE_ORDER = new Map(
  RELATION_SIDES.map((entry, index) => [entry.side, index])
)
const SIDE_BY_KEY = new Map(
  RELATION_SIDES.map((entry) => [entry.side, entry])
)

/** The ROW caption: the contract label verbatim, lowercase, exactly as iOS
 * (`Text(relation.label)`) and Android (`"${relation.label} · IDENT"`) read
 * it. The picker wording is a separate string — see `pickLabel`. */
export function rowLabel(
  type: IssueRelationType,
  direction: RelationDirection
): string {
  return relationLabel(type, direction)
}

/** The menu/dialog wording for one side, from the table above. */
export function pickLabel(
  type: IssueRelationType,
  direction: RelationDirection
): string {
  return (
    SIDE_BY_KEY.get(`${type}:${direction}`)?.pickLabel ??
    relationLabel(type, direction)
  )
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

/**
 * EXP-760: the rows, folded into the Linear-style heading groups the shared
 * table defines (lib/issue-relations.ts — desktop `group_title` mirrors it).
 * There is no card and no "Relations" title above them: the group heading IS
 * the label, so a row no longer needs its own trailing caption either.
 *
 * "Sub-issues" additionally carries a `done/total` counter, keyed on the
 * team's own status CATEGORY (a custom completed status counts), which is why
 * the counter lives here rather than in the pure grouping helper.
 */
export function IssueRelationGroups({
  rows,
  readOnly = false,
}: {
  rows: IssueRelationRow[]
  readOnly?: boolean
}) {
  const issueRefs = useIssueRefs()
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const groups = groupRelationRows(rows)
  if (groups.length === 0) return null

  return (
    <div className="flex flex-col gap-3">
      {groups.map((group) => {
        const done =
          group.key === `parent:forward`
            ? group.rows.filter(
                (row) => resolveStatus(row.other).category === `completed`
              ).length
            : null

        return (
          <div key={group.key} className="flex min-w-0 flex-col">
            <div className="flex items-center gap-1.5 pb-0.5 text-xs font-medium text-muted-foreground">
              <span>{group.title}</span>
              {done !== null && (
                <span className="font-mono tabular-nums text-muted-foreground/70">
                  {`${done}/${group.rows.length}`}
                </span>
              )}
            </div>
            {group.rows.map((row) => {
              const entry = SIDE_BY_KEY.get(`${row.type}:${row.direction}`)
              const Icon = entry?.icon ?? RelationRelatedIcon
              return (
                <div
                  key={row.id}
                  className="group flex min-w-0 items-center gap-2 py-1"
                >
                  <Icon className="size-3.5 shrink-0 text-muted-foreground" />
                  {/* The hover preview wraps the identifier + title cluster;
                      the remove button stays OUTSIDE the trigger so pointing
                      at it never opens a card over the thing being clicked. */}
                  <IssuePreviewHoverCard issueId={row.other.id}>
                    <button
                      type="button"
                      onClick={() => issueRefs?.open(row.other.identifier)}
                      className="flex min-w-0 flex-1 items-center gap-2 text-left"
                    >
                      <span className="shrink-0 font-mono text-xs text-muted-foreground group-hover:text-foreground">
                        {`#${row.other.identifier}`}
                      </span>
                      <IssueStatusIcon
                        issue={row.other}
                        className="size-3.5 shrink-0"
                      />
                      <span className="min-w-0 flex-1 truncate text-sm">
                        {row.other.title}
                      </span>
                    </button>
                  </IssuePreviewHoverCard>
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
      })}
    </div>
  )
}

/**
 * The issue detail's relations block: the grouped rows in the reading column's
 * gutter, and NOTHING at all when the issue has no relations (EXP-760 — the
 * "Add relation" affordance moved into the header's `…` menu, so an empty
 * block has no reason to exist).
 */
export function IssueRelationsSection({
  issueId,
  readOnly = false,
}: {
  issueId: string
  readOnly?: boolean
}) {
  const rows = useIssueRelations(issueId)
  if (rows.length === 0) return null

  return (
    <div className="mx-auto w-full max-w-3xl px-4 pt-3">
      <IssueRelationGroups rows={rows} readOnly={readOnly} />
    </div>
  )
}

interface PendingPick {
  type: IssueRelationType
  direction: RelationDirection
}

/**
 * The "Add relation" flow WITHOUT its trigger: `pick(side)` opens the shared
 * issue picker for one side, `dialog` is the picker element the caller renders
 * beside its other portals.
 *
 * Split out of the old menu+picker component (EXP-760) because the trigger is
 * now a MENU ITEM in three different menus — the issue header's `…`, the list
 * row's context menu, and the phone properties sheet's own chip — while the
 * picking rules (the deferral past the menu close, the duplicate dual-write)
 * must stay in exactly one place.
 */
export function useAddRelation(issueId: string) {
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

  const pick = (side: { type: IssueRelationType; direction: RelationDirection }) => {
    // Defer past the menu close + focus restore so the picker's focus trap
    // doesn't fight Radix.
    setTimeout(
      () => setPending({ type: side.type, direction: side.direction }),
      0
    )
  }

  const dialog = (
    <IssuePickerDialog
      open={pending !== null}
      onOpenChange={(open) => {
        if (!open) setPending(null)
      }}
      onPick={handlePick}
      excludeIssueIds={[issueId]}
      title={
        pending
          ? `${pickLabel(pending.type, pending.direction)}…`
          : `Select issue`
      }
      placeholder="Search issues…"
    />
  )

  return { pick, dialog }
}

/**
 * The dropdown form of the same flow — still used by the PHONE properties
 * sheet, which has no `…` menu of its own to hang the six sides off.
 */
export function IssueRelationsAdd({
  issueId,
  trigger,
}: {
  issueId: string
  trigger: React.ReactNode
}) {
  const { pick, dialog } = useAddRelation(issueId)

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
                onSelect={() => pick(entry)}
              >
                <Icon />
                {pickLabel(entry.type, entry.direction)}
              </DropdownMenuItem>
            )
          })}
        </DropdownMenuContent>
      </DropdownMenu>
      {dialog}
    </>
  )
}
