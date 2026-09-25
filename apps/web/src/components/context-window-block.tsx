// EXP-1051: WHERE a run's context window went — the usage overlay's first
// block, and the only place the window is more than one number.
//
// Collapsed it is the old "Context" line with a real bar under it: the
// headline (`65k / 200k (32%)`), the run's cost, and the stacked
// `SegmentedBar` whose slices are the launcher's own accounting. Expanded it
// is the LEGEND — one row per layer, its tokens and its share, `Free` last —
// because the question a full window raises ("what is even in there?") has no
// answer in a percentage.
//
// The folding, the numbers and every string come from `lib/context-layout.ts`
// (fixture-locked ×4); this file only draws them, and adds the two rows that
// are links rather than facts: the playbook every run is given, and the team
// prompt an owner can shorten.
import { useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  Collapsible,
  CollapsibleContent,
  DisclosureHeader,
  SegmentedBar,
  segmentToneClass,
} from "@exp/ui"
import { useTeamById } from "@/hooks/use-team-data"
import {
  CONTEXT_WINDOW_TITLE,
  contextWindowView,
  type ContextLegendRow,
  type ContextSegment,
} from "@/lib/context-layout"
import type { SessionUsageState } from "@/lib/agent-feed"
import { cn } from "@/lib/utils"

/** The overlay's section-title style — the same 11px caps the account and
 *  windows sections carry. */
const TITLE_CLASS = `text-[11px] uppercase tracking-wide text-muted-foreground`

/** The legend row's inside, shared by the static rows and the one that acts. */
function LegendRowBody({ row }: { row: ContextLegendRow }) {
  return (
    <>
      <span
        aria-hidden
        className={cn(`size-2.5 shrink-0 rounded-[3px]`, segmentToneClass(row.tone))}
      />
      <span className="min-w-0 shrink-0 truncate">{row.label}</span>
      {row.detail && (
        <span
          className="min-w-0 truncate text-[11px] text-muted-foreground/70"
          title={row.detail}
        >
          {row.detail}
        </span>
      )}
      <span className="ml-auto shrink-0 tabular-nums text-muted-foreground">
        {row.estimated ? `≈` : ``}
        {row.tokens}
      </span>
      <span className="w-11 shrink-0 text-right font-medium tabular-nums text-foreground">
        {row.percent}
      </span>
    </>
  )
}

const ROW_CLASS = `flex w-full min-w-0 items-center gap-2 rounded-sm px-1 py-1 text-left text-xs text-foreground/90`
const ACTION_ROW_CLASS = `cursor-pointer outline-none transition-colors duration-fast hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50`

export function ContextWindowBlock({
  sessionUsage,
  contextLayout,
  teamId,
  cost = null,
  className,
}: {
  sessionUsage: SessionUsageState | null
  /** The device's `context_layout` state, or null while it sent none — the
   *  bar then has one slice (the conversation) and the legend two rows. */
  contextLayout: ContextSegment[] | null
  /** The run's team, for the team-prompt row's link. */
  teamId: string | null | undefined
  /** `formatUsageCost` — kept visible beside the headline (EXP-1051 replaced
   *  the section that used to carry it). */
  cost?: string | null
  className?: string
}) {
  const [open, setOpen] = useState(false)
  const team = useTeamById(teamId)
  const view = contextWindowView(sessionUsage, contextLayout)
  // No usage yet, or a window of unknown size: there is no scale to draw on.
  if (!view) return null

  return (
    <div
      data-testid="context-window-block"
      className={cn(`space-y-1.5 px-3 py-2.5`, className)}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        {/* Trailing chevron: the numbers are what the eye goes to, and the
            fold indicator parks after them (the reference row reads
            `Context window … 65k / 200k (32%) ⌄`). */}
        <DisclosureHeader
          open={open}
          onToggle={() => setOpen((prev) => !prev)}
          chevron="trailing"
          data-testid="context-window-toggle"
        >
          <span className={TITLE_CLASS}>{CONTEXT_WINDOW_TITLE}</span>
          <span className="min-w-0 flex-1" />
          <span className="shrink-0 tabular-nums text-xs text-foreground">
            {view.headline}
          </span>
          {cost && <span className="shrink-0 text-[11px]">{cost}</span>}
        </DisclosureHeader>
        <SegmentedBar
          segments={view.bar}
          ticks={view.ticks}
          className="mt-1.5"
          data-testid="context-window-bar"
        />
        <CollapsibleContent className="mt-1.5 flex flex-col">
          {view.legend.map((row) => {
            const body = <LegendRowBody row={row} />
            // The team prompt: the one layer in here a person can shorten,
            // and Settings → General is where they do it.
            if (row.key === `team` && team) {
              return (
                <Link
                  key={row.key}
                  to="/t/$teamSlug/settings/general"
                  params={{ teamSlug: team.slug }}
                  data-testid="context-legend-row"
                  data-key={row.key}
                  className={cn(ROW_CLASS, ACTION_ROW_CLASS)}
                >
                  {body}
                </Link>
              )
            }
            return (
              <div
                key={row.key}
                data-testid="context-legend-row"
                data-key={row.key}
                className={ROW_CLASS}
              >
                {body}
              </div>
            )
          })}
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}
