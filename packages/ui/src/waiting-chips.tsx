import { useState } from "react"
import { ASK_DOCK_COPY, askDockTitle } from "@exp/domain-contract/ask-dock"
import {
  WAITING_CARDS_COPY,
  WAITING_CARDS_TEST_IDS,
  waitingChips,
  waitingCountLabel,
  type WaitingCardInput,
  type WaitingChip,
} from "@exp/domain-contract/waiting-cards"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Menu, MenuRow } from "./menu"
import { StripChip } from "./strip-chip"

const WaitingPlanIcon = conceptIcon(`coding-plan`)
const WaitingQuestionIcon = conceptIcon(`ui-help`)

/** The kind icon in the ask-dock tint (a plan primary, a question yellow). */
export function WaitingKindIcon({ kind, className }: { kind: WaitingChip[`kind`]; className?: string }) {
  return kind === `plan` ? (
    <WaitingPlanIcon className={cn(`size-3.5 shrink-0 text-primary`, className)} />
  ) : (
    <WaitingQuestionIcon className={cn(`size-3.5 shrink-0 text-yellow-400`, className)} />
  )
}

/** #EXP-1562 / #EXP-1785 (waiting-cards.json CHIPS): the tree's WAITING
 *  plans and questions at the start of the session screen header's trailing
 *  slot: wide = THE StripChip per card (the tinted kind icon, the clipped
 *  title); phones = ONE icon chip with the count opening THE Menu (the
 *  bottom sheet) of them. None waiting = nothing. */
export function ChatWaitingChips({
  cards,
  phone,
  onSelect,
  menuOpen,
}: {
  cards: readonly WaitingCardInput[]
  phone: boolean
  onSelect: (id: string) => void
  /** Holds the phone sheet open (a static paint). */
  menuOpen?: boolean
}) {
  const [shown, setShown] = useState(false)
  const { layout, count, chips } = waitingChips(cards, phone)
  if (layout === `hidden`) return null
  const fullTitle = (id: string) => {
    const card = cards.find((entry) => entry.id === id)
    return card ? askDockTitle(card.text) || undefined : undefined
  }
  if (layout === `chips`) {
    return (
      <div data-slot="chat-waiting-chips" className="flex min-w-0 shrink items-center gap-1 overflow-hidden">
        {chips.map((chip) => (
          <StripChip
            key={chip.id}
            data-testid={WAITING_CARDS_TEST_IDS.chip}
            data-kind={chip.kind}
            boxClassName="min-w-0 shrink"
            tooltip={fullTitle(chip.id)}
            lead={<WaitingKindIcon kind={chip.kind} />}
            label={<span className="block truncate">{chip.label}</span>}
            onClick={() => onSelect(chip.id)}
          />
        ))}
      </div>
    )
  }
  const label = waitingCountLabel(count)
  return (
    <Menu
      open={menuOpen ?? shown}
      onOpenChange={setShown}
      align="end"
      title={WAITING_CARDS_COPY.waiting}
      aria-label={WAITING_CARDS_COPY.waiting}
      data-testid={WAITING_CARDS_TEST_IDS.menu}
      contentClassName="w-80 p-1"
      trigger={
        <StripChip
          data-testid={WAITING_CARDS_TEST_IDS.button}
          aria-label={label}
          tooltip={label}
          boxClassName="shrink-0"
          className="px-2"
          lead={<WaitingQuestionIcon className="size-3.5 text-yellow-400" />}
          label={<span className="text-muted-foreground tabular-nums">{count}</span>}
        />
      }
      body={
        <div data-slot="chat-waiting-rows" className="flex flex-col">
          {chips.map((chip) => (
            <MenuRow
              key={chip.id}
              data-testid={`${WAITING_CARDS_TEST_IDS.menu}-row`}
              onSelect={() => {
                setShown(false)
                onSelect(chip.id)
              }}
              className="gap-2"
            >
              <WaitingKindIcon kind={chip.kind} />
              <span className="min-w-0 flex-1 truncate">{chip.label}</span>
              <span className="shrink-0 text-xs text-muted-foreground">
                {chip.kind === `plan` ? ASK_DOCK_COPY.planReady : ASK_DOCK_COPY.question}
              </span>
            </MenuRow>
          ))}
        </div>
      }
    />
  )
}
