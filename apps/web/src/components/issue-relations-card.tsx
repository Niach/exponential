import { useMemo, useState, type ReactElement, type ReactNode } from "react"
import { eq, inArray, or, useLiveQuery } from "@tanstack/react-db"
import type { Issue, User } from "@/db/schema"
import type { IssueRelationType, IssueStatus } from "@/lib/domain"
import { issueCollection, issueRelationCollection } from "@/lib/collections"
import {
  conceptIcon,
  Button,
  cn,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  GlassSectionHeader,
  ListRow,
  Pill,
  ProgressRing,
  TREE_BASE,
  TREE_INDENT,
  UserAvatar,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { relationLabel, type RelationDirection } from "@/lib/issue-relations"
import {
  issueRelationsView,
  RELATIONS_VIEW_COPY,
  type RelationBandKey,
  type RelationsViewBand,
  type RelationsViewInput,
  type RelationsViewRow,
} from "@/lib/issue-relations-view"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { IssuePickerDialog } from "@/components/issue-picker-dialog"
import { IssuePreviewHoverCard } from "@/components/issue-preview-card"
import {
  IssueStatusIcon,
  statusColorClass,
} from "@/components/issue-properties/status-dropdown"
import { issueMenuProps } from "@/components/issue-context-menu/attr"
import { IssueChip } from "@/components/issue-chip"
import { SubIssueComposer } from "@/components/sub-issue-composer"

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
//
// EXP-1097: what the detail DRAWS off these rows is the grouped-bands view
// (`useIssueRelationsView` below, over `lib/issue-relations-view.ts`).

const RelationParentIcon = conceptIcon(`relation-parent`)
const RelationSubIssueIcon = conceptIcon(`relation-sub-issue`)
const RelationBlocksIcon = conceptIcon(`relation-blocks`)
const RelationBlockedByIcon = conceptIcon(`relation-blocked-by`)
const RelationDuplicateIcon = conceptIcon(`relation-duplicate`)
const RelationRelatedIcon = conceptIcon(`relation-related`)
const UiCloseIcon = conceptIcon(`ui-close`)
const UiAddIcon = conceptIcon(`ui-add`)
const UnassignedIcon = conceptIcon(`ui-unassigned`)

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

// ── EXP-1097: direction A, the grouped bands ────────────────────────────
//
// What the detail DRAWS comes off ONE model, `lib/issue-relations-view.ts`
// (fixture-locked ×4, copy included): the "Sub-issue of" parent line above
// the title, the Sub-issues band (completion ring · `done/total` · `+`) over
// flat rows, and one FOLDABLE band per remaining side. This file only feeds
// the model the synced rows and draws its answer; nothing here decides what
// opens, what folds or what the copy says.

/** Where one relation row lands in the view: a band key, or the sub-issue
 *  list / the parent line. Unknown types fold into Related (the model's
 *  rule). */
export type RelationSlot = RelationBandKey | `child` | `parent`

export function relationSlot(
  row: Pick<IssueRelationRow, `type` | `direction`>
): RelationSlot {
  const forward = row.direction === `forward`
  switch (row.type) {
    case `parent`:
      return forward ? `child` : `parent`
    case `blocks`:
      return forward ? `blocking` : `blocked_by`
    case `duplicate`:
      return forward ? `duplicate_of` : `duplicated_by`
    default:
      return `related`
  }
}

/** The model's input off the card's point-of-view rows: each row folded back
 *  to its canonical direction, and the far issues it names. */
export function relationsViewInput(
  issueId: string,
  rows: readonly IssueRelationRow[],
  fold: { toggled: RelationBandKey[]; showAll: RelationBandKey[] }
): RelationsViewInput {
  const issues = new Map<string, RelationsViewInput[`issues`][number]>()
  for (const row of rows) {
    issues.set(row.other.id, {
      id: row.other.id,
      identifier: row.other.identifier,
      title: row.other.title,
      status: row.other.status,
    })
  }
  return {
    subjectId: issueId,
    relations: rows.map((row) =>
      row.direction === `forward`
        ? { type: row.type, issueId, relatedIssueId: row.other.id }
        : { type: row.type, issueId: row.other.id, relatedIssueId: issueId }
    ),
    issues: [...issues.values()],
    toggled: fold.toggled,
    showAll: fold.showAll,
  }
}

interface FoldState {
  toggled: RelationBandKey[]
  showAll: RelationBandKey[]
}

const NO_FOLD: FoldState = { toggled: [], showAll: [] }

// Fold state = SESSION state per issue: it survives prev/next navigation and
// the phone sheet reopening, never a reload (nothing persists it).
const foldMemory = new Map<string, FoldState>()

const flip = (list: RelationBandKey[], key: RelationBandKey) =>
  list.includes(key) ? list.filter((entry) => entry !== key) : [...list, key]

/**
 * The view model for one issue, live: the model's answer plus what the
 * drawing needs beside it — the resolved far issue per row (for its team
 * status glyph), its assignee, the relation row to remove, and the two
 * fold controls.
 */
export function useIssueRelationsView(issueId: string) {
  const rows = useIssueRelations(issueId)
  const [fold, setFold] = useState<{ issueId: string; state: FoldState }>(
    () => ({ issueId, state: foldMemory.get(issueId) ?? NO_FOLD })
  )
  const state =
    fold.issueId === issueId ? fold.state : (foldMemory.get(issueId) ?? NO_FOLD)
  const update = (next: FoldState) => {
    foldMemory.set(issueId, next)
    setFold({ issueId, state: next })
  }

  const view = useMemo(
    () => issueRelationsView(relationsViewInput(issueId, rows, state)),
    [issueId, rows, state]
  )

  const bySlot = useMemo(() => {
    const map = new Map<string, IssueRelationRow>()
    for (const row of rows) {
      const key = `${relationSlot(row)}:${row.other.id}`
      if (!map.has(key)) map.set(key, row)
    }
    return map
  }, [rows])

  // The far issues' assignees: `ResolvedIssueRef` carries no assignee, the
  // synced issue rows do.
  const otherIds = useMemo(
    () => [...new Set(rows.map((row) => row.other.id))].sort(),
    [rows]
  )
  const { data: otherIssues } = useLiveQuery(
    (query) =>
      otherIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, otherIds))
        : undefined,
    [otherIds.join(`,`)]
  )
  const assigneeById = useMemo(
    () =>
      new Map(
        ((otherIssues ?? []) as Issue[]).map((row) => [row.id, row.assigneeId])
      ),
    [otherIssues]
  )

  return {
    view,
    /** The resolved far issue of a view row (its status row, board slug). */
    refOf: (id: string) => rows.find((row) => row.other.id === id)?.other,
    /** The stored relation behind a drawn row — what "remove" deletes. */
    relationOf: (slot: RelationSlot, otherId: string) =>
      bySlot.get(`${slot}:${otherId}`),
    assigneeOf: (id: string) => assigneeById.get(id) ?? null,
    toggle: (key: RelationBandKey) =>
      update({ ...state, toggled: flip(state.toggled, key) }),
    toggleShowAll: (key: RelationBandKey) =>
      update({ ...state, showAll: flip(state.showAll, key) }),
  }
}

