import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS } from "@exp/domain-contract/chats"
import { SegmentedControl } from "@exp/ui"
import { cn } from "@/lib/utils"

// Chat 58cb32df + #EXP-1836 (chats.json `shell.boardChips`, ×4): the board's
// tabs under its header — All issues · My issues N · Drafts N as THE
// session screen's SegmentedControl (`SessionScreenTabs`), left-aligned,
// each count after its label in muted text; Drafts only while the viewer has
// drafts (phones). My issues = mine on THIS board (`boardMineIssues`).

export type BoardChip = `all` | `mine` | `drafts`

/** The tabs a viewer sees (Drafts only while they have drafts). */
export function boardChips(draftCount: number): BoardChip[] {
  return (CHATS_SHELL.boardChips as BoardChip[]).filter(
    (chip) => chip !== `drafts` || draftCount > 0
  )
}

/** A tab's count (My issues, Drafts); none for All issues or zero. */
export function boardChipCount(chip: BoardChip, counts: { mine: number; drafts: number }): number | null {
  if (chip === `all`) return null
  const n = counts[chip]
  return n > 0 ? n : null
}

export function BoardChips({
  value,
  counts,
  onChange,
  className,
}: {
  value: BoardChip
  counts: { mine: number; drafts: number }
  onChange: (chip: BoardChip) => void
  className?: string
}) {
  return (
    <div
      aria-label="Board filter"
      className={cn(`overflow-x-auto px-4 pb-2 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden`, className)}
    >
      <SegmentedControl
        value={value}
        onValueChange={onChange}
        className="w-max"
        options={boardChips(counts.drafts).map((chip) => {
          const count = boardChipCount(chip, counts)
          return {
            value: chip,
            label: (
              <span
                className="flex items-center gap-1.5 whitespace-nowrap"
                data-testid={CHATS_TEST_IDS.boardChip}
                data-chip={chip}
              >
                {CHATS_COPY.boardChips[chip]}
                {count !== null && <span className="text-muted-foreground tabular-nums">{count}</span>}
              </span>
            ),
          }
        })}
      />
    </div>
  )
}
