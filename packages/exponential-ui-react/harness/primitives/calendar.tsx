// FEED-131: the `calendar` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "calendar";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/calendar.tsx.
// Dates are local midnights and `today` is pinned, so both sides (and every
// run) paint the same month, marker and dropdown years.
import type { ComponentProps, ReactElement } from "react"
import { Calendar, CalendarDayButton } from "../../src/primitives/calendar"

const d = (y: number, m: number, day: number) => new Date(y, m - 1, day)
const TODAY = d(2026, 10, 22)

/** The selected day's button carries `data-xui-target` (the gate's focus case). */
const TargetDayButton = (props: ComponentProps<typeof CalendarDayButton>) => <CalendarDayButton {...props} data-xui-target={props.modifiers.selected ? `` : undefined} />

export const SPECIMENS: Record<string, () => ReactElement> = {
  "calendar/single": () => <Calendar mode="single" selected={d(2026, 10, 15)} defaultMonth={d(2026, 10, 1)} today={TODAY} components={{ DayButton: TargetDayButton }} />,
  "calendar/range": () => <Calendar mode="range" selected={{ from: d(2026, 10, 5), to: d(2026, 10, 12) }} defaultMonth={d(2026, 10, 1)} today={TODAY} />,
  "calendar/multiple-two-months": () => <Calendar mode="multiple" numberOfMonths={2} selected={[d(2026, 10, 2), d(2026, 10, 14), d(2026, 11, 6)]} defaultMonth={d(2026, 10, 1)} today={TODAY} />,
  "calendar/dropdown": () => <Calendar mode="single" captionLayout="dropdown" selected={d(2026, 10, 15)} defaultMonth={d(2026, 10, 1)} today={TODAY} fromYear={2020} toYear={2030} />,
  "calendar/disabled-week-numbers": () => (
    <Calendar mode="single" showWeekNumber disabled={[{ dayOfWeek: [0, 6] }, { before: d(2026, 10, 6) }]} selected={d(2026, 10, 13)} defaultMonth={d(2026, 10, 1)} today={TODAY} />
  ),
  "calendar/rtl": () => (
    <div dir="rtl">
      <Calendar mode="range" dir="rtl" selected={{ from: d(2026, 10, 12), to: d(2026, 10, 16) }} defaultMonth={d(2026, 10, 1)} today={TODAY} />
    </div>
  ),
}
