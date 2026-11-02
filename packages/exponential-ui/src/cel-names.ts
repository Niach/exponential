// VAPP-191: the expression functions' A2UI names (src/cel.ts), on their own
// so the prompt builds without the evaluator.

/** The functions the A2UI form adds to the bind table (`size` = the
 *  existing `len`; `expr` = the string form, `{expr, vars}`). */
export const EXPRESSION_CALL_NAMES: readonly string[] = [`mul`, `div`, `mod`, `round`, `floor`, `ceil`, `abs`, `min`, `max`, `today`, `now`, `dateAdd`, `dateDiff`, `startOf`, `endOf`, `weekday`, `isoWeek`, `hours`, `formatDuration`, `expr`]
