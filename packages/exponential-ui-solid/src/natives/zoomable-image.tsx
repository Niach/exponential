// EXP-1586 (React's natives/zoomable-image.tsx): ZoomableImage — the
// picture through the host's media request, transformed by the shared zoom
// controller (`createZoomController`, every number from the core's
// src/zoom.ts), and THE zoom pill: − / the percent (THE Menu of `zoomMenu`
// through `PressMenu`, on the Menu's own parts) / + / a hairline / fit.
// Until the picture loads (pending or failed) the box keeps its size and
// draws the placeholder (the image glyph + the alt) and NO pill.

import { createSignal, createEffect, on, type Component, type JSX } from "solid-js"
import { MEDIA_INTRINSIC_WIDTH, ZOOM, nativeRecipeProps, resolveRecipe } from "@exponential_app/eui"
import type { UiNode, ZoomInitial } from "@exponential_app/eui"
import { ZOOMABLE_IMAGE_NATIVE_TRANSITION, zoomBoxAspect, zoomableImageLayout, type ZoomControls } from "@exponential_app/eui/web/primitives"
import { useSurfaceContext } from "../context"
import { CHROME, PaintIcon } from "../icons"
import { createMediaSource } from "../media"
import { mergeStyle, type NativeProps } from "../node-view"
import { ZOOM_MATH, createZoomableImage } from "../primitives/zoomable-image"
import { createBoundState } from "./bound"
import { PressMenu, type MenuItem } from "./overlays"
import { BuiltinIcon, css, num, parts, str, type Attrs } from "./shared"

const LAYOUT = zoomableImageLayout(ZOOM.pill)
// One style object per part: a re-applied spread then leaves it alone.
const BUTTON_STYLE = css(LAYOUT.button)
const PERCENT_STYLE = css(LAYOUT.percent)
const DIVIDER_STYLE = css(LAYOUT.divider)
const PILL_STYLE = css(LAYOUT.pill)
const PILL_HIDDEN_STYLE = css({ ...LAYOUT.pill, ...LAYOUT.pillHidden })

const positive = (v: unknown): number | undefined => (typeof v === `number` && Number.isFinite(v) && v > 0 ? v : undefined)

/** React's `{...spread} key={value}`: a later `undefined` REMOVES the key. */
const over = (base: Attrs, extra: Attrs): Attrs => ({ ...base, ...extra })

