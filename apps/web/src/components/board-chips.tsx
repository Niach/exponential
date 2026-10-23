import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS } from "@exp/domain-contract/chats"
import { Pill } from "@exp/ui"
import { cn } from "@/lib/utils"

// Thread 58cb32df (chats.json `shell.boardChips`, ×3 with iOS and Android):
// the phone board's chips under its header — All issues · My issues N ·
// Drafts N as select Pills; Drafts only while the viewer has drafts, a
// count after My issues and Drafts. My issues and Drafts moved here from the
// Inbox's segments.

export type BoardChip = `all` | `mine` | `drafts`

/** The chips a viewer sees (Drafts only while they have drafts). */
export function boardChips(draftCount: number): BoardChip[] {
  return (CHATS_SHELL.boardChips as BoardChip[]).filter(
    (chip) => chip !== `drafts` || draftCount > 0
  )
}

/** A chip's label: its copy, then the count (My issues, Drafts). */
export function boardChipLabel(chip: BoardChip, counts: { mine: number; drafts: number }): string {
  const label = CHATS_COPY.boardChips[chip]
  if (chip === `all`) return label
  const n = counts[chip]
  return n > 0 ? `${label} ${n}` : label
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
      role="toolbar"
      aria-label="Board filter"
      className={cn(`flex gap-2 overflow-x-auto px-4 pb-2 [scrollbar-width:none]`, className)}
    >
      {boardChips(counts.drafts).map((chip) => (
        <Pill
          key={chip}
          mode="select"
          size="md"
          selected={chip === value}
          data-testid={CHATS_TEST_IDS.boardChip}
          data-chip={chip}
          className="h-9 px-4"
          onClick={() => onChange(chip)}
        >
          {boardChipLabel(chip, counts)}
        </Pill>
      ))}
    </div>
  )
}
