import { useEffect, useLayoutEffect, useRef, useState, type ComponentProps, type CSSProperties, type SyntheticEvent } from "react"
import { Maximize2Icon, MinusIcon, PlusIcon } from "lucide-react"
import {
  ZOOM,
  zoomAtRest,
  zoomBounds,
  zoomCentre,
  zoomClamp,
  zoomDoubleTap,
  zoomDrag,
  zoomIsTall,
  zoomKey,
  zoomMenu,
  zoomMomentum,
  zoomMomentumActive,
  zoomPanRoom,
  zoomPercent,
  zoomPinch,
  zoomResize,
  zoomRestView,
  zoomScroll,
  zoomStep,
  zoomTo,
  zoomWheel,
  type ZoomInitial,
  type ZoomMenuItem,
  type ZoomSize,
} from "@exponential_app/eui"
import {
  ZOOMABLE_IMAGE_CLASSES,
  ZOOMABLE_IMAGE_TRANSITION,
  createZoomController,
  zoomBoxAspect,
  zoomDeclaredSize,
  zoomImageStyle,
  type ZoomControls,
  type ZoomController,
  type ZoomMath,
  type ZoomPillState,
} from "@exponential_app/eui/web/primitives"

import { Button } from "./button"
import { cn } from "./cn"
import { DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuTrigger } from "./dropdown-menu"
import { useMediaQuery } from "./use-media-query"

export type { ZoomControls, ZoomPillState } from "@exponential_app/eui/web/primitives"
export type { ZoomInitial } from "@exponential_app/eui"

/** The core's zoom functions, injected into the shared controller. */
export const ZOOM_MATH: ZoomMath = {
  ZOOM,
  zoomIsTall,
  zoomRestView,
  zoomClamp,
  zoomAtRest,
  zoomTo,
  zoomCentre,
  zoomWheel,
  zoomScroll,
  zoomStep,
  zoomDoubleTap,
  zoomKey,
  zoomDrag,
  zoomPinch,
  zoomMomentum,
  zoomMomentumActive,
  zoomPanRoom,
  zoomResize,
  zoomPercent,
  zoomMenu,
  zoomBounds,
}

const REST_PILL: ZoomPillState = { percent: zoomPercent(1), atMin: false, atMax: false, atRest: true }

export interface ZoomableImageOptions {
  /** The picture as drawn (the native passes the host's media url). */
  src: string | undefined
  naturalWidth?: number
  naturalHeight?: number
  initial: ZoomInitial
  maxScale?: number
  /** The bound / controlled scale; undefined = the rest view. */
  scale?: number
  controls: ZoomControls
  /** The settle spring's CSS transition; null = snap. */
  transition: string | null
  onZoom?: (scale: number, atFit: boolean) => void
  onTap?: () => void
}

/** EXP-1586: the React binding of the shared zoom controller, used by the
 *  `ZoomableImage` primitive and the catalog native alike. */
export function useZoomableImage(o: ZoomableImageOptions) {
  const rootRef = useRef<HTMLDivElement | null>(null)
  const imageRef = useRef<HTMLImageElement | null>(null)
  const pillRef = useRef<HTMLDivElement | null>(null)
  const controller = useRef<ZoomController | null>(null)
  const [loaded, setLoaded] = useState<ZoomSize | null>(null)
  const [pill, setPill] = useState<ZoomPillState>(REST_PILL)
  const [pillVisible, setPillVisible] = useState(true)
  const declared = zoomDeclaredSize(o.naturalWidth, o.naturalHeight)
  const natural = declared ?? loaded
  const latest = useRef({ ...o, natural })
  latest.current = { ...o, natural }
  useLayoutEffect(() => {
    const root = rootRef.current
    const image = imageRef.current
    if (!root || !image) return
    const c = createZoomController({
      math: ZOOM_MATH,
      root,
      image,
      pill: () => pillRef.current,
      natural: () => latest.current.natural,
      initial: () => latest.current.initial,
      maxScale: () => latest.current.maxScale,
      boundScale: () => latest.current.scale,
      transition: () => latest.current.transition,
      autoHide: () => latest.current.controls === `auto`,
      onPill: setPill,
      onPillVisible: setPillVisible,
      onZoom: (scale, atFit) => latest.current.onZoom?.(scale, atFit),
      onTap: () => latest.current.onTap?.(),
    })
    controller.current = c
    return () => {
      c.destroy()
      controller.current = null
    }
  }, [])
  // A new picture forgets the loaded size.
  useLayoutEffect(() => setLoaded(null), [o.src])
  // A new picture, size, opening or limit: the rest view, at once.
  useLayoutEffect(() => controller.current?.reset(), [o.src, natural?.width, natural?.height, o.initial, o.maxScale])
  useLayoutEffect(() => controller.current?.syncScale(), [o.scale])
  const onImageLoad = (e: SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget
    if (!declared && img.naturalWidth > 0 && img.naturalHeight > 0) setLoaded({ width: img.naturalWidth, height: img.naturalHeight })
  }
  const menuItems = (fitLabel: string): ZoomMenuItem[] => {
    const c = controller.current
    const f = c?.frame()
    return c && f ? zoomMenu(c.view(), f, fitLabel) : []
  }
  return { rootRef, imageRef, pillRef, controller, natural, pill, pillVisible, onImageLoad, menuItems }
}

