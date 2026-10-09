import {
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react"
import type { DiffFile } from "@exp/domain-contract/diff"
import { contract } from "@exp/domain-contract"
import { Button } from "./button"
import { DiffCounts } from "./diff-counts"
import { DisclosureHeader } from "./disclosure-header"
import { GlassSectionHeader } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { ImagePreviewDialog } from "./image-preview-dialog"
import {
  GUIDE_CHANGES_TOPIC,
  guideCoverage,
  guideFileCountLabel,
  guideSectionCaption,
  type GuideChangeSet,
  SESSION_INLINE_TILE_HEIGHT,
  SESSION_RESULT_TILE_HEIGHT,
  SESSION_RESULTS_EARLIER_LABEL,
  sessionResultTileCaption,
  sessionResultIsTall,
  sessionResultPictures,
  type SessionResultGroup,
  sessionResultTileHeightFitting,
  sessionResultTileWidth,
  type SessionResultEntry,
} from "./session-results"
import { cn } from "./cn"

// EXP-879: the RESULTS face — the screenshots a run published with
// (EXP-933: and each topic's GFM report text, above its tiles)
// `exponential_sessions_guide` (EXP-1251, formerly `_results`), read off the synced `coding_sessions.results`
// jsonb. One group band per topic (EXP-818's `GlassSectionHeader`, the same
// band every list wears) over a wrapping strip of tiles; every tile is the
// same 320px tall, so an iOS, an Android and a web shot of one screen read as
// one row. Tapping a tile opens the shared lightbox. Desktop
// `crates/ui/src/session_results.rs`, iOS `SessionResultsView`, Android
// `SessionResultsScreen` mirror it.
//
// On a phone that 320px base makes a landscape shot 480px wide and the column
// clips it, so the page is measured ONCE and every tile shares the one
// `sessionResultTileHeightFitting` factor (the shared rule ×4) — per-row
// fitting would break the equal-height strip.
//
// EXP-1128: a TALL picture (a full-page capture, `sessionResultIsTall`) takes
// the 4:3 frame cropped to its TOP under a bottom fade and a `Tall` pill, so it
// reads as its first viewport instead of a 10px sliver; the lightbox then
// opens it fit-to-width in a vertical scroll. Same look ×4.
//
// EXP-1172: `exponential_sessions_show` pictures (`inline`) render in the run
// TRANSCRIPT at the call (`SessionInlineResultTile`, one tile at the 240px
// base, the call's caption under it); on this face a topic with more than one
// picture folds them under a collapsed `Earlier · N` disclosure below its
// final tiles, so the report leads. Fixture `session-inline.json` ×4.
//
// EXP-1154: the face reads as the GUIDE. The `Summary` topic leads as a plain
// paragraph (no band, no number); every other topic keeps its band with a
// muted `01 / 04` caption in the leading slot, its text and its tiles.
// EXP-1251: per section ONE `Changes · N files · +A −D ›` row (the diff files
// the topic names, by path or rename source) opens that section's diff page;
// every file no topic names lands in a trailing unnumbered `Other changes`
// band (with no report at all, one `Changes` band holds the whole diff) and a
// final hairline `Show complete diff` row opens everything. Pure rule
// `guideCoverage`, fixture `session-results.json` `coverage` ×4.

/** The measured CONTENT width of a node, 0 until the first measurement (and in
 *  jsdom, which has no layout): the callers render at the base height then. */
function useContentWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0)
  useLayoutEffect(() => {
    const node = ref.current
    if (!node) return
    const measure = () => {
      const style = getComputedStyle(node)
      const padding =
        (Number.parseFloat(style.paddingLeft) || 0) +
        (Number.parseFloat(style.paddingRight) || 0)
      setWidth(Math.max(0, node.getBoundingClientRect().width - padding))
    }
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    return () => observer.disconnect()
  }, [ref])
  return width
}

