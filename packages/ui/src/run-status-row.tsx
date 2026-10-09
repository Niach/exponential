import { AgentRunMark, type RunMarkState } from "./agent-brand-mark"
import { Button } from "./button"
import { cn } from "./cn"

// EXP-1175: the Run face's ONE status row, over the results thread or the
// full transcript — the run mark as the spinner, the caption in its tone,
// the last tool line muted under it (live runs only), and the Show work /
// Hide work toggle on the right. Label + state only. The app owns the rules
// (`runRowCaption`, `lastToolLine`, `showWorkLabel` in apps/web; fixture
// `run-row.json` ×4); this component only draws them. EXP-1245: the owner's
// thread draws one per TURN (`turnRowCaption`); a row without `onToggle`
// carries no Show work button.

export type RunStatusRowTone = `muted` | `amber` | `emerald` | `sky`

export const RUN_STATUS_ROW_TONE_CLASS: Record<RunStatusRowTone, string> = {
  muted: `text-muted-foreground`,
  amber: `text-amber-400`,
  emerald: `text-emerald-400`,
  sky: `text-sky-400`,
}

export function RunStatusRow({
  agent,
  markState,
  caption,
  tone,
  toolLine,
  showWork = false,
  toggleLabel,
  onToggle,
  className,
}: {
  agent: string | null | undefined
  markState?: RunMarkState
  caption: string
  tone: RunStatusRowTone
  /** The newest tool call's line; null = no second line. */
  toolLine?: string | null
  showWork?: boolean
  /** What pressing the toggle does next (`showWorkLabel(showWork)`). */
  toggleLabel?: string
  /** Absent = no toggle (a settled turn's row). */
  onToggle?: () => void
  className?: string
}) {
  return (
    <div
      data-testid="run-status-row"
      data-show-work={showWork}
      className={cn(`flex items-center gap-2.5 py-1.5`, className)}
    >
      <AgentRunMark
        agent={agent}
        state={markState}
        ringClassName="ring-background"
      />
      <div className="min-w-0 flex-1">
        <div
          data-slot="run-status-caption"
          className={cn(`truncate text-xs`, RUN_STATUS_ROW_TONE_CLASS[tone])}
        >
          {caption}
        </div>
        {toolLine ? (
          <div
            data-slot="run-status-tool"
            className="truncate text-[0.6875rem] text-muted-foreground/70"
          >
            {toolLine}
          </div>
        ) : null}
      </div>
      {onToggle && toggleLabel ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 shrink-0 text-xs"
          aria-pressed={showWork}
          onClick={onToggle}
        >
          {toggleLabel}
        </Button>
      ) : null}
    </div>
  )
}