/** The pill's labels (catalog/strings.json; override per host). */
export interface ZoomableImageStrings {
  zoomIn: string
  zoomOut: string
  zoomToFit: string
  zoomLevel: string
  zoomFit: string
}

export const ZOOMABLE_IMAGE_STRINGS: ZoomableImageStrings = { zoomIn: `Zoom in`, zoomOut: `Zoom out`, zoomToFit: `Zoom to fit`, zoomLevel: `Zoom level`, zoomFit: `Fit` }

export type ZoomableImageProps = Omit<ComponentProps<`div`>, `children` | `onTap`> & {
  src: string
  alt: string
  naturalWidth?: number
  naturalHeight?: number
  /** auto = fitWidth for a tall picture, else fit. */
  initial?: ZoomInitial
  maxScale?: number
  controls?: ZoomControls
  /** 1 = 100%; undefined = the rest view. */
  scale?: number
  onScaleChange?: (scale: number) => void
  /** A settled zoom change. */
  onZoom?: (zoom: { scale: number; atFit: boolean }) => void
  /** A single tap / click at rest. */
  onTap?: () => void
  strings?: Partial<ZoomableImageStrings>
}

/** EXP-1586: the Exponential UI `ZoomableImage` native as a primitive: a
 *  picture that zooms (pinch, ⌘/ctrl-wheel, double-click, + − 0 1) and pans
 *  (drag, scroll) inside its box, with THE zoom pill. */
export function ZoomableImage({ src, alt, naturalWidth, naturalHeight, initial = `auto`, maxScale, controls = `auto`, scale, onScaleChange, onZoom, onTap, strings, className, style, ...props }: ZoomableImageProps) {
  const reducedMotion = useMediaQuery(`(prefers-reduced-motion: reduce)`)
  const t = { ...ZOOMABLE_IMAGE_STRINGS, ...strings }
  const zoom = useZoomableImage({
    src,
    naturalWidth,
    naturalHeight,
    initial,
    maxScale,
    scale,
    controls,
    transition: reducedMotion ? null : ZOOMABLE_IMAGE_TRANSITION,
    onZoom: (s, atFit) => {
      onScaleChange?.(s)
      onZoom?.({ scale: s, atFit })
    },
    onTap,
  })
  const [menuOpen, setMenuOpen] = useState(false)
  useEffect(() => zoom.controller.current?.holdPill(menuOpen), [menuOpen])
  const aspect = zoomBoxAspect(ZOOM_MATH, zoom.natural, style?.height !== undefined)
  const rootStyle: CSSProperties = { ...(aspect !== null ? { aspectRatio: String(aspect) } : {}), ...style }
  const { pill } = zoom
  return (
    <div ref={zoom.rootRef} data-slot="zoomable-image" role="group" aria-label={alt || undefined} tabIndex={0} className={cn(ZOOMABLE_IMAGE_CLASSES.root, className)} style={rootStyle} {...props}>
      <img ref={zoom.imageRef} data-slot="zoomable-image-image" src={src || undefined} alt={alt} draggable={false} loading="lazy" decoding="async" className={ZOOMABLE_IMAGE_CLASSES.image} style={zoomImageStyle(zoom.natural)} onLoad={zoom.onImageLoad} />
      {controls !== `none` ? (
        <div ref={zoom.pillRef} data-slot="zoomable-image-pill" data-state={zoom.pillVisible ? `open` : `closed`} role="toolbar" className={cn(ZOOMABLE_IMAGE_CLASSES.pill, !zoom.pillVisible && ZOOMABLE_IMAGE_CLASSES.pillHidden)}>
          <Button type="button" variant="ghost" size="icon-sm" className={ZOOMABLE_IMAGE_CLASSES.button} aria-label={t.zoomOut} disabled={pill.atMin} onClick={() => zoom.controller.current?.step(-1)}>
            <MinusIcon />
          </Button>
          <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenuTrigger asChild>
              <Button type="button" variant="ghost" size="sm" className={ZOOMABLE_IMAGE_CLASSES.percent} aria-label={t.zoomLevel}>
                {pill.percent}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent side="top" align="center">
              {(menuOpen ? zoom.menuItems(t.zoomFit) : []).map((item) => (
                <DropdownMenuCheckboxItem
                  key={item.label}
                  checked={item.checked}
                  onSelect={() => {
                    const c = zoom.controller.current
                    if (item.scale === null) c?.rest()
                    else c?.zoomScale(item.scale)
                  }}
                >
                  {item.label}
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <Button type="button" variant="ghost" size="icon-sm" className={ZOOMABLE_IMAGE_CLASSES.button} aria-label={t.zoomIn} disabled={pill.atMax} onClick={() => zoom.controller.current?.step(1)}>
            <PlusIcon />
          </Button>
          <span aria-hidden="true" data-slot="zoomable-image-divider" className={ZOOMABLE_IMAGE_CLASSES.divider} />
          <Button type="button" variant="ghost" size="icon-sm" className={ZOOMABLE_IMAGE_CLASSES.button} aria-label={t.zoomToFit} aria-pressed={pill.atRest} onClick={() => zoom.controller.current?.rest()}>
            <Maximize2Icon />
          </Button>
        </div>
      ) : null}
    </div>
  )
}