type RelationsViewState = ReturnType<typeof useIssueRelationsView>

const BAND_ICON: Record<RelationBandKey, ReturnType<typeof conceptIcon>> = {
  blocked_by: RelationBlockedByIcon,
  blocking: RelationBlocksIcon,
  duplicate_of: RelationDuplicateIcon,
  duplicated_by: RelationDuplicateIcon,
  related: RelationRelatedIcon,
}

/** What the opener of a `RelationIssueRow` is handed when a caller renders it
 *  as a real link (the "Related work" dialog's `<Link>`). */
export interface RelationIssueRowLinkProps {
  className: string
  children: ReactNode
}

/** THE relation row ×4 — sub-issues and every band share it: status glyph ·
 *  mono identifier · title · assignee, the whole row opening the issue.
 *  SLOP-16 r3: exported, so the "Related work" dialog draws THIS row for its
 *  issues rather than a look-alike. `link` swaps the opener for a real link;
 *  `trailing` sits before the assignee (the card's remove button). */
export function RelationIssueRow({
  issue,
  open,
  assignee,
  phone,
  onOpen,
  link,
  trailing,
  depth,
  leading,
  glyph,
  code,
  noAssignee = false,
  className,
}: {
  issue: {
    id: string
    identifier: string
    title: string
    status: string
    statusId?: string | null
  }
  /** Not completed/cancelled — a closed issue's title dims. */
  open: boolean
  assignee?: User
  phone: boolean
  onOpen?: () => void
  link?: (props: RelationIssueRowLinkProps) => ReactElement
  trailing?: ReactNode
  /** Tree nesting (a batch PR's folded issues): the left padding per level;
   *  absent = the card's own 12px padding. */
  depth?: number
  /** Drawn first inside the row — the tree connector of a nested row. */
  leading?: ReactNode
  /** SLOP-16 r5: replaces the status glyph (a pull request row's PR glyph). */
  glyph?: ReactNode
  /** SLOP-16 r5: replaces the mono identifier (a pull request's `#n`). */
  code?: string
  /** SLOP-16 r5: a pull request row ends on its state pill, no assignee. */
  noAssignee?: boolean
  className?: string
}) {
  const openerClass = cn(
    `flex min-w-0 flex-1 items-center self-stretch text-left outline-none`,
    phone ? `gap-3` : `gap-2`
  )
  const openerBody = (
    <>
      {glyph ?? (
        <IssueStatusIcon
          issue={{
            status: issue.status as IssueStatus,
            statusId: issue.statusId ?? null,
          }}
          className={phone ? `size-4 shrink-0` : `size-3.5 shrink-0`}
        />
      )}
      <span className="shrink-0 font-mono text-xs text-muted-foreground">
        {code ?? issue.identifier}
      </span>
      <span
        className={cn(
          `min-w-0 flex-1 truncate`,
          phone ? `text-[0.9375rem]` : `text-sm`,
          !open && `text-foreground/60`
        )}
      >
        {issue.title}
      </span>
    </>
  )
  return (
    <ListRow
      interactive
      density={phone ? `list` : `compact`}
      data-testid={`relation-row-${issue.identifier}`}
      className={cn(
        `group min-w-0`,
        phone ? `min-h-12 gap-3 rounded-none px-3 py-0` : `h-8 rounded-md px-3`,
        depth !== undefined && `relative`,
        className
      )}
      style={
        depth !== undefined
          ? { paddingLeft: `${TREE_BASE + depth * TREE_INDENT}px` }
          : undefined
      }
      {...issueMenuProps(issue.id)}
    >
      {leading}
      {/* The hover preview wraps the identifier + title cluster; the remove
          button stays OUTSIDE the trigger so pointing at it never opens a
          card over the thing being clicked. */}
      <IssuePreviewHoverCard issueId={issue.id}>
        {link ? (
          link({ className: openerClass, children: openerBody })
        ) : (
          <Button
            type="button"
            variant="text"
            size="inline"
            onClick={onOpen}
            // The row's own look, not a button's: the opener is the same
            // bare cluster a `link` row renders.
            className={cn(
              `justify-start rounded-none font-normal text-inherit hover:text-inherit focus-visible:ring-0`,
              openerClass
            )}
          >
            {openerBody}
          </Button>
        )}
      </IssuePreviewHoverCard>
      {trailing}
      {noAssignee ? null : assignee ? (
        <UserAvatar user={assignee} size={phone ? 24 : 20} />
      ) : (
        <span
          className={cn(
            `flex shrink-0 items-center justify-center text-muted-foreground/50`,
            phone ? `size-6` : `size-5`
          )}
        >
          <UnassignedIcon className={phone ? `size-5` : `size-4`} />
        </span>
      )}
    </ListRow>
  )
}

