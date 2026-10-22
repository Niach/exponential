// FEED-131: shadcn's Table (`table-container` > `table` > header/body/footer
// rows) at the LOOK of the renderer's Table native: a muted 12px/medium
// header, hairline row dividers on `--border`, the glass row wash under the
// pointer, the selected row (`data-state="selected"`) on the active wash,
// cells 8px/12px at 14px, no outer border. The sheet ships no preflight,
// so the root collapses its borders itself (row hairlines need it).
export const TABLE_CLASSES = {
  container: `relative w-full overflow-x-auto`,
  root: `w-full caption-bottom border-collapse text-sm`,
  header: `[&_tr]:hover:bg-transparent`,
  body: `[&_tr:last-child]:border-0`,
  footer: `border-t border-border font-medium [&>tr]:last:border-b-0`,
  row: `border-b border-border transition-colors duration-fast hover:bg-glass-row data-[state=selected]:bg-glass-active`,
  head: `px-3 py-2 text-left align-middle text-xs font-medium whitespace-nowrap text-muted-foreground [&:has([role=checkbox])]:pr-0`,
  cell: `px-3 py-2 align-middle text-sm whitespace-nowrap [&:has([role=checkbox])]:pr-0`,
  caption: `mt-4 text-sm text-muted-foreground`,
} as const