export function ZoomableImageNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  // THE Menu's parts (its recipe styles the percent button's menu).
  const menuPart = parts({
    get node(): UiNode {
      return { id: `${props.node.id}.menu`, component: `Menu`, props: {}, children: [] }
    },
    props: {},
  })
  const src = () => str(props.props.src)
  const alt = () => str(props.props.alt)
  const controls = () => str(props.props.controls, `auto`) as ZoomControls
  const media = createMediaSource(() => ctx.host, src)
  // `scale` is bindable: a settled zoom writes the bound path.
  const [, setScale] = createBoundState<number | undefined>(() => props.node, () => props.scope, `scale`, () => positive(props.props.scale))
  const zoom = createZoomableImage({
    src: () => media().url,
    naturalWidth: () => positive(props.props.naturalWidth),
    naturalHeight: () => positive(props.props.naturalHeight),
    initial: () => str(props.props.initial, `auto`) as ZoomInitial,
    maxScale: () => positive(props.props.maxScale),
    scale: () => positive(props.props.scale),
    controls,
    transition: () => (ctx.reducedMotion ? null : ZOOMABLE_IMAGE_NATIVE_TRANSITION),
    onZoom: (scale, atFit) => {
      setScale(scale)
      void props.emit(`zoom`, { scale, atFit }, { scale })
    },
    onTap: () => void props.emit(`tap`),
  })
  const [menuOpen, setMenuOpen] = createSignal(false)
  createEffect(on(menuOpen, (open) => zoom.controller()?.holdPill(open), { defer: true }))
  const rootStyle = () => {
    const style = props.node.style
    const styled = style !== undefined && (style.height !== undefined || style.aspectRatio !== undefined)
    const aspect = zoomBoxAspect(ZOOM_MATH, zoom.natural(), styled)
    return mergeStyle(props.rootProps, { ...LAYOUT.root, ...(aspect !== null ? { aspectRatio: String(aspect) } : {}) })
  }
  const items = (): MenuItem[] => (menuOpen() ? zoom.menuItems(ctx.t(`zoomFit`)).map((item, i) => ({ kind: `checkbox` as const, label: item.label, value: String(i), checked: item.checked })) : [])
  const choose = (event: string, payload?: Record<string, unknown>) => {
    if (event !== `select`) return
    const c = zoom.controller()
    const picked = c && c.frame() ? zoom.menuItems(ctx.t(`zoomFit`))[num(payload?.value, -1)] : undefined
    setMenuOpen(false)
    if (!c || !picked) return
    if (picked.scale === null) c.rest()
    else c.zoomScale(picked.scale)
  }
  const imageStyle = () => {
    const n = zoom.natural()
    return css({ ...LAYOUT.image, ...(n ? { width: n.width, height: n.height } : {}), ...(zoom.ready() ? {} : LAYOUT.imagePending) })
  }
  // The placeholder takes the picture's radius (the `image` recipe).
  const placeholderStyle = () => {
    const radius = resolveRecipe(ctx.theme, { component: `ZoomableImage`, part: `image`, props: nativeRecipeProps(`ZoomableImage`, props.props, ctx.extensionDefs) }, ctx.mode).borderRadius
    return radius !== undefined ? css({ borderRadius: radius as number }) : undefined
  }
  return (
    <div {...over(props.rootProps, { style: rootStyle(), role: `group`, "aria-label": alt() || undefined, tabindex: 0 })} ref={zoom.rootRef}>
      <img {...part(`image`)} ref={zoom.imageRef} src={media().url || undefined} alt={alt()} draggable={false} loading="lazy" decoding="async" style={imageStyle()} onLoad={zoom.onImageLoad} />
      <svg class="xui-media-sizer" aria-hidden="true" width={MEDIA_INTRINSIC_WIDTH} height={1} />
      {zoom.ready() ? null : (
        <div class="xui-zoom-placeholder" aria-hidden="true" style={placeholderStyle()}>
          <PaintIcon icon={CHROME.image} aria-hidden="true" />
          {alt() ? <span>{alt()}</span> : null}
        </div>
      )}
      {controls() !== `none` && zoom.ready() ? (
        <div {...part(`pill`)} ref={zoom.pillRef} role="toolbar" data-state={zoom.pillVisible() ? `open` : `closed`} style={zoom.pillVisible() ? PILL_STYLE : PILL_HIDDEN_STYLE}>
          <button type="button" {...part(`zoomOut`, zoom.pill().atMin && `disabled`)} style={BUTTON_STYLE} aria-label={ctx.t(`zoomOut`)} disabled={zoom.pill().atMin} onClick={() => zoom.controller()?.step(-1)}>
            <BuiltinIcon slot="ZoomableImage.zoomOut" size={16} />
          </button>
          <PressMenu
            part={menuPart}
            emit={choose}
            scope={props.scope}
            items={items()}
            raws={[]}
            open={menuOpen}
            setOpen={setMenuOpen}
            side="top"
            align="center"
            trigger={(t) => (
              <button ref={t.ref} {...t.attrs()} {...part(`percent`, menuOpen() && `open`)} style={PERCENT_STYLE} aria-label={ctx.t(`zoomLevel`)} onPointerDown={t.onPointerDown} onKeyDown={t.onKeyDown}>
                {zoom.pill().percent}
              </button>
            )}
          />
          <button type="button" {...part(`zoomIn`, zoom.pill().atMax && `disabled`)} style={BUTTON_STYLE} aria-label={ctx.t(`zoomIn`)} disabled={zoom.pill().atMax} onClick={() => zoom.controller()?.step(1)}>
            <BuiltinIcon slot="ZoomableImage.zoomIn" size={16} />
          </button>
          <span {...part(`divider`)} aria-hidden="true" style={DIVIDER_STYLE} />
          <button type="button" {...part(`fit`, zoom.pill().atRest && `selected`)} style={BUTTON_STYLE} aria-label={ctx.t(`zoomToFit`)} aria-pressed={zoom.pill().atRest ? `true` : `false`} onClick={() => zoom.controller()?.rest()}>
            <BuiltinIcon slot="ZoomableImage.fit" size={16} />
          </button>
        </div>
      ) : null}
    </div>
  )
}

export const NATIVES_ZOOMABLE_IMAGE: Record<string, Component<NativeProps>> = {
  ZoomableImage: ZoomableImageNative,
}
