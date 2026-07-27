import { addDays, startOfDay } from "date-fns"
import { formatDateForMutation } from "@/lib/domain"

export interface DueDatePreset {
  date: Date
  id: `tomorrow` | `end_of_week` | `in_one_week`
  label: string
}

export function getEndOfWorkWeek(baseDate = new Date()) {
  const day = baseDate.getDay()
  const daysUntilFriday = (5 - day + 7) % 7
  return startOfDay(addDays(baseDate, daysUntilFriday))
}

export function getDueDatePresets(baseDate = new Date()): DueDatePreset[] {
  return [
    {
      id: `tomorrow`,
      label: `Tomorrow`,
      date: startOfDay(addDays(baseDate, 1)),
    },
    {
      id: `end_of_week`,
      label: `End of this week`,
      date: getEndOfWorkWeek(baseDate),
    },
    {
      id: `in_one_week`,
      label: `In one week`,
      date: startOfDay(addDays(baseDate, 7)),
    },
  ]
}

export function formatDueDateMenuMeta(date: Date) {
  const parts = new Intl.DateTimeFormat(`en-GB`, {
    day: `numeric`,
    month: `short`,
    weekday: `short`,
  }).formatToParts(date)
  const weekday = parts.find((part) => part.type === `weekday`)?.value ?? ``
  const day = parts.find((part) => part.type === `day`)?.value ?? ``
  const month = parts.find((part) => part.type === `month`)?.value ?? ``

  return `${weekday}, ${day} ${month}`.trim()
}

/**
 * Due-date urgency (REV2-48) — the rule iOS and Android already ship
 * (`dueDateColor`): due TODAY wins over overdue, then overdue, else muted.
 * The boundary is the device-LOCAL `YYYY-MM-DD` the EXP-38 overdue-first
 * comparator sorts on (`lib/board-view.ts`), so the color explains the order.
 */
export type DueDateTone = `overdue` | `today` | `upcoming`

export function dueDateTone(
  dueDate: string,
  today: string = formatDateForMutation(new Date())!
): DueDateTone {
  if (dueDate === today) return `today`
  return dueDate < today ? `overdue` : `upcoming`
}

const dueDateToneClasses: Record<DueDateTone, string> = {
  overdue: `text-red-500`,
  today: `text-orange-500`,
  upcoming: `text-muted-foreground`,
}

/** Tailwind text color for a due date — the shared list/cell treatment. */
export function dueDateToneClass(dueDate: string, today?: string): string {
  return dueDateToneClasses[dueDateTone(dueDate, today)]
}

export function matchesDueDateValue(
  date: Date | null | undefined,
  dueDate: string | null
) {
  if (!date || !dueDate) {
    return false
  }

  return formatDateForMutation(date) === dueDate
}