/** The card's row: `RelationIssueRow` bound to the relations model — its
 *  resolved status row, its assignee, and the remove button. */
function RelationBandRow({
  row,
  slot,
  model,
  users,
  phone,
  readOnly,
}: {
  row: RelationsViewRow
  slot: RelationSlot
  model: RelationsViewState
  users: ReadonlyMap<string, User>
  phone: boolean
  readOnly: boolean
}) {
  const issueRefs = useIssueRefs()
  const ref = model.refOf(row.id)
  const relation = model.relationOf(slot, row.id)
  const assigneeId = model.assigneeOf(row.id)
  const assignee = assigneeId ? users.get(assigneeId) : undefined
  return (
    <RelationIssueRow
      issue={{
        id: row.id,
        identifier: row.identifier,
        title: row.title,
        status: ref?.status ?? row.status,
        statusId: ref ? ref.statusId : null,
      }}
      open={row.open}
      assignee={assignee}
      phone={phone}
      onOpen={() => issueRefs?.open(row.identifier)}
      trailing={
        !readOnly && relation ? (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`Remove relation to ${row.identifier}`}
            // A phone has no hover: an invisible tap target beside the avatar
            // would delete a link unseen, so there it is always drawn.
            className={cn(
              `shrink-0 text-muted-foreground`,
              !phone &&
                `opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100`
            )}
            onClick={() => removeRelation(relation)}
          >
            <UiCloseIcon className="size-3.5" />
          </Button>
        ) : null
      }
    />
  )
}

