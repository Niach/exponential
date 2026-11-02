import { For, Show, createEffect, createSignal, on, onCleanup, onMount, splitProps, type Accessor, type ComponentProps, type JSX } from "solid-js"
import { Maximize2, Minus, Plus } from "lucide-solid"
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

import { PaintIcon } from "../icons"
import { css } from "../style"
import { Button } from "./button"
import { cn } from "./cn"
import { DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuTrigger } from "./dropdown-menu"
import { createMediaQuery } from "./use-media-query"

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

/** Live inputs (getters) of `createZoomableImage`. */
export interface ZoomableImageOptions {
  src: () => string | undefined
  naturalWidth: () => number | undefined
  naturalHeight: () => number | undefined
  initial: () => ZoomInitial
  maxScale: () => number | undefined
  scale: () => number | undefined
  controls: () => ZoomControls
  transition: () => string | null
  onZoom?: (scale: number, atFit: boolean) => void
  onTap?: () => void
}

/** EXP-1586 (React's `useZoomableImage`): the Solid binding of the shared
 *  zoom controller, used by the `ZoomableImage` primitive and the native. */
export function createZoomableImage(o: ZoomableImageOptions) {
  let root: HTMLDivElement | undefined
  let image: HTMLImageElement | undefined
  let pillEl: HTMLDivElement | undefined
  let controller: ZoomController | null = null
  const [loaded, setLoaded] = createSignal<ZoomSize | null>(null)
  // The picture has painted (its `load` fired): until then (pending or
  // failed) the native draws its placeholder and no pill.
  const [ready, setReady] = createSignal(false)
  const [pill, setPill] = createSignal<ZoomPillState>(REST_PILL)
  const [pillVisible, setPillVisible] = createSignal(true)
  const declared = () => zoomDeclaredSize(o.naturalWidth(), o.naturalHeight())
  const natural: Accessor<ZoomSize | null> = () => declared() ?? loaded()
  onMount(() => {
    if (!root || !image) return
    const c = createZoomController({
      math: ZOOM_MATH,
      root,
      image,
      pill: () => pillEl ?? null,
      natural,
      initial: o.initial,
      maxScale: o.maxScale,
      boundScale: o.scale,
      transition: o.transition,
      autoHide: () => o.controls() === `auto`,
      onPill: (p) => setPill(p),
      onPillVisible: setPillVisible,
      onZoom: (scale, atFit) => o.onZoom?.(scale, atFit),
      onTap: () => o.onTap?.(),
    })
    controller = c
    onCleanup(() => {
      c.destroy()
      controller = null
    })
  })
  // A new picture forgets the loaded size; a new picture, size, opening or
  // limit: the rest view, at once.
  createEffect(
    on(
      o.src,
      () => {
        setLoaded(null)
        setReady(false)
      },
      { defer: true }
    )
  )
  createEffect(on([o.src, () => natural()?.width, () => natural()?.height, o.initial, o.maxScale], () => controller?.reset(), { defer: true }))
  createEffect(on(o.scale, () => controller?.syncScale(), { defer: true }))
  const onImageLoad = (e: Event) => {
    const img = e.currentTarget as HTMLImageElement
    setReady(true)
    if (!declared() && img.naturalWidth > 0 && img.naturalHeight > 0) setLoaded({ width: img.naturalWidth, height: img.naturalHeight })
  }
  const menuItems = (fitLabel: string): ZoomMenuItem[] => {
    const f = controller?.frame()
    return controller && f ? zoomMenu(controller.view(), f, fitLabel) : []
  }
  return {
    rootRef: (el: HTMLDivElement) => (root = el),
    imageRef: (el: HTMLImageElement) => (image = el),
    pillRef: (el: HTMLDivElement) => (pillEl = el),
    controller: () => controller,
    natural,
    ready,
    pill,
    pillVisible,
    onImageLoad,
    menuItems,
  }
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

export type ZoomableImageProps = Omit<ComponentProps<`div`>, `children` | `style`> & {
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
  /** React-style inline style (camelCase, numbers = px). */
  style?: Record<string, string | number>
}

/** EXP-1586: React's `ZoomableImage` (the Exponential UI native as a
 *  primitive: zoom + pan + THE zoom pill). */
export function ZoomableImage(props: ZoomableImageProps): JSX.Element {
  const [local, rest] = splitProps(props, [`src`, `alt`, `naturalWidth`, `naturalHeight`, `initial`, `maxScale`, `controls`, `scale`, `onScaleChange`, `onZoom`, `onTap`, `strings`, `class`, `style`])
  const reducedMotion = createMediaQuery(`(prefers-reduced-motion: reduce)`)
  const t = () => ({ ...ZOOMABLE_IMAGE_STRINGS, ...local.strings })
  const controls = () => local.controls ?? `auto`
  const zoom = createZoomableImage({
    src: () => local.src,
    naturalWidth: () => local.naturalWidth,
    naturalHeight: () => local.naturalHeight,
    initial: () => local.initial ?? `auto`,
    maxScale: () => local.maxScale,
    scale: () => local.scale,
    controls,
    transition: () => (reducedMotion() ? null : ZOOMABLE_IMAGE_TRANSITION),
    onZoom: (s, atFit) => {
      local.onScaleChange?.(s)
      local.onZoom?.({ scale: s, atFit })
    },
    onTap: () => local.onTap?.(),
  })
  const [menuOpen, setMenuOpen] = createSignal(false)
  createEffect(on(menuOpen, (open) => zoom.controller()?.holdPill(open), { defer: true }))
  const rootStyle = () => {
    const aspect = zoomBoxAspect(ZOOM_MATH, zoom.natural(), local.style?.height !== undefined)
    return css({ ...(aspect !== null ? { aspectRatio: String(aspect) } : {}), ...local.style })
  }
  return (
    <div ref={zoom.rootRef} data-slot="zoomable-image" role="group" aria-label={local.alt || undefined} tabindex={0} class={cn(ZOOMABLE_IMAGE_CLASSES.root, local.class)} style={rootStyle()} {...rest}>
      <img ref={zoom.imageRef} data-slot="zoomable-image-image" src={local.src || undefined} alt={local.alt} draggable={false} loading="lazy" decoding="async" class={ZOOMABLE_IMAGE_CLASSES.image} style={css(zoomImageStyle(zoom.natural()))} onLoad={zoom.onImageLoad} />
      <Show when={controls() !== `none`}>
        <div ref={zoom.pillRef} data-slot="zoomable-image-pill" data-state={zoom.pillVisible() ? `open` : `closed`} role="toolbar" class={cn(ZOOMABLE_IMAGE_CLASSES.pill, !zoom.pillVisible() && ZOOMABLE_IMAGE_CLASSES.pillHidden)}>
          <Button type="button" variant="ghost" size="icon-sm" class={ZOOMABLE_IMAGE_CLASSES.button} aria-label={t().zoomOut} disabled={zoom.pill().atMin} onClick={() => zoom.controller()?.step(-1)}>
            <PaintIcon icon={Minus} />
          </Button>
          <DropdownMenu open={menuOpen()} onOpenChange={setMenuOpen}>
            <DropdownMenuTrigger as={Button} type="button" variant="ghost" size="sm" class={ZOOMABLE_IMAGE_CLASSES.percent} aria-label={t().zoomLevel}>
              {zoom.pill().percent}
            </DropdownMenuTrigger>
            <DropdownMenuContent side="top" align="center">
              <For each={menuOpen() ? zoom.menuItems(t().zoomFit) : []}>
                {(item) => (
                  <DropdownMenuCheckboxItem
                    checked={item.checked}
                    onSelect={() => {
                      const c = zoom.controller()
                      if (item.scale === null) c?.rest()
                      else c?.zoomScale(item.scale)
                    }}
                  >
                    {item.label}
                  </DropdownMenuCheckboxItem>
                )}
              </For>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button type="button" variant="ghost" size="icon-sm" class={ZOOMABLE_IMAGE_CLASSES.button} aria-label={t().zoomIn} disabled={zoom.pill().atMax} onClick={() => zoom.controller()?.step(1)}>
            <PaintIcon icon={Plus} />
          </Button>
          <span aria-hidden="true" data-slot="zoomable-image-divider" class={ZOOMABLE_IMAGE_CLASSES.divider} />
          <Button type="button" variant="ghost" size="icon-sm" class={ZOOMABLE_IMAGE_CLASSES.button} aria-label={t().zoomToFit} aria-pressed={zoom.pill().atRest ? `true` : `false`} onClick={() => zoom.controller()?.rest()}>
            <PaintIcon icon={Maximize2} />
          </Button>
        </div>
      </Show>
    </div>
  )
}
