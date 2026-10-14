import type { ReactNode } from "react"
import type { GuideViewerState } from "@exp/domain-contract/guide-viewer"
import { Button } from "./button"
import { conceptIcon } from "./icons.generated"
import { ImagePreviewDialog, type PreviewNaturalSize } from "./image-preview-dialog"
import { Pill } from "./pill"

// EXP-1283: THE Guide picture viewer ×4 (`guideViewer`, fixture
// `session-results.json` `viewer`): tapping a Guide picture opens it next to
// what it shows — the topic, the picture's label, the topic's GFM text —
// with `‹ index / count ›` through every Guide picture (no wrap), ←/→/Esc.
// Wide: the picture left, a 360px panel right; phones: a full-screen page,
// the picture on top, the panel below (a swipe steps). Built on THE
// lightbox (`ImagePreviewDialog` `panel`).

const PrevIcon = conceptIcon(`ui-chevron-left`)
const NextIcon = conceptIcon(`ui-chevron-right`)

export const GUIDE_VIEWER_COPY = {
  previous: `Previous picture`,
  next: `Next picture`,
} as const

/** The panel beside the picture. */
export function GuidePictureViewerPanel({
  state,
  renderText,
  onSelect,
}: {
  state: GuideViewerState
  /** The app's GFM renderer; plain pre-wrapped prose without one. */
  renderText?: (text: string) => ReactNode
  onSelect: (attachmentId: string) => void
}) {
  return (
    <div data-testid="guide-viewer-panel" className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-6 pt-6 pb-4 sm:pr-12">
        <h2 data-testid="guide-viewer-topic" className="text-lg font-semibold text-foreground">
          {state.topic}
        </h2>
        <Pill size="sm" className="self-start" data-testid="guide-viewer-label">
          {state.label}
        </Pill>
        {state.text !== null && (
          <div data-testid="guide-viewer-text" className="text-sm text-foreground/90">
            {renderText ? renderText(state.text) : <p className="whitespace-pre-wrap">{state.text}</p>}
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center justify-between px-4 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={GUIDE_VIEWER_COPY.previous}
          data-testid="guide-viewer-prev"
          disabled={state.prevId === null}
          onClick={() => state.prevId && onSelect(state.prevId)}
        >
          <PrevIcon />
        </Button>
        <span data-testid="guide-viewer-index" className="text-sm text-muted-foreground tabular-nums">
          {state.index} / {state.count}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={GUIDE_VIEWER_COPY.next}
          data-testid="guide-viewer-next"
          disabled={state.nextId === null}
          onClick={() => state.nextId && onSelect(state.nextId)}
        >
          <NextIcon />
        </Button>
      </div>
    </div>
  )
}

export function GuidePictureViewer({
  state,
  src,
  naturalSize,
  renderText,
  onSelect,
  onClose,
}: {
  state: GuideViewerState
  src: string
  naturalSize?: PreviewNaturalSize | null
  renderText?: (text: string) => ReactNode
  /** Step to another Guide picture (by attachment id). */
  onSelect: (attachmentId: string) => void
  onClose: () => void
}) {
  return (
    <ImagePreviewDialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
      src={src}
      alt={state.label}
      label={state.label}
      naturalSize={naturalSize}
      onPrev={state.prevId ? () => onSelect(state.prevId!) : undefined}
      onNext={state.nextId ? () => onSelect(state.nextId!) : undefined}
      panel={<GuidePictureViewerPanel state={state} renderText={renderText} onSelect={onSelect} />}
    />
  )
}
