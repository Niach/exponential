import { useEffect, useMemo, useState, type ReactNode } from "react"
import {
  unchangedBetween,
  unchangedBefore,
  unchangedLabel,
  type DiffFile,
  type DiffHunk,
  type DiffLine,
} from "@exp/domain-contract/diff"

import { Button } from "./button"
import { cn } from "./cn"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "./collapsible"
import { DiffCounts, DiffPath, DiffStatusLetter } from "./diff-counts"
import { highlightLine, languageFor } from "./diff-lang"
import { GLASS_SURFACE } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// EXP-895 — ONE file's diff, unified layout, ×4. The card is the whole of the
// per-file rendering: the sticky `letter · path · counts · chevron` header and a
// four-column body (old gutter, new gutter, sign, text). It takes a `DiffFile`
// off the shared parser and nothing else — no PullFile, no patch strings.
//
// The web analog of the desktop IDE's virtualized diff: instead of virtualizing
// it CAPS and expands, so a 20k-line file never hard-freezes the tab.

const ChevronDownGlyph = conceptIcon(`ui-chevron-down`)

/** A file with more hunk lines than this starts collapsed. */
const COLLAPSE_THRESHOLD = 300
/** An expanded file reveals this many lines per "Show more" step. */
const LINE_CHUNK = 500
/** Per-line syntax highlighting is skipped past this many hunk lines. */
const HIGHLIGHT_LIMIT = 1500

export type DiffDensity = `comfortable` | `compact`

/** The hunk lines of a file — the number every cap above is measured in. */
export function diffLineCount(file: DiffFile): number {
  let n = 0
  for (const hunk of file.hunks) n += hunk.lines.length
  return n
}

/** Whether a file opens by default at this list's setting. */
export function diffOpensByDefault(
  file: DiffFile,
  defaultCollapsed: boolean
): boolean {
  return !defaultCollapsed && diffLineCount(file) <= COLLAPSE_THRESHOLD
}

/**
 * What a file with no hunks says instead of rows. A binary blob, a pure rename,
 * an empty new file and a patch GitHub refused to send all land here, and the
 * reader has to be told WHICH.
 */
export function noHunksNote(file: DiffFile): string {
  if (file.binary) return `Binary file`
  switch (file.status) {
    case `added`:
      return `Empty file added`
    case `removed`:
      return `File removed`
    case `renamed`:
      return `Renamed without content changes`
    case `copied`:
      return `Copied without content changes`
    default:
      return `No textual diff (binary or too large)`
  }
}

type Row =
  /** A `N unchanged lines` divider — context the patch never carried. */
  | { kind: `gap`; key: string; text: string }
  | { kind: `hunk`; key: string; text: string }
  | { kind: `line`; key: string; line: DiffLine }

/** The file flattened into display rows: a gap divider before each hunk that
 *  skipped context, the verbatim `@@ … @@` header, then the hunk's lines. */
function buildRows(hunks: readonly DiffHunk[]): Row[] {
  const out: Row[] = []
  hunks.forEach((hunk, h) => {
    const prev = h === 0 ? null : hunks[h - 1]
    const skipped = prev ? unchangedBetween(prev, hunk) : unchangedBefore(hunk)
    if (skipped > 0) {
      out.push({ kind: `gap`, key: `g${h}`, text: unchangedLabel(skipped) })
    }
    out.push({ kind: `hunk`, key: `h${h}`, text: hunk.header })
    hunk.lines.forEach((line, i) => {
      out.push({ kind: `line`, key: `l${h}-${i}`, line })
    })
  })
  return out
}

const GRID_CLASS: Record<DiffDensity, string> = {
  comfortable: `grid grid-cols-[3rem_3rem_1rem_1fr]`,
  compact: `grid grid-cols-[2.5rem_2.5rem_0.75rem_1fr]`,
}

const GUTTER_CLASS = `select-none pr-2 text-right tabular-nums text-diff-gutter-fg`

