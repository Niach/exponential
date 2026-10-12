import type { IconConcept } from "@exp/icons"
import spec from "@exp/domain-contract/fixtures/getting-started.json"

import { Button } from "./button"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Meter } from "./meter"

// EXP-1270 (EXP-1259 D6, `getting-started.json`, the #EXP-1270 redesign):
// THE Getting started block's face ×4 — a vertical path in the list
// language. The group band (the title, the muted `{done} of {total}`, a
// `meter`-px meter, the Docs button; phones: Docs as an icon button) over
// flat hairline-divided rows in the fixture's order, one per entry:
// - DONE: the icon and title muted, a green check trailing; still clickable.
// - NEXT (the ONE expanded row): the active fill with a hairline top and
//   bottom, the icon + the title (medium) + a small `nextLabel` trailing,
//   under the title the step's `why` (13px muted) and ONE primary button
//   labelled `cta`; the row and the button both hand the key back.
// - LATER: plain, the foreground title, a quiet chevron trailing (muted 45%).
// Rows are at least `row.height` px (phones `row.heightPhone`); the block is
// at most `maxWidth` wide and scrolls vertically only, never sideways.
//
// Presentational only: the app derives the entries (the pure
// `deriveGettingStarted`, web `components/getting-started/`) and runs the
// targets; the styleguide draws the same component off the fixture.

export const GETTING_STARTED_SPEC = spec

export type GettingStartedListKey = (typeof spec.steps)[number][`key`]

export interface GettingStartedListEntry {
  key: GettingStartedListKey | (string & {})
  done: boolean
  /** The first step not done: the expanded row. */
  next: boolean
}

type Step = (typeof spec.steps)[number]

const STEPS = new Map<string, Step>(spec.steps.map((step) => [step.key, step]))
const STEP_ICONS = new Map(
  spec.steps.map((step) => [step.key as string, conceptIcon(step.icon as IconConcept)])
)
const DocsIcon = conceptIcon(spec.docs.icon as IconConcept)
const DoneIcon = conceptIcon(`ui-check`)
const LaterIcon = conceptIcon(`ui-chevron-right`)
const ROW = spec.row

export function gettingStartedRowTestId(key: string): string {
  return spec.testIds.row.replace(`{key}`, key)
}

const ROW_CLASS = `flex w-full items-center gap-3 text-left text-sm outline-none transition-colors duration-fast focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50`