function ResultTile({
  entry,
  height,
  src,
  onOpen,
  caption = entry.label,
}: {
  entry: SessionResultEntry
  height: number
  src: string
  onOpen: () => void
  /** The line under the shot: the label on the Results face, the show
   *  call's caption in the transcript (EXP-1172). */
  caption?: string
}) {
  const tall = sessionResultIsTall(entry)
  return (
    <Button
      variant="ghost"
      onClick={onOpen}
      title={entry.label}
      data-testid={`session-result-${entry.attachmentId}`}
      className={cn(
        `h-auto w-auto flex-col items-start gap-1.5 rounded-lg p-0`,
        `hover:bg-transparent`
      )}
    >
      <span className="relative flex">
        <img
          src={src}
          alt={entry.label}
          loading="lazy"
          data-tall={tall || undefined}
          // ONE height for the whole page (the 320px base, or the fitted one
          // on a narrow column — never an `h-[320px]` literal that would drift
          // from the constant), the probed aspect for the width: the tile
          // keeps the shot's shape and a 4:3 desktop frame stands in when the
          // upload could not be measured OR is tall (`sessionResultTileWidth`);
          // a tall one anchors to its top so the crop is the first viewport.
          style={{
            height,
            aspectRatio: `${sessionResultTileWidth(entry, height)} / ${height}`,
          }}
          className={cn(
            `w-auto rounded-lg border border-glass-stroke-card object-cover`,
            tall && `object-top`
          )}
        />
        {tall && (
          <>
            <span
              aria-hidden
              className="pointer-events-none absolute inset-x-0 bottom-0 h-12 rounded-b-lg bg-linear-to-t from-black/50 to-transparent"
            />
            <span
              data-testid="session-result-tall"
              className="pointer-events-none absolute right-1.5 bottom-1.5 rounded-full border border-glass-stroke-card bg-black/60 px-1.5 py-0.5 text-[10px] leading-none text-white/90"
            >
              Tall
            </span>
          </>
        )}
      </span>
      <span className="w-full truncate text-left text-xs text-muted-foreground">
        {caption}
      </span>
    </Button>
  )
}

function ResultPreview({
  entry,
  src,
  onClose,
}: {
  entry: SessionResultEntry
  src: string
  onClose: () => void
}) {
  return (
    <ImagePreviewDialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
      src={src}
      alt={entry.label}
      label={entry.label}
      naturalSize={
        entry.width !== null && entry.height !== null
          ? { width: entry.width, height: entry.height }
          : null
      }
    />
  )
}

/** EXP-1172: a group's inline pictures, folded under `Earlier · N` until
 *  opened; open, they wrap like the tiles above them. */
function EarlierFold({
  entries,
  height,
  attachmentSrc,
  onOpen,
}: {
  entries: readonly SessionResultEntry[]
  height: number
  attachmentSrc: (attachmentId: string) => string
  onOpen: (entry: SessionResultEntry) => void
}) {
  const [open, setOpen] = useState(false)
  return (
    <div className="flex flex-col pt-3" data-testid="session-results-earlier">
      <DisclosureHeader
        open={open}
        onToggle={() => setOpen((value) => !value)}
        className="w-auto self-start text-xs"
      >
        {`${SESSION_RESULTS_EARLIER_LABEL} · ${entries.length}`}
      </DisclosureHeader>
      {open && (
        <div className="flex flex-wrap gap-3 pt-3">
          {entries.map((entry) => (
            <ResultTile
              key={`${entry.topic}/${entry.label}/${entry.attachmentId}`}
              entry={entry}
              height={height}
              src={attachmentSrc(entry.attachmentId)}
              onOpen={() => onOpen(entry)}
            />
          ))}
        </div>
      )}
    </div>
  )
}

/** EXP-1154: a Guide section's band: the shared group band with the muted
 *  `01 / 04` caption in its leading slot (no caption without a number: the
 *  PR-body fallback's one group). */
export function GuideSectionHeader({
  label,
  index,
  total,
  className,
}: {
  label: string
  index?: number
  total?: number
  className?: string
}) {
  return (
    <GlassSectionHeader
      label={label}
      className={cn(`-mx-3 w-auto md:mx-0`, className)}
      leading={
        index !== undefined && total !== undefined ? (
          <span
            className="mr-1 text-xs tabular-nums text-muted-foreground"
            data-testid="guide-section-caption"
          >
            {guideSectionCaption(index, total)}
          </span>
        ) : undefined
      }
    />
  )
}

const GuideChangesGlyph = conceptIcon(`guide-changes`)
const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)

/** EXP-1251: the label of the Guide's last row. */
export const GUIDE_SHOW_COMPLETE_DIFF_LABEL = contract.diffUi.guideShowCompleteDiff

/** EXP-1251: what a Changes row opens: a numbered section (1-based), the
 *  lead, the automatic `Other changes` band or the complete diff. */
export interface GuideChangesTarget {
  section: number | `lead` | `other` | `all`
  files: DiffFile[]
}

const GUIDE_ROW_CLASS = `flex h-9 w-full min-w-0 items-center gap-2.5 px-1 text-left text-sm`

/** EXP-1251: a section's ONE Changes row: the `guide-changes` glyph, the
 *  label, the muted file count, the counts and a chevron; inert (no chevron)
 *  without `onOpen`. */
