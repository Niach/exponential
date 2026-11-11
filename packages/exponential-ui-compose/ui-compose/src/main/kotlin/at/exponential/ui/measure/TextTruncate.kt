package at.exponential.ui.measure

/**
 * #EXP-1778 (`fixtures/text-truncate.json`): a Text `truncate: "middle"`
 * line (start…end), the rule gpui's `text_ellipsis_middle` applies (the TS
 * twin is packages/exponential-ui/src/text-truncate.ts). Per-character widths
 * are summed; a line that fits shows whole, else the `…` takes its width off
 * the box and the rest splits 2:1 between the longest start and the longest
 * end that fit; no room for either = the `…` alone. Characters = code points.
 */
fun middleTruncateWidth(text: String, width: Double, affix: String = "…", measure: (String) -> Double): String {
    val chars = text.codePoints().toArray().map { String(Character.toChars(it)) }
    val widths = chars.map(measure)
    if (widths.sum() <= width) return text
    val affixWidth = affix.codePoints().toArray().sumOf { measure(String(Character.toChars(it))) }
    val budget = width - affixWidth
    if (budget <= 0.0) return affix
    val frontBudget = budget * (2.0 / 3.0)
    val backBudget = budget - frontBudget
    var front = 0
    var frontWidth = 0.0
    while (front < chars.size && frontWidth + widths[front] <= frontBudget) frontWidth += widths[front++]
    var back = chars.size
    var backWidth = 0.0
    while (back > 0 && backWidth + widths[back - 1] <= backBudget) backWidth += widths[--back]
    if (front >= back) return affix
    return chars.subList(0, front).joinToString("") + affix + chars.subList(back, chars.size).joinToString("")
}