/** The rows under a band: flat on md+, hairline-divided on phones. */
export function RowList({ phone, children }: { phone: boolean; children: ReactNode }) {
  return (
    <div className={cn(`flex flex-col`, phone && `divide-y divide-glass-stroke`)}>
      {children}
    </div>
  )
}

/** THE foldable band ×4: chevron · icon · title · count, its rows, then
 *  "Show N more" / "Show less". SLOP-16 r5: exported, so the "Related work"
 *  dialog draws THIS band over its own rows rather than a look-alike. */
export function RelationBandFrame({
  testId,
  icon: Icon,
  title,
  count,
  expanded,
  onToggle,
  tail,
  onTail,
  phone,
  children,
}: {
  testId: string
  icon: ReturnType<typeof conceptIcon>
  title: string
  count: number
  expanded: boolean
  onToggle: () => void
  /** "Show N more" / "Show less"; null when nothing is hidden. */
  tail: string | null
  onTail: () => void
  phone: boolean
  children: ReactNode
}) {
  return (
    <div className="flex flex-col" data-testid={testId}>
      <GlassSectionHeader
        label={title}
        leading={<Icon className="size-3.5 shrink-0 text-muted-foreground" />}
        count={count}
        expanded={expanded}
        onToggle={onToggle}
        className={phone ? `mb-0 py-2` : `mb-0.5`}
      />
      {expanded && (
        <RowList phone={phone}>
          {children}
          {tail && (
            <button
              type="button"
              onClick={onTail}
              className={cn(
                `flex items-center rounded-md text-left text-muted-foreground outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50`,
                phone ? `h-11 px-3 text-sm` : `h-7 px-3 text-xs`
              )}
            >
              {tail}
            </button>
          )}
        </RowList>
      )}
    </div>
  )
}

/** One foldable band of the view, bound to the relations model. */
function RelationBand({
  band,
  model,
  users,
  phone,
  readOnly,
}: {
  band: RelationsViewBand
  model: RelationsViewState
  users: ReadonlyMap<string, User>
  phone: boolean
  readOnly: boolean
}) {
  return (
    <RelationBandFrame
      testId={`relation-band-${band.key}`}
      icon={BAND_ICON[band.key]}
      title={band.title}
      count={band.count}
      expanded={band.expanded}
      onToggle={() => model.toggle(band.key)}
      tail={band.more ?? band.less}
      onTail={() => model.toggleShowAll(band.key)}
      phone={phone}
    >
      {band.rows.map((row) => (
        <RelationBandRow
          key={row.id}
          row={row}
          slot={band.key}
          model={model}
          users={users}
          phone={phone}
          readOnly={readOnly}
        />
      ))}
    </RelationBandFrame>
  )
}

