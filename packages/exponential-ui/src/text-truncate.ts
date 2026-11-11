// #EXP-1778 (fixtures/text-truncate.json): a Text `truncate: "middle"` line,
// the IDE's `text_ellipsis_middle` (gpui's `TruncateMiddle`), as a pure rule
// every renderer replays. Widths are per character (`measure`), summed: the
// line fits whole, else the `…` takes its width off the box and the rest
// splits 2:1, the longest start that fits two thirds, then the longest end
// that fits the remainder. No room for either = the `…` alone, and so is a
// cut keeping fewer than `MIDDLE_MIN_KEPT` characters (#EXP-1789).

/** #EXP-1789: a cut keeping fewer characters than this shows the `…` alone. */
export const MIDDLE_MIN_KEPT = 4

/** The shown line of `text` in `width` px, cut in the middle. */
export function middleTruncateWidth(text: string, width: number, measure: (ch: string) => number, affix = `…`): string {
  // `[...text]` so astral characters (emoji) are never cut in half.
  const chars = [...text]
  const widths = chars.map(measure)
  const total = widths.reduce((a, w) => a + w, 0)
  if (total <= width) return text
  const budget = width - [...affix].reduce((a, ch) => a + measure(ch), 0)
  if (budget <= 0) return affix
  const frontBudget = budget * (2 / 3)
  const backBudget = budget - frontBudget
  let front = 0
  let frontWidth = 0
  while (front < chars.length && frontWidth + widths[front]! <= frontBudget) frontWidth += widths[front++]!
  let back = chars.length
  let backWidth = 0
  while (back > 0 && backWidth + widths[back - 1]! <= backBudget) backWidth += widths[--back]!
  if (front >= back || front + chars.length - back < MIDDLE_MIN_KEPT) return affix
  return `${chars.slice(0, front).join(``)}${affix}${chars.slice(back).join(``)}`
}
