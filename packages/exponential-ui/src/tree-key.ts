// VAPP-285: the WAI-ARIA tree keys (src/tree.ts has the rows); no import,
// so the web renderers handle them without the expression evaluator.

/** One visible row as treeKey reads it. */
export interface TreeKeyRow {
  level: number
  /** true/false on a branch, absent on a leaf (and the loading row). */
  expanded?: boolean
}

/** What a key does on row `index`: move the focus, fold the row, or nothing. */
export type TreeKeyResult = { focus: number } | { toggle: true } | null

/** The WAI-ARIA tree keys (src/tree.ts module comment). `key` = a DOM key name. */
export function treeKey(rows: readonly TreeKeyRow[], index: number, key: string, rtl: boolean): TreeKeyResult {
  const row = rows[index]
  if (!row) return null
  const openKey = rtl ? `ArrowLeft` : `ArrowRight`
  const closeKey = rtl ? `ArrowRight` : `ArrowLeft`
  const last = rows.length - 1
  if (key === `ArrowDown`) return index < last ? { focus: index + 1 } : null
  if (key === `ArrowUp`) return index > 0 ? { focus: index - 1 } : null
  if (key === `Home`) return index > 0 ? { focus: 0 } : null
  if (key === `End`) return index < last ? { focus: last } : null
  if (key === openKey) {
    if (row.expanded === false) return { toggle: true }
    if (row.expanded === true) {
      const next = rows[index + 1]
      return next && next.level === row.level + 1 ? { focus: index + 1 } : null
    }
    return null
  }
  if (key === closeKey) {
    if (row.expanded === true) return { toggle: true }
    for (let i = index - 1; i >= 0; i--) {
      if (rows[i]!.level < row.level) return rows[i]!.level === row.level - 1 ? { focus: i } : null
    }
    return null
  }
  return null
}
