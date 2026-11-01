// EXP-1586: ZoomableImage (catalog/style.json `zoomableImage`): the picture
// through the host's media request, transformed by the shared zoom controller
// (@exponential_app/eui/web/primitives `createZoomController`, every number
// from the core's src/zoom.ts), and THE zoom pill: − / the percent (THE Menu
// of `zoomMenu`, on the Menu's own parts and recipe) / + / a hairline / fit.
// The parts' look is the theme recipe; the layout styles only place them.

import { useEffect, useMemo, useState } from "react"
import { DropdownMenu as MenuPrimitive } from "radix-ui"
import { MEDIA_INTRINSIC_WIDTH, OVERLAY_OFFSET, OVERLAY_PADDING, ZOOM } from "@exponential_app/eui"
import type { UiNode, ZoomInitial } from "@exponential_app/eui"
import { ZOOMABLE_IMAGE_NATIVE_TRANSITION, zoomBoxAspect, zoomableImageLayout, type ZoomControls } from "@exponential_app/eui/web/primitives"
import { useSurfaceContext } from "../context"
import { useMediaSource } from "../media"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"
import { ZOOM_MATH, useZoomableImage } from "../primitives/zoomable-image"
import { useBoundState } from "./bound"
import { MenuItems, type MenuItem } from "./overlays"
import { BuiltinIcon, num, str, useParts } from "./shared"

const LAYOUT = zoomableImageLayout(ZOOM.pill)

const positive = (v: unknown): number | undefined => (typeof v === `number` && Number.isFinite(v) && v > 0 ? v : undefined)

export function ZoomableImageNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  // THE Menu's parts (its recipe styles the percent button's menu).
  const menuNode = useMemo<UiNode>(() => ({ id: `${node.id}.menu`, component: `Menu`, props: {}, children: [] }), [node.id])
  const menuPart = useParts(menuNode, menuNode.props)
  const src = str(props.src)
  const alt = str(props.alt)
  const controls = str(props.controls, `auto`) as ZoomControls
  const media = useMediaSource(ctx.host, src)
  // `scale` is bindable: a settled zoom writes the bound path.
  const [, setScale] = useBoundState<number | undefined>(node, scope, `scale`, positive(props.scale))
  const zoom = useZoomableImage({
    src: media.url,
    naturalWidth: positive(props.naturalWidth),
    naturalHeight: positive(props.naturalHeight),
    initial: str(props.initial, `auto`) as ZoomInitial,
    maxScale: positive(props.maxScale),
    scale: positive(props.scale),
    controls,
    transition: ctx.reducedMotion ? null : ZOOMABLE_IMAGE_NATIVE_TRANSITION,
    onZoom: (scale, atFit) => {
      setScale(scale)
      void emit(`zoom`, { scale, atFit }, { scale })
    },
    onTap: () => void emit(`tap`),
  })
  const [menuOpen, setMenuOpen] = useState(false)
  useEffect(() => zoom.controller.current?.holdPill(menuOpen), [menuOpen])
  const styled = node.style !== undefined && (node.style.height !== undefined || node.style.aspectRatio !== undefined)
  const aspect = zoomBoxAspect(ZOOM_MATH, zoom.natural, styled)
  const items: MenuItem[] = menuOpen ? zoom.menuItems(ctx.t(`zoomFit`)).map((item, i) => ({ kind: `checkbox`, label: item.label, value: String(i), checked: item.checked })) : []
  const choose = (event: string, payload?: Record<string, unknown>) => {
    if (event !== `select`) return
    const c = zoom.controller.current
    const f = c?.frame()
    const picked = c && f ? zoom.menuItems(ctx.t(`zoomFit`))[num(payload?.value, -1)] : undefined
    setMenuOpen(false)
    if (!c || !picked) return
    if (picked.scale === null) c.rest()
    else c.zoomScale(picked.scale)
  }
  const { pill } = zoom
  return (
    <div
      {...(rootProps as Record<string, unknown>)}
      ref={zoom.rootRef}
      role="group"
      aria-label={alt || undefined}
      tabIndex={0}
      style={mergeStyle(rootProps, { ...LAYOUT.root, ...(aspect !== null ? { aspectRatio: String(aspect) } : {}) })}
    >
      <img {...(part(`image`) as Record<string, string>)} ref={zoom.imageRef} src={media.url || undefined} alt={alt} draggable={false} loading="lazy" decoding="async" style={{ ...LAYOUT.image, ...(zoom.natural ? { width: zoom.natural.width, height: zoom.natural.height } : {}) }} onLoad={zoom.onImageLoad} />
      <svg className="xui-media-sizer" aria-hidden="true" width={MEDIA_INTRINSIC_WIDTH} height={1} />
      {controls !== `none` ? (
        <div {...(part(`pill`) as Record<string, string>)} ref={zoom.pillRef} role="toolbar" data-state={zoom.pillVisible ? `open` : `closed`} style={zoom.pillVisible ? LAYOUT.pill : { ...LAYOUT.pill, ...LAYOUT.pillHidden }}>
          <button type="button" {...(part(`zoomOut`, pill.atMin && `disabled`) as Record<string, string>)} style={LAYOUT.button} aria-label={ctx.t(`zoomOut`)} disabled={pill.atMin} onClick={() => zoom.controller.current?.step(-1)}>
            <BuiltinIcon slot="ZoomableImage.zoomOut" size={16} />
          </button>
          <MenuPrimitive.Root open={menuOpen} onOpenChange={setMenuOpen} dir={ctx.direction}>
            <MenuPrimitive.Trigger asChild>
              <button type="button" {...(part(`percent`, menuOpen && `open`) as Record<string, string>)} style={LAYOUT.percent} aria-label={ctx.t(`zoomLevel`)}>
                {pill.percent}
              </button>
            </MenuPrimitive.Trigger>
            <MenuPrimitive.Portal container={ctx.portal ?? undefined}>
              <MenuPrimitive.Content {...(menuPart(`content`, menuOpen && `open`) as Record<string, string>)} side="top" align="center" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Menu" data-open-on="press">
                <MenuItems ns={MenuPrimitive} items={items} rawItems={undefined} part={menuPart} overlay="Menu" emit={choose} scope={scope} path="" close={() => setMenuOpen(false)} />
              </MenuPrimitive.Content>
            </MenuPrimitive.Portal>
          </MenuPrimitive.Root>
          <button type="button" {...(part(`zoomIn`, pill.atMax && `disabled`) as Record<string, string>)} style={LAYOUT.button} aria-label={ctx.t(`zoomIn`)} disabled={pill.atMax} onClick={() => zoom.controller.current?.step(1)}>
            <BuiltinIcon slot="ZoomableImage.zoomIn" size={16} />
          </button>
          <span {...(part(`divider`) as Record<string, string>)} aria-hidden="true" style={LAYOUT.divider} />
          <button type="button" {...(part(`fit`, pill.atRest && `selected`) as Record<string, string>)} style={LAYOUT.button} aria-label={ctx.t(`zoomToFit`)} aria-pressed={pill.atRest} onClick={() => zoom.controller.current?.rest()}>
            <BuiltinIcon slot="ZoomableImage.fit" size={16} />
          </button>
        </div>
      ) : null}
    </div>
  )
}
