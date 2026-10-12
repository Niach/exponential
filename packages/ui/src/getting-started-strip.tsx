import type { IconConcept } from "@exp/icons"
import spec from "@exp/domain-contract/fixtures/getting-started.json"

import { Button } from "./button"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Meter } from "./meter"

// EXP-1270 (EXP-1259 D6, `getting-started.json`): THE Getting started block's
// face ×4 — a header row (the title, the muted `{done} of {total}`, a
// `meter`-px meter, the Docs button) over ONE row of square cards in the
// fixture's order. A card is `card.width`×`card.height` at radius lg with the
// card hairline: the step icon top-left, a green check (done) or the muted
// step number top-right, the title bottom-left (13px medium, two lines max).
// The NEXT step is highlighted (the hover fill + a foreground hairline at
// 30%); done cards keep their check and stay clickable; no descriptions, no
// locked state, no per-card buttons. The row never wraps: narrower than its
// cards it scrolls sideways (a phone-width browser gets the same strip).
//
// Presentational only: the app derives the entries (the pure
// `deriveGettingStarted`, web `components/getting-started/`) and runs the
// targets; the styleguide draws the same component off the fixture.

export const GETTING_STARTED_SPEC = spec

export type GettingStartedStripKey = (typeof spec.steps)[number][`key`]

export interface GettingStartedStripEntry {
  key: GettingStartedStripKey | (string & {})
  done: boolean
  /** The first step not done: the highlighted card. */
  next: boolean
}

const STEP_ICONS = new Map(
  spec.steps.map((step) => [step.key as string, conceptIcon(step.icon as IconConcept)])
)
const STEP_TITLES = new Map(spec.steps.map((step) => [step.key as string, step.title]))
const DocsIcon = conceptIcon(spec.docs.icon as IconConcept)
const DoneIcon = conceptIcon(`ui-check`)
const CARD = spec.card

export function gettingStartedCardTestId(key: string): string {
  return spec.testIds.card.replace(`{key}`, key)
}

/** One square card. */
export function GettingStartedCard({
  entry,
  number,
  onSelect,
}: {
  entry: GettingStartedStripEntry
  /** The 1-based position in the visible list. */
  number: number
  onSelect?: () => void
}) {
  const Icon = STEP_ICONS.get(entry.key)
  return (
    <button
      type="button"
      data-slot="getting-started-card"
      data-testid={gettingStartedCardTestId(entry.key)}
      data-done={entry.done ? `true` : undefined}
      data-next={entry.next ? `true` : undefined}
      onClick={onSelect}
      className={cn(
        `flex shrink-0 flex-col justify-between rounded-lg border text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/50`,
        entry.next
          ? `border-foreground/30 bg-glass-active`
          : `border-glass-stroke hover:bg-glass-active`
      )}
      style={{ width: CARD.width, height: CARD.height, padding: CARD.padding }}
    >
      <span className="flex items-start justify-between">
        {Icon ? (
          <Icon
            aria-hidden
            style={{ width: CARD.icon, height: CARD.icon }}
            className={entry.done ? `text-muted-foreground` : undefined}
          />
        ) : (
          <span />
        )}
        {entry.done ? (
          <DoneIcon aria-label="Done" className="size-4 text-green-500" />
        ) : (
          <span className="text-xs text-muted-foreground tabular-nums">{number}</span>
        )}
      </span>
      <span
        className={cn(
          `line-clamp-2 text-[13px] leading-snug font-medium`,
          entry.done && `text-muted-foreground`
        )}
      >
        {STEP_TITLES.get(entry.key) ?? entry.key}
      </span>
    </button>
  )
}

/** The header + the strip of cards. */
export function GettingStartedStrip({
  entries,
  done,
  total,
  progress,
  onSelect,
  className,
}: {
  entries: readonly GettingStartedStripEntry[]
  done: number
  total: number
  /** `{done} of {total}` (the model's `progress`). */
  progress: string
  onSelect?: (key: string) => void
  className?: string
}) {
  return (
    <div
      data-slot="getting-started"
      data-testid={spec.testIds.block}
      className={cn(`flex min-w-0 flex-col gap-3`, className)}
    >
      <header className="flex min-w-0 items-center gap-3">
        <h2 className="shrink-0 text-sm font-medium">{spec.title}</h2>
        <span className="shrink-0 text-sm text-muted-foreground tabular-nums">{progress}</span>
        <Meter
          value={total > 0 ? (done / total) * 100 : 0}
          className="h-1 min-w-0 shrink"
          style={{ width: spec.meter }}
        />
        <Button variant="outline" size="sm" className="ml-auto shrink-0" asChild>
          <a
            href={spec.docs.url}
            target="_blank"
            rel="noreferrer"
            data-testid={spec.testIds.docs}
          >
            <DocsIcon className="size-4" />
            {spec.docs.label}
          </a>
        </Button>
      </header>
      <div className="-mx-1 flex overflow-x-auto px-1 pb-1" style={{ gap: CARD.gap }}>
        {entries.map((entry, index) => (
          <GettingStartedCard
            key={entry.key}
            entry={entry}
            number={index + 1}
            onSelect={onSelect ? () => onSelect(entry.key) : undefined}
          />
        ))}
      </div>
    </div>
  )
}