export function GuideChangesRow({
  changes,
  onOpen,
  label = GUIDE_CHANGES_TOPIC,
  className,
}: {
  changes: GuideChangeSet<DiffFile>
  onOpen?: () => void
  label?: string
  className?: string
}) {
  const body = (
    <>
      <GuideChangesGlyph className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
        {guideFileCountLabel(changes.fileCount)}
      </span>
      <DiffCounts additions={changes.additions} deletions={changes.deletions} className="text-xs" />
      {onOpen && <ChevronRightGlyph className="size-4 shrink-0 text-muted-foreground" aria-hidden />}
    </>
  )
  const rowClass = cn(GUIDE_ROW_CLASS, `border-b border-glass-stroke`, className)
  if (!onOpen) {
    return (
      <div className={rowClass} data-testid="guide-changes-row">
        {body}
      </div>
    )
  }
  return (
    <Button
      variant="ghost"
      onClick={onOpen}
      data-testid="guide-changes-row"
      className={cn(rowClass, `rounded-none font-normal hover:bg-glass-row`)}
    >
      {body}
    </Button>
  )
}

/** EXP-1251: the Guide's final hairline row: the whole diff. */
export function GuideShowCompleteDiffRow({
  changes,
  onOpen,
}: {
  changes: GuideChangeSet<DiffFile>
  onOpen?: () => void
}) {
  const body = (
    <>
      <span className="min-w-0 flex-1 truncate">{GUIDE_SHOW_COMPLETE_DIFF_LABEL}</span>
      <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
        {guideFileCountLabel(changes.fileCount)}
      </span>
      <DiffCounts additions={changes.additions} deletions={changes.deletions} className="text-xs" />
      {onOpen && <ChevronRightGlyph className="size-4 shrink-0 text-muted-foreground" aria-hidden />}
    </>
  )
  const rowClass = cn(GUIDE_ROW_CLASS, `border-t border-glass-stroke text-muted-foreground`)
  if (!onOpen) {
    return (
      <div className={rowClass} data-testid="guide-complete-diff">
        {body}
      </div>
    )
  }
  return (
    <Button
      variant="ghost"
      onClick={onOpen}
      data-testid="guide-complete-diff"
      className={cn(rowClass, `rounded-none font-normal hover:bg-glass-row hover:text-foreground`)}
    >
      {body}
    </Button>
  )
}

/**
 * EXP-1172: the picture an `exponential_sessions_show` call filed, drawn
 * under that call's transcript row — one tile at the inline base, fitted to
 * the column like a Results page, the call's caption (else the label) under
 * it, the shared lightbox behind a tap.
 */
export function SessionInlineResultTile({
  entry,
  attachmentSrc,
  className,
}: {
  entry: SessionResultEntry
  attachmentSrc: (attachmentId: string) => string
  className?: string
}) {
  const [preview, setPreview] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)
  const width = useContentWidth(containerRef)
  const height = width
    ? sessionResultTileHeightFitting([entry], width, SESSION_INLINE_TILE_HEIGHT)
    : SESSION_INLINE_TILE_HEIGHT
  const src = attachmentSrc(entry.attachmentId)
  return (
    <div ref={containerRef} className={cn(`flex`, className)} data-testid="session-inline-result">
      <ResultTile
        entry={entry}
        height={height}
        src={src}
        caption={sessionResultTileCaption(entry)}
        onOpen={() => setPreview(true)}
      />
      {preview && (
        <ResultPreview entry={entry} src={src} onClose={() => setPreview(false)} />
      )}
    </div>
  )
}

