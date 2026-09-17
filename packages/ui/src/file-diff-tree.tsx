import { useMemo, useState, type ReactNode } from "react"
import { contract } from "@exp/domain-contract"
import { summaryLabel, totals, type DiffFile } from "@exp/domain-contract/diff"
import { diffFileTree, type DiffTreeNode } from "@exp/domain-contract/diff-tree"

import { Button } from "./button"
import { cn } from "./cn"
import { DiffCounts, DiffStatusLetter } from "./diff-counts"
import { conceptIcon } from "./icons.generated"
import { Input } from "./input"

// EXP-916 — the file column beside the diff: the review's summary over a
// filterable TREE of the changed files, folders first, VS-Code compact chains
// (`apps/web/src` is one node). The shape is the contract's `diffFileTree`, so
// the desktop's `diff_pane::file_tree`, iOS's `DiffFileTree` and Android's
// draw the same rows in the same order.
//
// Picking a row is the caller's move (the list scrolls, a sheet closes), so
// this only reports the path. A non-blank filter flattens to the matching FILE
// rows — a search result, not a pruned tree.

const ChevronDownGlyph = conceptIcon(`ui-chevron-down`)
const FolderGlyph = conceptIcon(`ui-folder`)
const FolderOpenGlyph = conceptIcon(`ui-folder-open`)

export const DIFF_FILTER_PLACEHOLDER = contract.diffUi.filterPlaceholder

/** One level of indent, in px — the same step every client uses. */
const INDENT_STEP = 12

export function FileDiffTree({
  files,
  selected,
  onSelect,
  flush = false,
  className,
}: {
  files: readonly DiffFile[]
  /** The path the caller is showing — highlighted in the list. */
  selected?: string | null
  onSelect: (path: string) => void
  /** EXP-916: no card chrome and the rows fill the height — the tree as
   *  the sidebar's panel (a review's `ReviewFilesNav`), whose column is
   *  the frame. Default: a bordered card beside the diff. */
  flush?: boolean
  className?: string
}) {
  const [filter, setFilter] = useState(``)
  // Folders are OPEN by default; the map holds only the ones the reader shut,
  // keyed by path, so a refreshed `files` never reopens a folded branch.
  const [closed, setClosed] = useState<Record<string, boolean>>({})
  // The header counts the WHOLE diff, never the filtered slice: it is the
  // review's summary line, not a search result count.
  const summary = useMemo(() => {
    const sum = totals(files)
    return summaryLabel(sum.files, sum.additions, sum.deletions)
  }, [files])
  const nodes = useMemo(() => diffFileTree(files, filter), [files, filter])

  const rows: ReactNode[] = []
  const walk = (list: readonly DiffTreeNode[], depth: number) => {
    for (const node of list) {
      const indent = { paddingLeft: `${depth * INDENT_STEP + 12}px` }
      if (node.kind === `file`) {
        rows.push(
          <Button
            key={`f:${node.path}`}
            variant="ghost"
            size="xs"
            onClick={() => onSelect(node.path)}
            title={node.path}
            style={indent}
            data-testid={`diff-nav-row-${node.path}`}
            className={cn(
              `flex h-6 w-full items-center justify-start gap-2 rounded-none pr-3 font-normal`,
              selected === node.path && `bg-glass-active`
            )}
          >
            <DiffStatusLetter
              status={files[node.index]?.status ?? `modified`}
            />
            <span className="min-w-0 truncate font-mono">{node.name}</span>
            <span className="ml-auto" />
            <DiffCounts
              additions={node.additions}
              deletions={node.deletions}
            />
          </Button>
        )
        continue
      }
      const open = closed[node.path] !== true
      rows.push(
        <Button
          key={`d:${node.path}`}
          variant="ghost"
          size="xs"
          onClick={() =>
            setClosed((prev) => ({ ...prev, [node.path]: open }))
          }
          title={node.path}
          style={indent}
          aria-expanded={open}
          data-testid={`diff-nav-dir-${node.path}`}
          className="flex h-6 w-full items-center justify-start gap-2 rounded-none pr-3 font-normal"
        >
          {open ? (
            <FolderOpenGlyph className="size-3 shrink-0 text-muted-foreground" />
          ) : (
            <FolderGlyph className="size-3 shrink-0 text-muted-foreground" />
          )}
          <span className="min-w-0 truncate font-mono">{node.name}</span>
          <span className="ml-auto" />
          <DiffCounts additions={node.additions} deletions={node.deletions} />
          <ChevronDownGlyph
            className={cn(
              `size-3 shrink-0 text-muted-foreground transition-transform duration-fast`,
              open && `rotate-180`
            )}
            aria-hidden
          />
        </Button>
      )
      if (open) walk(node.children, depth + 1)
    }
  }
  walk(nodes, 0)

  return (
    <div
      className={cn(
        flush
          ? `flex h-full min-h-0 flex-col`
          : `rounded-md border border-glass-stroke`,
        className
      )}
      data-testid="file-diff-tree"
      data-flush={flush || undefined}
    >
      <div
        className={cn(
          `flex items-center gap-2 border-b border-glass-stroke bg-glass-section px-3 py-1.5 text-xs`,
          !flush && `rounded-t-md`
        )}
      >
        <span className="min-w-0 truncate font-medium">{summary}</span>
      </div>
      <div className="border-b border-glass-stroke p-1.5">
        <Input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={DIFF_FILTER_PLACEHOLDER}
          aria-label={DIFF_FILTER_PLACEHOLDER}
          className="h-7 text-xs"
          data-testid="diff-nav-filter"
        />
      </div>
      <div
        className={cn(
          `overflow-y-auto py-1`,
          flush ? `min-h-0 flex-1` : `max-h-[60vh]`
        )}
      >
        {rows}
      </div>
    </div>
  )
}
