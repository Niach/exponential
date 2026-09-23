// EXP-630: story points, rendered on the TEAM's scale (`teams.estimation_type`,
// contract `issueEstimation`). The stored value is always a point number, so
// a team can switch scales without touching a single issue; `none` hides the
// chip everywhere. T-shirt sizes are the fibonacci points worn as XS…XL,
// exactly how Linear stores them.
import {
  ISSUE_ESTIMATE_TSHIRT_LABELS,
  ISSUE_ESTIMATION_SCALES,
  type IssueEstimation,
} from "@/lib/domain"

export type EstimationScale = Exclude<IssueEstimation, `none`>

/** The settings picker rows, in Linear's order and wording. */
export const ESTIMATION_TYPE_OPTIONS: { value: IssueEstimation; label: string; hint: string }[] = [
  { value: `none`, label: `Not in use`, hint: `` },
  { value: `exponential`, label: `Exponential`, hint: `1, 2, 4, 8, 16 points` },
  { value: `fibonacci`, label: `Fibonacci`, hint: `1, 2, 3, 5, 8 points` },
  { value: `linear`, label: `Linear`, hint: `1, 2, 3, 4, 5 points` },
  { value: `tshirt`, label: `T-shirt`, hint: `XS, S, M, L, XL` },
]

export function estimationScale(type: IssueEstimation): readonly number[] {
  return type === `none` ? [] : ISSUE_ESTIMATION_SCALES[type]
}

function tshirtLabel(value: number): string | null {
  const index = ISSUE_ESTIMATION_SCALES.tshirt.indexOf(value)
  return index === -1 ? null : ISSUE_ESTIMATE_TSHIRT_LABELS[index]!
}

/** "No estimate", "M", "1 point", "5 points". */
export function estimateLabel(value: number | null | undefined, type: IssueEstimation = `fibonacci`): string {
  if (value === null || value === undefined) return `No estimate`
  if (type === `tshirt`) {
    const size = tshirtLabel(value)
    if (size) return size
  }
  return value === 1 ? `1 point` : `${value} points`
}

/** The chip form: "M" on the t-shirt scale, "5 pt" elsewhere. */
export function estimateShortLabel(value: number, type: IssueEstimation = `fibonacci`): string {
  if (type === `tshirt`) {
    const size = tshirtLabel(value)
    if (size) return size
  }
  return `${value} pt`
}

/**
 * Picker options: "No estimate" first, then the scale's ladder, plus the
 * current value when it sits off the ladder (an import from another scale),
 * so the trigger always names a listed option. Values are strings (the
 * picker's currency); `""` = clear.
 */
export function estimatePickerOptions(
  current: number | null | undefined,
  type: IssueEstimation
): { value: string; label: string }[] {
  const values = new Set<number>(estimationScale(type))
  if (typeof current === `number` && current >= 0) values.add(current)
  return [
    { value: ``, label: `No estimate` },
    ...[...values]
      .sort((left, right) => left - right)
      .map((value) => ({ value: String(value), label: estimateLabel(value, type) })),
  ]
}

/** The picker's string back to the wire value (`""` → null). */
export function parseEstimatePick(value: string | null | undefined): number | null {
  if (!value) return null
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null
}
