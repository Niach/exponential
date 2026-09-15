import { useMemo, useState } from "react"
import { summaryLabel, totals, type DiffFile } from "@exp/domain-contract/diff"

import { Button } from "./button"
import { cn } from "./cn"
import {
  DiffCounts,
  diffPathBase,
  diffPathDir,
  DiffStatusLetter,
} from "./diff-counts"
import { Input } from "./input"

// EXP-895 — the file column: the diff's own summary over a filterable flat list
// of `letter · name · dimmed dir · counts` rows. The web analog of the desktop
// IDE's diff file list + `scroll_to_file`; picking a row is the caller's move
// (the list scrolls, a sheet closes), so this only reports the path.

export const DIFF_FILTER_PLACEHOLDER = `Filter files`

export function FileDiffNav({
  files,
  selected,
  onSelect,
  filterable = true,
  className,
}: {
  files: readonly DiffFile[]
  /** The path the caller is showing — highlighted in the list. */
  selected?: string | null
  onSelect: (path: string) => void
  filterable?: boolean
  className?: string
}) {
  const [filter, setFilter] = useState(``)
  // The header counts the WHOLE diff, never the filtered slice: it is the
  // review's summary line, not a search result count.
  const summary = useMemo(() => {
    const sum = totals(files)
    return summaryLabel(sum.files, sum.additions, sum.deletions)
  }, [files])
  const shown = useMemo(() => {
    const needle = filter.trim().toLowerCase()
    if (!needle) return files
    return files.filter((file) => file.path.toLowerCase().includes(needle))
  }, [files, filter])

  return (
    <div
      className={cn(`rounded-md border border-glass-stroke`, className)}
      data-testid="file-diff-nav"
    >
      <div className="flex items-center gap-2 rounded-t-md border-b border-glass-stroke bg-glass-section px-3 py-1.5 text-xs">
        <span className="min-w-0 truncate font-medium">{summary}</span>
      </div>
      {filterable && (
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
      )}
      <div className="max-h-[60vh] overflow-y-auto py-1">
        {shown.map((file) => (
          <Button
            key={file.path}
            variant="ghost"
            size="xs"
            onClick={() => onSelect(file.path)}
            title={file.path}
            data-testid={`diff-nav-row-${file.path}`}
            className={cn(
              `flex h-6 w-full items-center justify-start gap-2 rounded-none px-3 font-normal`,
              selected === file.path && `bg-glass-active`
            )}
          >
            <DiffStatusLetter status={file.status} />
            <span className="min-w-0 shrink-0 truncate font-mono">
              {diffPathBase(file.path)}
            </span>
            {diffPathDir(file.path) && (
              <span className="min-w-0 truncate font-mono text-muted-foreground">
                {diffPathDir(file.path)}
              </span>
            )}
            <span className="ml-auto" />
            <DiffCounts additions={file.additions} deletions={file.deletions} />
          </Button>
        ))}
      </div>
    </div>
  )
}