/** The bands alone — the md+ detail under Sub-issues, the phone sheet under
 *  its "Relations" heading. Nothing when the issue has no such relation. */
function RelationBands({
  model,
  phone,
  readOnly,
}: {
  model: RelationsViewState
  phone: boolean
  readOnly: boolean
}) {
  const { userMap } = useTeamUsers(useIssueRefs()?.teamId)
  if (model.view.bands.length === 0) return null
  return (
    <div className="flex flex-col gap-1.5">
      {model.view.bands.map((band) => (
        <RelationBand
          key={band.key}
          band={band}
          model={model}
          users={userMap}
          phone={phone}
          readOnly={readOnly}
        />
      ))}
    </div>
  )
}

/**
 * The Sub-issues block: the band (completion ring filled done/total in the
 * team's COMPLETED colour · "Sub-issues" · `done/total` · `+`) over flat
 * rows, or an "Add sub-issues" row while there are none. The `+` and that
 * row open the EXISTING inline composer (`SubIssueComposer`, rendered by the
 * caller under the rows). Read-only viewers see the band without the `+`, and
 * nothing at all when there are no sub-issues.
 */
function SubIssuesBlock({
  model,
  phone,
  readOnly,
  composing,
  onCompose,
}: {
  model: RelationsViewState
  phone: boolean
  readOnly: boolean
  composing: boolean
  onCompose: () => void
}) {
  const { resolve: resolveStatus } = useTeamStatusesContext()
  const { userMap } = useTeamUsers(useIssueRefs()?.teamId)
  const { rows, done, total, progress } = model.view.subIssues

  if (total === 0) {
    if (readOnly || composing) return null
    return (
      <button
        type="button"
        data-testid="add-sub-issues"
        onClick={onCompose}
        className={cn(
          `flex items-center gap-2 rounded-md px-3 text-left text-sm text-muted-foreground outline-none transition-colors duration-fast hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50`,
          // EXP-1191: md+ hangs the hover fill outside the column so the
          // glyph sits on its edge.
          phone ? `h-11 bg-glass-section` : `-mx-3 h-8 hover:bg-glass-row`
        )}
      >
        <UiAddIcon className="size-4" />
        {RELATIONS_VIEW_COPY.addSubIssues}
      </button>
    )
  }

  const completed = resolveStatus({ status: `done`, statusId: null })
  return (
    <section className="flex flex-col" data-testid="sub-issues">
      <GlassSectionHeader
        label={RELATIONS_VIEW_COPY.subIssues}
        leading={
          <ProgressRing
            done={done}
            total={total}
            colorClass={completed ? statusColorClass(completed) : undefined}
            colorHex={
              completed && !completed.builtinKey ? completed.colorHex : undefined
            }
          />
        }
        className={phone ? `mb-0 py-2` : `mb-0.5`}
        trailing={
          <>
            <span className="font-mono text-xs tabular-nums text-muted-foreground">
              {progress}
            </span>
            {!readOnly && (
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label="Add sub-issue"
                className="-my-1 -mr-1.5 text-muted-foreground"
                onClick={onCompose}
              >
                <UiAddIcon />
              </Button>
            )}
          </>
        }
      />
      <RowList phone={phone}>
        {rows.map((row) => (
          <RelationBandRow
            key={row.id}
            row={row}
            slot="child"
            model={model}
            users={userMap}
            phone={phone}
            readOnly={readOnly}
          />
        ))}
      </RowList>
    </section>
  )
}

/**
 * The "Sub-issue of [parent chip]" line ABOVE the title (`view.parent`).
 * Nothing when the issue has no parent.
 */
