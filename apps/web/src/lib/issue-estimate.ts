// EXP-630: story points. ONE integer per issue, no per-team scale: the picker
// offers the fibonacci ladder most teams use, and any other value (an
// imported linear or t-shirt estimate, a value set over the API) still
// renders as its number and stays selectable until it is changed.
export const ISSUE_ESTIMATE_LADDER = [1, 2, 3, 5, 8, 13, 21] as const

/** "No estimate", "1 point", "5 points". */
export function estimateLabel(value: number | null | undefined): string {
  if (value === null || value === undefined) return `No estimate`
  return value === 1 ? `1 point` : `${value} points`
}

/** The short form for chips: "5 pt" / "1 pt". */
export function estimateShortLabel(value: number): string {
  return `${value} pt`
}

/**
 * Picker options: "None" first, then the ladder, plus the current value when
 * it sits off the ladder (so the trigger always names a listed option).
 * Values are strings (the picker speaks strings); `""` = clear.
 */
export function estimatePickerOptions(
  current: number | null | undefined
): { value: string; label: string }[] {
  const values = new Set<number>(ISSUE_ESTIMATE_LADDER)
  if (typeof current === `number` && current >= 0) values.add(current)
  return [
    { value: ``, label: `No estimate` },
    ...[...values]
      .sort((left, right) => left - right)
      .map((value) => ({ value: String(value), label: estimateLabel(value) })),
  ]
}

/** The picker's string back to the wire value (`""` → null). */
export function parseEstimatePick(value: string | null | undefined): number | null {
  if (!value) return null
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null
}
