// VAPP-87: Image, Video, AudioPlayer, Avatar, Carousel.

import { useEffect, useRef, useState } from "react"
import { useBoundState } from "./bound"
import { Avatar as AvatarPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { NodeView, mergeStyle } from "../node-view"
import { bool, BuiltinIcon, num, str, useParts } from "./shared"
import { useMediaSource, useMediaSrc } from "../media"

/** Image (contract §3): `loading` lazy|eager, `focalX/focalY` (0..1, the
 *  point kept when the picture is cropped → `object-position`), and the
 *  `fallback` icon (default `builtinIcons["Image.fallback"]`) on error or
 *  without a src; `data-state` = loading | loaded | error | empty. */
export function ImageNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const src = str(props.src)
  const alt = str(props.alt)
  const fit = str(props.fit, `cover`)
  const [state, setState] = useState<`loading` | `loaded` | `error`>(`loading`)
  useEffect(() => setState(`loading`), [src])
  const style: Record<string, string | number> = {}
  if (props.width !== undefined) style.width = num(props.width)
  if (props.height !== undefined) style.height = num(props.height)
  if (props.aspectRatio !== undefined) style.aspectRatio = String(num(props.aspectRatio))
  const objectFit = fit === `scaleDown` ? `scale-down` : fit
  const fx = Math.max(0, Math.min(1, num(props.focalX, 0.5)))
  const fy = Math.max(0, Math.min(1, num(props.focalY, 0.5)))
  // The host's media request (VAPP-91: headers → a blob url once fetched);
  // a failed fetch is the Image's error state (fallback + alt).
  const media = useMediaSource(ctx.host, src)
  const url = media.url
  const shown = media.error ? `error` : state
  const failed = !src || shown === `error`
  const fallback = str(props.fallback)
  return (
    <div {...(rootProps as Record<string, unknown>)} style={mergeStyle(rootProps, Object.keys(style).length ? style : undefined)} role={failed ? `img` : undefined} aria-label={failed ? alt : undefined} data-state={src ? shown : `empty`}>
      {src && url && shown !== `error` ? (
        <img
          src={url}
          alt={alt}
          loading={props.loading === `eager` ? `eager` : `lazy`}
          decoding="async"
          style={{ objectFit: objectFit as never, objectPosition: `${fx * 100}% ${fy * 100}%` }}
          onLoad={() => setState(`loaded`)}
          onError={() => setState(`error`)}
        />
      ) : null}
      {failed ? (
        <div {...(part(`fallback`) as Record<string, string>)} aria-hidden="true">
          {fallback ? <IconGlyph icons={ctx.host.icons} name={fallback} /> : <BuiltinIcon slot="Image.fallback" />}
        </div>
      ) : null}
    </div>
  )
}

export function VideoNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const src = str(props.src)
  const poster = str(props.poster)
  const url = useMediaSrc(ctx.host, src)
  const posterUrl = useMediaSrc(ctx.host, poster)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <video {...(part(`controls`) as Record<string, string>)} src={url} poster={posterUrl} controls autoPlay={bool(props.autoplay)} muted={bool(props.autoplay)} playsInline preload="metadata" data-duration-ms={props.durationMs !== undefined ? num(props.durationMs) : undefined} />
    </div>
  )
}

