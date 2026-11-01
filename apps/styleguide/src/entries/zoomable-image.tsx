import { ZOOM_MATH, ZoomableImage, planCardStandIn } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1586 — THE lightbox picture: Exponential UI's `ZoomableImage`
// (catalog style.json `zoomableImage`), the zoom view every lightbox draws
// (`ImagePreviewDialog`, the Guide viewer, the plan page). The desktop twin
// draws the same two specimens: a 1440×900 picture in a 640×400 box with
// the pill always shown, at rest and at 200%.

const NATURAL = { width: 1440, height: 900 }
const BOX = { width: 640, height: 400 }

/** The plan card's stand-in picture (the desktop twin draws the same). */
const PICTURE = planCardStandIn(`att-2`)

/** An island is static markup, so the controller never measures or paints:
 *  each specimen states the view it would paint (the core's own math) and
 *  the pill's percent for it. */
function paintedView(scale: number | null) {
  const frame = { natural: NATURAL, viewport: BOX }
  const rest = ZOOM_MATH.zoomRestView(frame)
  const view = scale === null ? rest : ZOOM_MATH.zoomTo(rest, scale, ZOOM_MATH.zoomCentre(frame), frame)
  return {
    transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
    percent: ZOOM_MATH.zoomPercent(view.scale),
  }
}

/** Tailwind registers its translate vars with @property, which a shadow
 *  tree ignores: the pill's -translate-x-1/2 needs a y to paint. */
export const ZOOM_PILL_ISLAND_CSS = `[data-slot=zoomable-image-pill]{--tw-translate-y:0}`

function Specimen({ id, label, scale }: { id: string; label: string; scale: number | null }) {
  const { transform, percent } = paintedView(scale)
  const at = `[data-zoom-specimen=${id}]`
  const css = `${ZOOM_PILL_ISLAND_CSS}${at} [data-slot=zoomable-image-image]{transform:${transform}}${at} [aria-label="Zoom level"]{font-size:0}${at} [aria-label="Zoom level"]::after{content:"${percent}";font-size:var(--text-sm)}`
  return (
    <div className="flex flex-col gap-2" data-zoom-specimen={id}>
      <style dangerouslySetInnerHTML={{ __html: css }} />
      <span className="text-xs text-muted-foreground">{label}</span>
      <ZoomableImage
        src={PICTURE}
        alt="Specimen"
        naturalWidth={NATURAL.width}
        naturalHeight={NATURAL.height}
        controls="always"
        scale={scale ?? undefined}
        style={{ width: BOX.width, height: BOX.height }}
        className="bg-muted"
      />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `zoomable-image`,
  section: `general`,
  owner: `EXP-1586`,
  title: `Zoomable image`,
  blurb: `THE lightbox picture: Exponential UI's ZoomableImage. It opens fit (never upscaled; a picture taller than 3× its width opens fit-width at its top). ⌘/ctrl-wheel and a trackpad pinch zoom at the cursor, a plain wheel pans once zoomed (and scrolls a tall picture), a drag pans with momentum, a double click toggles rest ↔ max(100 %, 2× rest), + = − 0 1 step, rest and jump to 100 %. The pill (− / the percent menu / + / fit) floats bottom-centre and hides when idle. Phones: pinch, double tap, a tap toggles the pill; the host's swipe pages only at rest. In a lightbox ←/→ still page (each picture opens at rest) and Esc closes.`,
  status: {
    web: { state: `ok`, symbol: `ZoomableImage`, file: `packages/exponential-ui-react/src/primitives/zoomable-image.tsx`, note: `in ImagePreviewDialog (packages/ui/src/image-preview-dialog.tsx)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/zoomable_image.rs` },
    ios: { state: `ok`, symbol: `ZoomableImage`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Paint/ZoomableImage.swift` },
    android: { state: `ok`, symbol: `ZoomableImage`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/ZoomableImage.kt` },
  },
  island: () => (
    <div className="flex flex-col gap-6">
      <Specimen id="fit" label="Fit" scale={null} />
      <Specimen id="zoomed" label="200%" scale={2} />
    </div>
  ),
}
