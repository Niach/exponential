import {
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react"
import type { DiffFile } from "@exp/domain-contract/diff"
import { Button } from "./button"
import { DiffCounts, DiffPath } from "./diff-counts"
import { DisclosureHeader } from "./disclosure-header"
import { GlassSectionHeader } from "./glass-rows"
import { ImagePreviewDialog } from "./image-preview-dialog"
import {
  groupSessionResults,
  guideFileRows,
  guideSectionCaption,
  guideSections,
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
// `exponential_sessions_results`, read off the synced `coding_sessions.results`
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
// muted `01 / 04` caption in the leading slot, its text, the FILES it touched
// (hairline rows: the dimmed-directory path + `+N −M` when the loaded diff has
// the path; a tap opens the Changes face on that file) and its tiles. Pure
// rules `guideSections`/`guideSectionCaption`/`guideFileRows`, fixture
// `session-results.json` `guide` ×4.

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

/** EXP-1154: one file a Guide section touched: the path, the counts when the
 *  loaded diff knows it; a ghost button row when it opens the file. */
export function GuideFileRow({
  path,
  counts,
  isMobile = false,
  onOpen,
}: {
  path: string
  counts: { additions: number; deletions: number } | null
  isMobile?: boolean
  onOpen?: () => void
}) {
  const body = (
    <>
      <DiffPath path={path} isMobile={isMobile} className="text-xs" />
      {counts && (
        <DiffCounts
          additions={counts.additions}
          deletions={counts.deletions}
          className="text-xs"
        />
      )}
    </>
  )
  const rowClass = `flex h-8 w-full min-w-0 items-center justify-between gap-3 px-1 text-left text-xs`
  if (!onOpen) {
    return (
      <div className={rowClass} data-testid="guide-file-row">
        {body}
      </div>
    )
  }
  return (
    <Button
      variant="ghost"
      onClick={onOpen}
      title={path}
      data-testid="guide-file-row"
      className={cn(rowClass, `rounded-none font-normal hover:bg-glass-row`)}
    >
      {body}
    </Button>
  )
}

/** EXP-1154: the Guide's row tap, offered only while there is a diff to open
 *  it on; with no files the rows render inert (no counts, no tap). */
export function guideFileOpener(
  files: readonly DiffFile[] | null | undefined,
  open: (path: string) => void
): ((path: string) => void) | undefined {
  return files && files.length > 0 ? open : undefined
}

/** EXP-1154: a Guide section's files, flat hairline-divided rows. */
export function GuideFileList({
  paths,
  files,
  isMobile = false,
  onOpenFile,
}: {
  paths: readonly string[]
  files?: readonly DiffFile[] | null
  isMobile?: boolean
  onOpenFile?: (path: string) => void
}) {
  if (paths.length === 0) return null
  return (
    <div
      className="mt-3 flex flex-col divide-y divide-glass-stroke border-y border-glass-stroke"
      data-testid="guide-file-list"
    >
      {guideFileRows(paths, files).map((row) => (
        <GuideFileRow
          key={row.path}
          path={row.path}
          counts={row.counts}
          isMobile={isMobile}
          onOpen={onOpenFile ? () => onOpenFile(row.path) : undefined}
        />
      ))}
    </div>
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
  results,
  groups: groupsProp,
  attachmentSrc,
  renderText,
  files,
  onOpenFile,
  isMobile = false,
  numbered = true,
  className,
}: {
  /** Pictures only (EXP-879); `groups` wins when both are passed. */
  results?: readonly SessionResultEntry[]
  /** EXP-933: the report — `parseSessionResultGroups`, text and pictures. */
  groups?: readonly SessionResultGroup[]
  /** The URL a published shot reads from — the app owns the route, this
   *  package only owns the tiles. */
  attachmentSrc: (attachmentId: string) => string
  /** EXP-933: renders a topic's GFM text. The app owns the markdown renderer
   *  (issue pills, links); without one the text shows as plain pre-wrapped
   *  prose. */
  renderText?: (text: string) => ReactNode
  /** EXP-1154: the loaded diff the file rows read their counts from (null or
   *  absent = paths without counts). */
  files?: readonly DiffFile[] | null
  /** EXP-1154: a file row opens the Changes face on that path; absent = the
   *  rows are plain text. */
  onOpenFile?: (path: string) => void
  /** Middle-truncates file-row directories on a phone (EXP-698). */
  isMobile?: boolean
  /** EXP-1154: false = bands without the `01 / 04` caption (the PR-body
   *  fallback's single group). */
  numbered?: boolean
  /** Overrides the page gutter (a host that already pads passes `px-0`). */
  className?: string
}) {
  const [preview, setPreview] = useState<SessionResultEntry | null>(null)
  const groups = groupsProp ?? groupSessionResults(results ?? [])
  const pictures = sessionResultPictures(groups)
  const guide = guideSections(groups)
  // Every band's wrapping strip sits in the SAME column, so one measurement
  // (the root's content width) fits the whole page.
  const containerRef = useRef<HTMLDivElement>(null)
  const width = useContentWidth(containerRef)
  const height = width
    ? sessionResultTileHeightFitting(pictures, width)
    : SESSION_RESULT_TILE_HEIGHT
  const body = (group: SessionResultGroup) => (
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
      <GuideFileList
        paths={group.files ?? []}
        files={files}
        isMobile={isMobile}
        onOpenFile={onOpenFile}
      />
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
      {guide.lead && (
        <div className="flex flex-col" data-testid="guide-lead">
          {body(guide.lead)}
        </div>
      )}
      {guide.sections.map(({ group, index, total }) => (
        // Keyed by position too: a workflow concatenates several runs'
        // groups, so one topic may band twice.
        <div key={`${index}/${group.topic}`} className="flex flex-col">
          <GuideSectionHeader
            label={group.topic}
            index={numbered ? index : undefined}
            total={numbered ? total : undefined}
          />
          <div className="flex flex-col pt-2">{body(group)}</div>
        </div>
      ))}
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