export function IssueParentLine({
  issueId,
  phone = false,
}: {
  issueId: string
  phone?: boolean
}) {
  const issueRefs = useIssueRefs()
  const model = useIssueRelationsView(issueId)
  const parent = model.view.parent
  const ref = parent ? model.refOf(parent.id) : undefined
  if (!parent || !ref) return null
  return (
    <div
      data-testid="sub-issue-of"
      className={cn(
        `flex min-w-0 items-center gap-1.5 text-muted-foreground`,
        phone ? `px-5 pt-3 text-[0.8125rem]` : `-mb-2 px-5 pt-4 text-xs`
      )}
    >
      <RelationSubIssueIcon className="size-3.5 shrink-0" />
      <span className="shrink-0">{RELATIONS_VIEW_COPY.subIssueOf}</span>
      <IssueChip
        issue={ref}
        onClick={issueRefs ? () => issueRefs.open(ref.identifier) : undefined}
        className="min-w-0"
      />
    </div>
  )
}

/**
 * The issue detail's relations block under the description: the Sub-issues
 * block with the inline composer, then — md+ only — the foldable bands. The
 * phone draws the bands inside its properties sheet instead
 * (`MobileRelationBands`). "Add relation" stays in the header's `…` menu
 * (EXP-760) and the phone sheet's "Add".
 */
export function IssueRelationsSection({
  issue,
  teamId,
  users,
  readOnly = false,
  phone = false,
}: {
  issue: Issue
  teamId: string
  users: User[]
  readOnly?: boolean
  phone?: boolean
}) {
  const model = useIssueRelationsView(issue.id)
  // The composer belongs to the issue it was opened on: prev/next onto
  // another issue closes it (the section is not remounted per issue).
  const [composingFor, setComposingFor] = useState<string | null>(null)
  const composing = composingFor === issue.id
  const setComposing = (open: boolean) =>
    setComposingFor(open ? issue.id : null)
  const showBands = !phone && model.view.bands.length > 0
  const hasSubIssues = model.view.subIssues.total > 0
  if (readOnly && !hasSubIssues && !showBands) return null

  return (
    <div
      className={cn(
        `flex flex-col gap-4`,
        phone ? `px-4 pt-5` : `px-5 pt-3`
      )}
    >
      {(hasSubIssues || !readOnly) && (
        <div className="flex flex-col">
          <SubIssuesBlock
            model={model}
            phone={phone}
            readOnly={readOnly}
            composing={composing}
            onCompose={() => setComposing(true)}
          />
          {/* Keyed on the issue so prev/next navigation never carries a
              half-typed child over (REV-47's rule). */}
          {!readOnly && composing && (
            <SubIssueComposer
              key={`sub-issues:${issue.id}`}
              parent={issue}
              teamId={teamId}
              users={users}
              open
              onOpenChange={setComposing}
            />
          )}
        </div>
      )}
      {showBands && (
        <RelationBands model={model} phone={false} readOnly={readOnly} />
      )}
    </div>
  )
}

/**
 * The phone properties sheet's "Relations" block: the heading with "Add",
 * then the same bands as md+ (sub-issues and the parent live on the detail
 * itself). Hidden for a read-only viewer with nothing to show.
 */
export function MobileRelationBands({
  issueId,
  readOnly = false,
}: {
  issueId: string
  readOnly?: boolean
}) {
  const model = useIssueRelationsView(issueId)
  if (readOnly && model.view.bands.length === 0) return null
  return (
    <div className="flex flex-col gap-1.5" data-testid="mobile-relations">
      <div className="flex items-center px-5 pb-0.5">
        <span className="text-sm font-medium text-foreground/85">
          {RELATIONS_VIEW_COPY.relations}
        </span>
        <span className="flex-1" />
        {!readOnly && (
          <IssueRelationsAdd
            issueId={issueId}
            trigger={
              <Pill size="sm" mode="action" leading={<UiAddIcon />}>
                {RELATIONS_VIEW_COPY.add}
              </Pill>
            }
          />
        )}
      </div>
      <div className="px-4">
        <RelationBands model={model} phone readOnly={readOnly} />
      </div>
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
