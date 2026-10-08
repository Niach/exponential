// VAPP-87: Image, Video, AudioPlayer, Avatar, Carousel.

import { useEffect, useRef, useState } from "react"
import { Avatar as AvatarPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { CHROME } from "../icons"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { bool, num, str, useParts } from "./shared"

export function ImageNative({ props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const src = str(props.src)
  const alt = str(props.alt)
  const fit = str(props.fit, `cover`)
  const style: Record<string, string | number> = {}
  if (props.width !== undefined) style.width = num(props.width)
  if (props.height !== undefined) style.height = num(props.height)
  if (props.aspectRatio !== undefined) style.aspectRatio = String(num(props.aspectRatio))
  const objectFit = fit === `scaleDown` ? `scale-down` : fit
  return (
    <div {...(rootProps as Record<string, unknown>)} style={Object.keys(style).length ? style : undefined} role={src ? undefined : `img`} aria-label={src ? undefined : alt}>
      {src ? <img src={ctx.host.resolveUrl ? ctx.host.resolveUrl(src) : src} alt={alt} style={{ objectFit: objectFit as never }} /> : <div className="xui-image-placeholder">{alt || `image`}</div>}
    </div>
  )
}

export function VideoNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const src = str(props.src)
  const poster = str(props.poster)
  const resolve = (u: string) => (ctx.host.resolveUrl ? ctx.host.resolveUrl(u) : u)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <video {...(part(`controls`) as Record<string, string>)} src={src ? resolve(src) : undefined} poster={poster ? resolve(poster) : undefined} controls autoPlay={bool(props.autoplay)} muted={bool(props.autoplay)} playsInline preload="metadata" data-duration-ms={props.durationMs !== undefined ? num(props.durationMs) : undefined} />
    </div>
  )
}

export function AudioPlayerNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const src = str(props.src)
  const title = str(props.title)
  return (
    <div {...(rootProps as Record<string, unknown>)} style={{ flexDirection: `column`, gap: `var(--xui-spacing-xs)` }}>
      {title ? <span {...(part(`track`) as Record<string, string>)}>{title}</span> : null}
      <audio {...(part(`controls`) as Record<string, string>)} src={src ? (ctx.host.resolveUrl ? ctx.host.resolveUrl(src) : src) : undefined} controls preload="metadata" aria-label={title || undefined} data-duration-ms={props.durationMs !== undefined ? num(props.durationMs) : undefined} />
    </div>
  )
}

function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!)
    .join(``)
    .toUpperCase()
}

/** A stable hue for a seed (the app's avatar palette is host-side; the SDK
 *  tints with one hue so two avatars of one person always match). */
export function seedHue(seed: string): number {
  let h = 0
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0
  return h % 360
}

export function AvatarNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const name = str(props.name)
  const src = str(props.src)
  const seed = str(props.seed) || name
  const hue = seed ? seedHue(seed) : null
  const tint = hue === null ? undefined : { backgroundColor: `color-mix(in oklab, hsl(${hue} 70% 55%) 22%, transparent)`, color: `hsl(${hue} 60% ${ctx.mode === `dark` ? 72 : 38}%)` }
  return (
    <AvatarPrimitive.Root {...(rootProps as Record<string, unknown>)} aria-label={name || undefined} role="img">
      {src ? <AvatarPrimitive.Image {...(part(`image`) as Record<string, string>)} src={ctx.host.resolveUrl ? ctx.host.resolveUrl(src) : src} alt={name} /> : null}
      <AvatarPrimitive.Fallback {...(part(`fallback`) as Record<string, string>)} delayMs={src ? 300 : 0} style={tint}>
        {initials(name) || `?`}
      </AvatarPrimitive.Fallback>
    </AvatarPrimitive.Root>
  )
}

export function CarouselNative({ node, props, rootProps, emit }: NativeProps) {
  const part = useParts(node, props)
  const track = useRef<HTMLDivElement>(null)
  const external = num(props.page, 0)
  const [page, setPage] = useState(external)
  const pages = node.children
  useEffect(() => {
    setPage(external)
    const el = track.current
    if (el && el.children[external] && typeof el.scrollTo === `function`) el.scrollTo({ left: (el.children[external] as HTMLElement).offsetLeft, behavior: `smooth` })
  }, [external])
  const onScroll = () => {
    const el = track.current
    if (!el || el.clientWidth === 0) return
    const next = Math.round(el.scrollLeft / el.clientWidth)
    if (next !== page) {
      setPage(next)
      void emit(`change`, { page: next })
    }
  }
  const go = (next: number) => {
    const el = track.current
    const n = pages.length
    const target = bool(props.loop) ? ((next % n) + n) % n : Math.max(0, Math.min(n - 1, next))
    setPage(target)
    void emit(`change`, { page: target })
    if (el && el.children[target] && typeof el.scrollTo === `function`) el.scrollTo({ left: (el.children[target] as HTMLElement).offsetLeft, behavior: `smooth` })
  }
  const Dot = `button` as const
  return (
    <div {...(rootProps as Record<string, unknown>)} role="region" aria-roledescription="carousel">
      <div ref={track} className="xui-carousel-track" onScroll={onScroll}>
        {pages.map((child, i) => (
          <div key={child.id} {...(part(`page`, i === page && `selected`) as Record<string, string>)} role="group" aria-roledescription="slide" aria-label={`${i + 1} of ${pages.length}`}>
            <NodeView node={child} />
          </div>
        ))}
      </div>
      {props.indicators !== false && pages.length > 1 ? (
        <div className="xui-carousel-dots" role="tablist">
          {pages.map((child, i) => (
            <Dot key={child.id} type="button" {...(part(`indicator`, i === page && `selected`) as Record<string, string>)} role="tab" aria-selected={i === page} aria-label={`Page ${i + 1}`} onClick={() => go(i)} />
          ))}
        </div>
      ) : null}
      <span className="xui-sr-only">
        <CHROME.chevronLeft aria-hidden="true" />
      </span>
    </div>
  )
}