export function FileDiffCard({
  file,
  open,
  defaultOpen = true,
  onOpenChange,
  density = `comfortable`,
  isMobile = false,
  className,
}: {
  file: DiffFile
  /** Controlled disclosure. Absent = the card owns its own, seeded by
   *  `defaultOpen`. */
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  density?: DiffDensity
  /** Threaded in, never read here — see `DiffPath`. */
  isMobile?: boolean
  className?: string
}) {
  const [ownOpen, setOwnOpen] = useState(defaultOpen)
  const isOpen = open ?? ownOpen
  const setOpen = (next: boolean) => {
    if (open === undefined) setOwnOpen(next)
    onOpenChange?.(next)
  }

  const rows = useMemo(() => buildRows(file.hunks), [file.hunks])
  const lineTotal = useMemo(() => diffLineCount(file), [file])
  const lang = useMemo(() => languageFor(file.path), [file.path])

  const [visibleCount, setVisibleCount] = useState(LINE_CHUNK)
  // Reset the reveal cap when the file's hunks themselves change (a refresh).
  useEffect(() => setVisibleCount(LINE_CHUNK), [file.hunks])

  const doHighlight = lang !== null && lineTotal <= HIGHLIGHT_LIMIT

  // Rows are built only while expanded — a collapsed large file costs nothing.
  const body = useMemo(() => {
    if (!isOpen || rows.length === 0) return null
    let shown = 0
    const out: ReactNode[] = []
    for (const row of rows) {
      if (row.kind === `gap`) {
        // A PLAIN row, never a button: the hunk's skipped context is not on the
        // wire, so there is nothing to expand to (EXP-895).
        out.push(
          <div
            key={row.key}
            className="bg-diff-hunk-bg/60 px-3 py-0.5 text-center whitespace-pre text-diff-gutter-fg"
            data-diff-row="gap"
          >
            {row.text}
          </div>
        )
        continue
      }
      if (row.kind === `hunk`) {
        out.push(
          <div
            key={row.key}
            className="bg-diff-hunk-bg px-3 py-0.5 whitespace-pre text-diff-hunk-fg"
            data-diff-row="hunk"
          >
            {row.text}
          </div>
        )
        continue
      }
      if (shown >= visibleCount) break
      shown += 1
      const { line } = row
      if (line.kind === `meta`) {
        // `\ No newline at end of file` — numbered on neither side.
        out.push(
          <div
            key={row.key}
            className="px-3 italic whitespace-pre text-diff-gutter-fg"
            data-diff-row="meta"
          >
            {line.text}
          </div>
        )
        continue
      }
      const content =
        doHighlight && lang ? highlightLine(lang, line.text, row.key) : line.text
      out.push(
        <div
          key={row.key}
          data-diff-row={line.kind}
          className={cn(
            GRID_CLASS[density],
            line.kind === `add` && `bg-diff-add-bg`,
            line.kind === `del` && `bg-diff-del-bg`
          )}
        >
          <span className={GUTTER_CLASS}>{line.oldNo ?? ``}</span>
          <span className={GUTTER_CLASS}>{line.newNo ?? ``}</span>
          <span
            className={cn(
              `select-none text-center`,
              line.kind === `add` && `text-diff-add-fg`,
              line.kind === `del` && `text-diff-del-fg`
            )}
          >
            {line.kind === `add` ? `+` : line.kind === `del` ? `−` : ``}
          </span>
          <span
            className={cn(
              `pr-3 whitespace-pre [tab-size:4]`,
              doHighlight
                ? `text-foreground/90`
                : line.kind === `add`
                  ? `text-diff-add-fg`
                  : line.kind === `del`
                    ? `text-diff-del-fg`
                    : `text-muted-foreground`
            )}
          >
            {content || ` `}
          </span>
        </div>
      )
    }
    return out
  }, [isOpen, rows, visibleCount, lang, doHighlight, density])

  const hiddenCount = Math.max(0, lineTotal - visibleCount)

  return (
    // overflow-clip (not hidden) keeps the rounded corners without creating a
    // scroll container, so the sticky header can stick to the page's scrollport.
    <Collapsible
      open={isOpen}
      onOpenChange={setOpen}
      className={cn(GLASS_SURFACE, `overflow-clip`, className)}
      data-testid="file-diff-card"
    >
      <div className="sticky top-0 z-10 rounded-t-md bg-background">
        <CollapsibleTrigger
          className={cn(
            `flex w-full items-center gap-2 rounded-t-md border-b border-glass-stroke bg-muted/30 px-3 py-1.5 text-left text-xs transition-colors duration-fast hover:bg-muted/50`,
            !isOpen && `rounded-b-md border-b-transparent`
          )}
        >
          <DiffStatusLetter status={file.status} />
          <DiffPath path={file.path} isMobile={isMobile} />
          {file.previousPath && (
            <span
              className="min-w-0 shrink truncate font-mono text-muted-foreground"
              title={file.previousPath}
            >
              {`← ${file.previousPath}`}
            </span>
          )}
          <span className="ml-auto" />
          <DiffCounts additions={file.additions} deletions={file.deletions} />
          {/* EXP-706: the disclosure chevron trails the row (native parity) —
              pointing down when collapsed, flipped when open. */}
          <ChevronDownGlyph
            className={cn(
              `size-3.5 shrink-0 text-muted-foreground transition-transform duration-fast`,
              isOpen && `rotate-180`
            )}
          />
        </CollapsibleTrigger>
      </div>
      <CollapsibleContent>
        {file.hunks.length > 0 ? (
          <>
            <div className="diff-code overflow-x-auto font-mono text-[0.6875rem] leading-relaxed">
              <div className="w-max min-w-full py-0.5">{body}</div>
            </div>
            {hiddenCount > 0 && (
              <Button
                variant="ghost"
                size="xs"
                className="w-full rounded-none border-t border-glass-stroke text-muted-foreground"
                onClick={() => setVisibleCount((c) => c + LINE_CHUNK)}
              >
                {`Show ${Math.min(LINE_CHUNK, hiddenCount)} more lines (${hiddenCount} hidden)`}
              </Button>
            )}
          </>
        ) : (
          <div className="px-3 py-2 text-xs text-muted-foreground">
            {noHunksNote(file)}
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}
