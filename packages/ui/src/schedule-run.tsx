import { useState } from "react"
import { conceptIcon } from "./icons.generated"
import {
  Trace,
  TraceCollapsible,
  TraceCollapsibleContent,
  TraceCollapsibleTrigger,
  TraceIcon,
  TraceLine,
  TraceTitle,
} from "./trace"

const ScheduleIcon = conceptIcon(`trigger-schedule`)

/** #FEED-317 (transcript-grouping.json `checkInCases`): quiet schedule ticks
 *  as THE tool-group row: one Trace sm line (`Schedule ×5 · 11:15–12:15`)
 *  opening to a line per tick; a single tick never opens. */
export function ScheduleRunRow({ title, ticks }: { title: string; ticks: readonly string[] }) {
  const [open, setOpen] = useState(false)
  const line = (label: string) => (
    <TraceLine>
      <TraceIcon>
        <ScheduleIcon />
      </TraceIcon>
      <TraceTitle title={label}>{label}</TraceTitle>
    </TraceLine>
  )
  if (ticks.length < 2) {
    return (
      <Trace size="sm" data-testid="session-schedule-run">
        {line(title)}
      </Trace>
    )
  }
  return (
    <Trace size="sm" data-testid="session-schedule-run">
      <TraceCollapsible open={open} onOpenChange={setOpen}>
        <TraceCollapsibleTrigger>{line(title)}</TraceCollapsibleTrigger>
        <TraceCollapsibleContent>
          <div className="flex flex-col gap-1.5">
            {ticks.map((tick, index) => (
              <Trace key={index} size="sm">
                {line(tick)}
              </Trace>
            ))}
          </div>
        </TraceCollapsibleContent>
      </TraceCollapsible>
    </Trace>
  )
}
