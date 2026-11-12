import { type ComponentProps, type ReactNode } from "react"
import { CHATS_SHELL } from "@exp/domain-contract/chats"
import { NEEDS_YOU_COPY, NEEDS_YOU_TEST_IDS, type NeedsYouRow } from "@exp/domain-contract/needs-you"
import fixture from "@exp/domain-contract/fixtures/needs-you.json"
import { WAITING_CARDS_ICONS } from "@exp/domain-contract/waiting-cards"
import mergeTrain from "@exp/domain-contract/fixtures/merge-train.json"
import {
  MERGE_TRAIN_COPY,
  MERGE_TRAIN_TEST_IDS,
  stuckRow,
  type StuckInput,
  type StuckRow,
} from "@exp/domain-contract/merge-train"
import { Badge } from "./badge"
import { Button } from "./button"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { ListRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// #FEED-191 NEEDS YOU ×web (fixtures/needs-you.json; desktop domain::needs_you,
// iOS ExpCore NeedsYou.swift, Android domain/NeedsYou.kt): the model is the
// contract's `needsYou()`; these only paint it.
//
//   NeedsYouList      one reading column of THE inbox page's ListRow, else
//                     THE EmptyState; a stuck PR's actions under its meta
//   NeedsYouEntryRow  the phones' first row of Chats (the Archived row's box)
//   NeedsYouStuckButtons  a stuck PR's THE Button sm row (merge-train `stuckRow`)
//   NeedsYouSpecimen  the styleguide's `needs-you`: `cases[1]` + one stuck row
//                     (merge-train `stuckRow` cases[0]) + the entry row

const NEEDS_YOU_TONE = `text-yellow-400`
const EntryIcon = conceptIcon(`nav-needs-you`)
const PlanIcon = conceptIcon(WAITING_CARDS_ICONS.plan as Parameters<typeof conceptIcon>[0])
const QuestionIcon = conceptIcon(WAITING_CARDS_ICONS.question as Parameters<typeof conceptIcon>[0])
const PrIcon = conceptIcon(`pr-open`)
const GLYPHS = { plan: PlanIcon, question: QuestionIcon, pr: PrIcon } as const

export function NeedsYouList<Row extends NeedsYouRow>({
  rows,
  onOpen,
  actions,
  className,
}: {
  rows: readonly Row[]
  onOpen?: (row: Row) => void
  /** #EXP-1710: a row's THE Button sm row under its meta (a stuck PR's
   *  Retry / Merge now / Close PR); null = none. */
  actions?: (row: Row) => ReactNode
  className?: string
}) {
  if (rows.length === 0) {
    return <EmptyState icon={EntryIcon} title={NEEDS_YOU_COPY.empty} size="page" />
  }
  return (
    <div data-testid={NEEDS_YOU_TEST_IDS.list} className={cn(`flex flex-col`, className)}>
      {rows.map((row) => {
        const Glyph = GLYPHS[row.kind]
        return (
          <ListRow
            key={row.messageId}
            interactive
            density="list"
            // THE inbox page's reading row.
            className="items-start px-3 py-2"
            data-testid={NEEDS_YOU_TEST_IDS.row}
            data-message-id={row.messageId}
            data-kind={row.kind}
            onClick={() => onOpen?.(row)}
          >
            <Glyph className={cn(`mt-0.5 size-4 shrink-0`, NEEDS_YOU_TONE)} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-sm font-medium">{row.title}</span>
                <span className="ml-auto shrink-0 pl-2 text-right text-xs text-muted-foreground">{row.age}</span>
              </div>
              <div className="mt-0.5 truncate text-xs text-muted-foreground">
                {[row.chat, row.meta].filter(Boolean).join(` · `)}
              </div>
              {actions?.(row)}
            </div>
          </ListRow>
        )
      })}
    </div>
  )
}

/** The phones' Needs you entry: as tall as the Archived row, the glyph in the
 *  needs-you tone centred in the lead disc's column (no disc), the count Badge
 *  trailing in the same tone. */
export function NeedsYouEntryRow({
  count,
  onOpen,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `onClick`> & { count: number; onOpen?: () => void }) {
  return (
    <button
      type="button"
      data-testid={NEEDS_YOU_TEST_IDS.entry}
      {...props}
      onClick={onOpen}
      className={cn(
        `flex w-full items-center gap-4 text-left outline-none transition-colors duration-fast active:bg-glass-active/40 focus-visible:bg-glass-active/40`,
        className
      )}
    >
      <span className={cn(`flex shrink-0 justify-center`, NEEDS_YOU_TONE)} style={{ width: CHATS_SHELL.leadDisc }}>
        <EntryIcon style={{ width: CHATS_SHELL.archivedGlyph, height: CHATS_SHELL.archivedGlyph }} />
      </span>
      <span className="flex min-w-0 flex-1 items-center gap-2 self-stretch border-b border-glass-stroke py-3">
        <span className="min-w-0 flex-1 truncate text-[15px] font-medium text-foreground">{NEEDS_YOU_COPY.entry}</span>
        <Badge count={count} tone="attention" />
      </span>
    </button>
  )
}

export type StuckAction = StuckRow[`actions`][number]

const STUCK_LABELS: Record<StuckAction, string> = {
  retry: MERGE_TRAIN_COPY.needsYou.retry,
  "merge-now": MERGE_TRAIN_COPY.needsYou.mergeNow,
  "close-pr": MERGE_TRAIN_COPY.needsYou.close,
}

/** #EXP-1710 (merge-train.json `stuckRow`): a stuck PR's actions as THE
 *  Button sm row under its meta: Retry · Merge now (owners) · Close PR. A
 *  press never opens the row. `children` = the caller's dialogs. */
export function NeedsYouStuckButtons({
  actions,
  onAction,
  children,
}: {
  actions: readonly StuckAction[]
  onAction?: (action: StuckAction) => void
  children?: ReactNode
}) {
  return (
    <div
      className="mt-2 flex flex-wrap items-center gap-2"
      data-testid={MERGE_TRAIN_TEST_IDS.stuckRow}
      onClick={(event) => event.stopPropagation()}
    >
      {actions.map((id) => (
        <Button key={id} type="button" size="sm" variant="outline" data-action={id} onClick={() => onAction?.(id)}>
          {STUCK_LABELS[id]}
        </Button>
      ))}
      {children}
    </div>
  )
}

type SpecimenRow = NeedsYouRow & { actions?: readonly StuckAction[] }

const SPECIMEN_STUCK = stuckRow(mergeTrain.stuckRow[0]!.input as StuckInput)!

// `cases[1]`'s rows, then ONE stuck PR row (merge-train `stuckRow` cases[0])
// under them: PR rows sit under every decision.
const SPECIMEN_ROWS: SpecimenRow[] = [
  ...(fixture.cases[1]!.expected as unknown as NeedsYouRow[]),
  {
    messageId: `pr12`,
    sessionId: `r2`,
    rootId: `r2`,
    kind: `pr`,
    opens: `results`,
    title: SPECIMEN_STUCK.title,
    chat: `Palette`,
    meta: SPECIMEN_STUCK.meta,
    age: `3h`,
    createdAt: `2027-01-15T05:00:00.000Z`,
    actions: SPECIMEN_STUCK.actions,
  },
]

/** THE styleguide specimen ×4: `cases[1]`'s rows + one stuck row and the
 *  phone entry row. */
export function NeedsYouSpecimen({ width = 560 }: { width?: number }) {
  return (
    <div
      data-testid="needs-you-specimen"
      className="flex flex-col gap-4 rounded-3xl border border-glass-stroke-card bg-background p-5 max-md:rounded-none max-md:border-0 max-md:p-4"
      style={{ width, maxWidth: `100%` }}
    >
      <NeedsYouEntryRow count={SPECIMEN_ROWS.length} />
      <NeedsYouList
        rows={SPECIMEN_ROWS}
        actions={(row) => (row.actions ? <NeedsYouStuckButtons actions={row.actions} /> : null)}
      />
    </div>
  )
}