export function AudioPlayerNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const src = str(props.src)
  const title = str(props.title)
  const url = useMediaSrc(ctx.host, src)
  return (
    <div {...(rootProps as Record<string, unknown>)} style={mergeStyle(rootProps, { flexDirection: `column`, gap: `var(--xui-spacing-xs)` })}>
      {title ? <span {...(part(`track`) as Record<string, string>)}>{title}</span> : null}
      <audio {...(part(`controls`) as Record<string, string>)} src={url} controls preload="metadata" aria-label={title || undefined} data-duration-ms={props.durationMs !== undefined ? num(props.durationMs) : undefined} />
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
  const url = useMediaSrc(ctx.host, src)
  const tint = hue === null ? undefined : { backgroundColor: `color-mix(in oklab, hsl(${hue} 70% 55%) 22%, transparent)`, color: `hsl(${hue} 60% ${ctx.mode === `dark` ? 72 : 38}%)` }
  return (
    <AvatarPrimitive.Root {...(rootProps as Record<string, unknown>)} aria-label={name || undefined} role="img">
      {src ? <AvatarPrimitive.Image {...(part(`image`) as Record<string, string>)} src={url} alt={name} /> : null}
      <AvatarPrimitive.Fallback {...(part(`fallback`) as Record<string, string>)} delayMs={src ? 300 : 0} style={tint}>
        {initials(name) || `?`}
      </AvatarPrimitive.Fallback>
    </AvatarPrimitive.Root>
  )
}

export function CarouselNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const track = useRef<HTMLDivElement>(null)
  const pages = node.children
  const [page, setPage] = useBoundState(node, scope, `page`, num(props.page, 0))
  const scrollTo = (i: number) => {
    const el = track.current
    if (el && el.children[i] && typeof el.scrollTo === `function`) el.scrollTo({ left: (el.children[i] as HTMLElement).offsetLeft, behavior: ctx.reducedMotion ? `auto` : `smooth` })
  }
  const external = num(props.page, 0)
  useEffect(() => scrollTo(external), [external])
  const onScroll = () => {
    const el = track.current
    if (!el || el.clientWidth === 0) return
    const next = Math.round(Math.abs(el.scrollLeft) / el.clientWidth)
    if (next !== page) {
      setPage(next)
      void emit(`change`, { page: next })
    }
  }
  const go = (next: number) => {
    const n = pages.length
    if (n === 0) return
    const target = bool(props.loop) ? ((next % n) + n) % n : Math.max(0, Math.min(n - 1, next))
    setPage(target)
    void emit(`change`, { page: target })
    scrollTo(target)
  }
  const many = pages.length > 1
  return (
    <div
      {...(rootProps as Record<string, unknown>)}
      role="region"
      aria-roledescription="carousel"
      onKeyDown={(e) => {
        const rtl = ctx.direction === `rtl`
        if (e.key === `ArrowRight`) go(page + (rtl ? -1 : 1))
        else if (e.key === `ArrowLeft`) go(page + (rtl ? 1 : -1))
      }}
    >
      <div ref={track} className="xui-carousel-track" onScroll={onScroll} aria-live="polite">
        {pages.map((child, i) => (
          <div key={child.id} {...(part(`page`, i === page && `selected`) as Record<string, string>)} role="group" aria-roledescription="slide" aria-label={ctx.t(`pageOf`, { page: i + 1, total: pages.length })} aria-hidden={i === page ? undefined : true}>
            <NodeView node={child} />
          </div>
        ))}
      </div>
      {many ? (
        <div className="xui-carousel-controls">
          <button type="button" className="xui-carousel-nav" aria-label={ctx.t(`previous`)} disabled={!bool(props.loop) && page === 0} onClick={() => go(page - 1)}>
            <BuiltinIcon slot="Carousel.previous" />
          </button>
          {props.indicators !== false ? (
            <div className="xui-carousel-dots" role="tablist">
              {pages.map((child, i) => (
                <button key={child.id} type="button" {...(part(`indicator`, i === page && `selected`) as Record<string, string>)} role="tab" aria-selected={i === page} aria-label={ctx.t(`pageOf`, { page: i + 1, total: pages.length })} onClick={() => go(i)} />
              ))}
            </div>
          ) : null}
          <button type="button" className="xui-carousel-nav" aria-label={ctx.t(`next`)} disabled={!bool(props.loop) && page === pages.length - 1} onClick={() => go(page + 1)}>
            <BuiltinIcon slot="Carousel.next" />
          </button>
        </div>
      ) : null}
    </div>
  )
}