export function SessionResultsView({
  groups,
  attachmentSrc,
  renderText,
  files,
  onOpenChanges,
  onOpenFile,
  numbered = true,
  className,
}: {
  /** EXP-933: the report — `parseSessionResultGroups`, text and pictures. */
  groups: readonly SessionResultGroup[]
  /** The URL a published shot reads from — the app owns the route, this
   *  package only owns the tiles. */
  attachmentSrc: (attachmentId: string) => string
  /** EXP-933: renders a topic's GFM text. The app owns the markdown renderer
   *  (issue pills, links); without one the text shows as plain pre-wrapped
   *  prose. */
  renderText?: (text: string) => ReactNode
  /** EXP-1251: the loaded diff the coverage reads (null or absent = not
   *  loaded: no Changes rows, no Other changes, no complete row). */
  files?: readonly DiffFile[] | null
  /** EXP-1251: a Changes row (or Show complete diff) opens that file set. */
  onOpenChanges?: (target: GuideChangesTarget) => void
  /** @deprecated EXP-1251: without `onOpenChanges`, a row opens its FIRST
   *  file here (the pre-Guide callers). */
  onOpenFile?: (path: string) => void
  /** EXP-1154: false = bands without the `01 / 04` caption (the PR-body
   *  fallback's single group). */
  numbered?: boolean
  /** Overrides the page gutter (a host that already pads passes `px-0`). */
  className?: string
}) {
  const [preview, setPreview] = useState<SessionResultEntry | null>(null)
  const pictures = sessionResultPictures(groups)
  const coverage = guideCoverage(groups, files)
  const opener = (target: GuideChangesTarget): (() => void) | undefined => {
    if (target.files.length === 0) return undefined
    if (onOpenChanges) return () => onOpenChanges(target)
    if (onOpenFile) return () => onOpenFile(target.files[0]!.path)
    return undefined
  }
  // Every band's wrapping strip sits in the SAME column, so one measurement
  // (the root's content width) fits the whole page.
  const containerRef = useRef<HTMLDivElement>(null)
  const width = useContentWidth(containerRef)
  const height = width
    ? sessionResultTileHeightFitting(pictures, width)
    : SESSION_RESULT_TILE_HEIGHT
  const changesRow = (
    changes: GuideChangeSet<DiffFile> | null,
    section: GuideChangesTarget[`section`]
  ) =>
    changes && changes.fileCount > 0 ? (
      <GuideChangesRow
        className="mt-3"
        changes={changes}
        onOpen={opener({ section, files: changes.files })}
      />
    ) : null
  const body = (
    group: SessionResultGroup,
    changes: GuideChangeSet<DiffFile> | null,
    section: GuideChangesTarget[`section`]
  ) => (
    <>
      {group.text !== null && (
        <div
          className={cn(
            `max-w-3xl text-sm`,
            // TipTap's trailing-node paragraph is an empty line under a
            // closing code block or list; read-only it is only a gap.
            `[&_.tiptap-content>p:last-child:has(>br.ProseMirror-trailingBreak:only-child)]:hidden`
          )}
          data-testid="session-result-text"
        >
          {renderText ? (
            renderText(group.text)
          ) : (
            <p className="whitespace-pre-wrap">{group.text}</p>
          )}
        </div>
      )}
      {changesRow(changes, section)}
      {group.entries.length > 0 && (
        <div className="flex flex-wrap gap-3 pt-3">
          {group.entries.map((entry) => (
            <ResultTile
              key={`${entry.topic}/${entry.label}/${entry.attachmentId}`}
              entry={entry}
              height={height}
              src={attachmentSrc(entry.attachmentId)}
              onOpen={() => setPreview(entry)}
            />
          ))}
        </div>
      )}
      {group.earlier.length > 0 && (
        <EarlierFold
          entries={group.earlier}
          height={height}
          attachmentSrc={attachmentSrc}
          onOpen={setPreview}
        />
      )}
    </>
  )
  return (
    <div
      ref={containerRef}
      // Phone: the COLUMN (text + tiles) sits at the band's LABEL; the band
      // pulls out by its own px-3 (`-mx-3`), so it lines up with the header's
      // title. EXP-1191 md+: the work column's ONE edge (`WORK_GUTTER_CLASS`)
      // — text and the band's box both start on it, like every other band
      // and card of the work column. The column, not the band, is what
      // `useContentWidth` measures, so tiles fit it exactly.
      className={cn(`flex flex-col gap-7 px-7 py-5 md:px-5`, className)}
      data-testid="session-results"
    >
      {coverage.lead && (
        <div className="flex flex-col" data-testid="guide-lead">
          {body(coverage.lead.group, coverage.lead.changes, `lead`)}
        </div>
      )}
      {coverage.sections.map(({ group, index, total, changes }) => (
        // Keyed by position too: a workflow concatenates several runs'
        // groups, so one topic may band twice.
        <div key={`${index}/${group.topic}`} className="flex flex-col">
          <GuideSectionHeader
            label={group.topic}
            index={numbered ? index : undefined}
            total={numbered ? total : undefined}
          />
          <div className="flex flex-col pt-2">{body(group, changes, index)}</div>
        </div>
      ))}
      {coverage.other && (
        <div className="flex flex-col" data-testid="guide-other-changes">
          <GuideSectionHeader label={coverage.other.topic} className="text-muted-foreground" />
          <div className="flex flex-col pt-2">
            {changesRow(coverage.other.changes, `other`)}
          </div>
        </div>
      )}
      {coverage.complete && coverage.complete.fileCount > 0 && (
        <GuideShowCompleteDiffRow
          changes={coverage.complete}
          onOpen={opener({ section: `all`, files: coverage.complete.files })}
        />
      )}
      {preview && (
        <ResultPreview
          entry={preview}
          src={attachmentSrc(preview.attachmentId)}
          onClose={() => setPreview(null)}
        />
      )}
    </div>
  )
}