/** One step's row: done, the expanded next one, or a later one. */
export function GettingStartedRow({
  entry,
  phone = false,
  onSelect,
}: {
  entry: GettingStartedListEntry
  /** The phone density (`row.heightPhone`). */
  phone?: boolean
  onSelect?: () => void
}) {
  const step = STEPS.get(entry.key)
  const Icon = STEP_ICONS.get(entry.key)
  const title = step?.title ?? entry.key
  const minHeight = phone ? ROW.heightPhone : ROW.height
  const icon = Icon ? (
    <Icon
      aria-hidden
      className={cn(`shrink-0`, entry.done && `text-muted-foreground`)}
      style={{ width: ROW.iconSize, height: ROW.iconSize }}
    />
  ) : null

  if (entry.next && !entry.done) {
    return (
      <div
        data-slot="getting-started-row"
        data-testid={gettingStartedRowTestId(entry.key)}
        data-next="true"
        onClick={onSelect}
        // The list's dividers (and the band's hairline over a first row)
        // are its hairline top and bottom.
        className="flex w-full cursor-pointer gap-3 bg-glass-active text-sm"
        style={{
          minHeight,
          paddingLeft: ROW.padding,
          paddingRight: ROW.padding,
          paddingTop: ROW.expandedPadding,
          paddingBottom: ROW.expandedPadding,
        }}
      >
        <span className="flex h-5 items-center">{icon}</span>
        <span className="flex min-w-0 flex-1 flex-col gap-2.5">
          <span className="flex min-w-0 flex-col gap-1">
            <span className="flex min-w-0 items-center gap-3">
              <span className="min-w-0 flex-1 truncate font-medium">{title}</span>
              <span
                data-part="next"
                className="shrink-0 rounded-sm bg-glass-section px-1.5 py-0.5 text-xs text-muted-foreground"
              >
                {spec.nextLabel}
              </span>
            </span>
            {step?.why && (
              <span className="text-[13px] text-muted-foreground">{step.why}</span>
            )}
          </span>
          {step?.cta && (
            <span>
              <Button
                size="sm"
                data-testid={spec.testIds.cta}
                onClick={(event) => {
                  event.stopPropagation()
                  onSelect?.()
                }}
              >
                {step.cta}
              </Button>
            </span>
          )}
        </span>
      </div>
    )
  }

  return (
    <button
      type="button"
      data-slot="getting-started-row"
      data-testid={gettingStartedRowTestId(entry.key)}
      data-done={entry.done ? `true` : undefined}
      onClick={onSelect}
      className={cn(ROW_CLASS, `hover:bg-glass-row`)}
      style={{ minHeight, paddingLeft: ROW.padding, paddingRight: ROW.padding }}
    >
      {icon}
      <span
        className={cn(`min-w-0 flex-1 truncate`, entry.done && `text-muted-foreground`)}
      >
        {title}
      </span>
      {entry.done ? (
        <DoneIcon aria-label="Done" className="size-4 shrink-0 text-green-500" />
      ) : (
        <LaterIcon aria-hidden className="size-4 shrink-0 text-muted-foreground/45" />
      )}
    </button>
  )
}

/** The group band + the rows. */
export function GettingStartedList({
  entries,
  done,
  total,
  progress,
  onSelect,
  phone = false,
  framed = true,
  className,
}: {
  entries: readonly GettingStartedListEntry[]
  done: number
  total: number
  /** `{done} of {total}` (the model's `progress`). */
  progress: string
  onSelect?: (key: string) => void
  /** Phones: the taller rows, Docs as an icon button, full width. */
  phone?: boolean
  /** The block's own hairline box and `maxWidth`; off where a surface
   *  (the dialog, the phone sheet) already is the box. */
  framed?: boolean
  className?: string
}) {
  return (
    <div
      data-slot="getting-started"
      data-testid={spec.testIds.block}
      className={cn(
        `flex w-full min-w-0 flex-col overflow-x-hidden`,
        framed && `rounded-xl border border-glass-stroke bg-glass-card`,
        className
      )}
      style={framed && !phone ? { maxWidth: spec.maxWidth } : undefined}
    >
      <header
        data-part="band"
        className="flex min-w-0 items-center gap-3 border-b border-glass-stroke bg-glass-section py-2.5"
        style={{ paddingLeft: ROW.padding, paddingRight: ROW.padding }}
      >
        <h2 className="shrink-0 text-sm font-medium">{spec.title}</h2>
        <span className="shrink-0 text-sm text-muted-foreground tabular-nums">{progress}</span>
        <Meter
          value={total > 0 ? (done / total) * 100 : 0}
          className="h-1 min-w-0 shrink"
          style={{ width: spec.meter }}
        />
        {phone ? (
          <Button variant="ghost" size="icon-sm" className="ml-auto shrink-0" asChild>
            <a
              href={spec.docs.url}
              target="_blank"
              rel="noreferrer"
              aria-label={spec.docs.label}
              data-testid={spec.testIds.docs}
            >
              <DocsIcon className="size-4" />
            </a>
          </Button>
        ) : (
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
        )}
      </header>
      <div data-part="rows" className="flex flex-col divide-y divide-glass-stroke">
        {entries.map((entry) => (
          <GettingStartedRow
            key={entry.key}
            entry={entry}
            phone={phone}
            onSelect={onSelect ? () => onSelect(entry.key) : undefined}
          />
        ))}
      </div>
    </div>
  )
}
