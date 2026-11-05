import { type ComponentProps } from "react"
import { CHATS_SHELL } from "@exp/domain-contract/chats"
import { NEEDS_YOU_COPY, NEEDS_YOU_TEST_IDS, type NeedsYouRow } from "@exp/domain-contract/needs-you"
import fixture from "@exp/domain-contract/fixtures/needs-you.json"
import { WAITING_CARDS_ICONS } from "@exp/domain-contract/waiting-cards"
import { Badge } from "./badge"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { ListRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// #FEED-191 NEEDS YOU ×web (fixtures/needs-you.json; desktop domain::needs_you,
// iOS ExpCore NeedsYou.swift, Android domain/NeedsYou.kt): the model is the
// contract's `needsYou()`; these only paint it.
//
//   NeedsYouList      one reading column of THE inbox page's ListRow, else
//                     THE EmptyState
//   NeedsYouEntryRow  the phones' first row of Chats (the Archived row's box)
//   NeedsYouSpecimen  the styleguide's `needs-you`: `cases[1]` + the entry row

const NEEDS_YOU_TONE = `text-yellow-400`
const EntryIcon = conceptIcon(`nav-needs-you`)
const PlanIcon = conceptIcon(WAITING_CARDS_ICONS.plan as Parameters<typeof conceptIcon>[0])
const QuestionIcon = conceptIcon(WAITING_CARDS_ICONS.question as Parameters<typeof conceptIcon>[0])
const PrIcon = conceptIcon(`pr-open`)
const GLYPHS = { plan: PlanIcon, question: QuestionIcon, pr: PrIcon } as const

export function NeedsYouList({
  rows,
  onOpen,
  className,
}: {
  rows: readonly NeedsYouRow[]
  onOpen?: (row: NeedsYouRow) => void
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
                {row.meta ? `${row.chat} · ${row.meta}` : row.chat}
              </div>
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
        <Badge count={count} className="bg-yellow-400 text-black" />
      </span>
    </button>
  )
}

const SPECIMEN_ROWS = fixture.cases[1]!.expected as unknown as NeedsYouRow[]

/** THE styleguide specimen ×4: `cases[1]`'s rows and the phone entry row. */
export function NeedsYouSpecimen({ width = 560 }: { width?: number }) {
  return (
    <div
      data-testid="needs-you-specimen"
      className="flex flex-col gap-4 rounded-3xl border border-glass-stroke-card bg-background p-5 max-md:rounded-none max-md:border-0 max-md:p-4"
      style={{ width, maxWidth: `100%` }}
    >
      <NeedsYouEntryRow count={SPECIMEN_ROWS.length} />
      <NeedsYouList rows={SPECIMEN_ROWS} />
    </div>
  )
}
