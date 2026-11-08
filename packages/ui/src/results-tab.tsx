import { useState, type ReactNode } from "react"
import type { CardSurface } from "@exp/domain-contract/plan-card"
import { RESULTS_TAB, RESULTS_TAB_SPECIMEN, type Result, type ResultPicture } from "@exp/domain-contract/results-tab"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { conceptIcon } from "./icons.generated"
import { ImagePreviewDialog } from "./image-preview-dialog"
import { useSpecimenAttachmentSrc } from "./specimen-samples"
import { SurfaceCard } from "./surface-card"

// #FEED-191 RESULTS TAB ×web (fixtures/results-tab.json; desktop
// domain::results_tab, iOS ExpCore ResultsTab.swift, Android
// domain/ResultsTab.kt): the session screen's last tab. The model is the
// contract's `resultsTab()`; these only paint it.
//
//   ResultsTab     the PR block slot (`lead`), then the results INLINE
//                  `tile.listGap` apart, else THE EmptyState; a picture opens
//                  THE lightbox over that result's pictures at its index
//   ResultSection  line 1 the title + age (no badge), the description, then
//                  ONE row of thumbnails with a muted caption-or-label under
//                  each, else THE surface card inline

const ResultsIcon = conceptIcon(`chat-results`)
const TILE = RESULTS_TAB.tile

/** `wide` = web md+ (and the IDE), `phone` = web-mobile (and the natives). */
export type ResultsTabSize = `wide` | `phone`

/** A thumbnail's width (px): by the picture's aspect at `tile.thumbHeight`,
 *  capped at `tile.thumbMaxWidth`; an unknown size = 4:3. */
export function resultThumbWidth(picture: Pick<ResultPicture, `width` | `height`>, size: ResultsTabSize): number {
  const aspect = picture.width && picture.height ? picture.width / picture.height : 4 / 3
  return Math.min(TILE.thumbMaxWidth[size], Math.round(TILE.thumbHeight[size] * aspect))
}

const pictureText = (picture: ResultPicture) => picture.caption ?? picture.label

export interface ResultBodyRenderers {
  /** The result's surface (its raw card's), null = none. */
  surfaceFor?: (result: Result) => CardSurface | null
  /** A surface press's reply text (THE chat's `surfaceReply`). */
  onSurfaceReply?: (result: Result, text: string) => void
}

export function ResultSection({
  result,
  size,
  attachmentSrc,
  onOpenPicture,
  surfaceFor,
  onSurfaceReply,
}: {
  result: Result
  size: ResultsTabSize
  attachmentSrc: (attachmentId: string) => string
  onOpenPicture?: (index: number) => void
} & ResultBodyRenderers) {
  // Pictures win, else the surface.
  const surface = result.pictures.length === 0 && result.surface ? (surfaceFor?.(result) ?? null) : null
  return (
    <div
      data-testid={RESULTS_TAB.testIds.section}
      data-result={result.id}
      className="flex min-w-0 flex-col"
      style={{ gap: TILE.gap }}
    >
      <div className="flex min-w-0 flex-col" style={{ gap: TILE.textGap }}>
        <div className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-sm font-semibold text-foreground">{result.title}</span>
          <span className="shrink-0 text-xs text-muted-foreground">{result.age}</span>
        </div>
        {result.description && (
          <span data-slot="result-description" className="text-sm text-muted-foreground">
            {result.description}
          </span>
        )}
      </div>
      {result.pictures.length > 0 && (
        <div
          data-slot="result-pictures"
          className="flex min-w-0 overflow-x-auto overscroll-x-contain [scrollbar-width:none]"
          style={{ gap: TILE.gap }}
        >
          {result.pictures.map((picture, index) => {
            const width = resultThumbWidth(picture, size)
            return (
              <button
                key={`${picture.attachmentId}-${index}`}
                type="button"
                data-slot="result-picture"
                className="flex shrink-0 flex-col gap-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                style={{ width }}
                onClick={() => onOpenPicture?.(index)}
              >
                <img
                  src={attachmentSrc(picture.attachmentId)}
                  alt={pictureText(picture)}
                  draggable={false}
                  className="block border border-glass-stroke bg-muted object-cover object-top"
                  style={{ width, height: TILE.thumbHeight[size], borderRadius: TILE.radius }}
                />
                <span className="truncate text-xs text-muted-foreground">{pictureText(picture)}</span>
              </button>
            )
          })}
        </div>
      )}
      {surface && (
        <div data-slot="result-surface" className="min-w-0">
          <SurfaceCard
            surface={surface}
            testKey={`tab-${result.id}`}
            onReply={onSurfaceReply ? (reply) => onSurfaceReply(result, reply) : undefined}
          />
        </div>
      )}
    </div>
  )
}

/** THE Results tab body (`resultsTab()`; none = the empty state). */
export function ResultsTab({
  results,
  size,
  lead,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  surfaceFor,
  onSurfaceReply,
  className,
}: {
  results: readonly Result[]
  size: ResultsTabSize
  /** The PR block (the Stack band + StackRail, the Changes row). */
  lead?: ReactNode
  attachmentSrc?: (attachmentId: string) => string
  className?: string
} & ResultBodyRenderers) {
  // THE lightbox over one result's pictures.
  const [open, setOpen] = useState<{ resultId: string; index: number } | null>(null)
  const pictures = results.find((result) => result.id === open?.resultId)?.pictures ?? []
  const shown = open ? (pictures[open.index] ?? null) : null
  return (
    <div
      data-testid={RESULTS_TAB.testIds.list}
      className={cn(`flex min-w-0 flex-col`, className)}
      style={{ gap: TILE.leadGap }}
    >
      {lead}
      {results.length > 0 ? (
        <div className="flex min-w-0 flex-col" style={{ gap: TILE.listGap }}>
          {results.map((result) => (
            <ResultSection
              key={result.id}
              result={result}
              size={size}
              attachmentSrc={attachmentSrc}
              onOpenPicture={(index) => setOpen({ resultId: result.id, index })}
              surfaceFor={surfaceFor}
              onSurfaceReply={onSurfaceReply}
            />
          ))}
        </div>
      ) : (
        <EmptyState icon={ResultsIcon} title={RESULTS_TAB.copy.empty} size="page" />
      )}
      <ImagePreviewDialog
        open={shown !== null}
        onOpenChange={(next) => (next ? undefined : setOpen(null))}
        src={shown ? attachmentSrc(shown.attachmentId) : ``}
        alt={shown ? pictureText(shown) : ``}
        label={shown ? pictureText(shown) : ``}
        naturalSize={shown?.width && shown.height ? { width: shown.width, height: shown.height } : null}
        onPrev={open && open.index > 0 ? () => setOpen({ ...open, index: open.index - 1 }) : undefined}
        onNext={open && open.index < pictures.length - 1 ? () => setOpen({ ...open, index: open.index + 1 }) : undefined}
      />
    </div>
  )
}

/** THE styleguide specimen ×4 (`tab.specimen`): the tab body, no PR block; its
 *  pictures are the committed samples (EXP-1654), never fetched. */
export function ResultsTabSpecimen({ width = 640, size = `wide` }: { width?: number; size?: ResultsTabSize }) {
  const attachmentSrc = useSpecimenAttachmentSrc()
  return (
    <div
      data-testid="results-tab-specimen"
      className="rounded-3xl border border-glass-stroke-card bg-background p-5 max-md:rounded-none max-md:border-0 max-md:p-0"
      style={{ width, maxWidth: `100%` }}
    >
      <ResultsTab results={RESULTS_TAB_SPECIMEN} size={size} attachmentSrc={attachmentSrc} />
    </div>
  )
}
