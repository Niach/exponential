// VAPP-87: Image, Video, AudioPlayer, Avatar, Carousel.

import { useEffect, useRef, useState } from "react"
import { useBoundState } from "./bound"
import { Avatar as AvatarPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { CHROME, IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import type { HostPlugin } from "../host"
import { NodeView, mergeStyle, withClass } from "../node-view"
import { MEDIA_ASPECT_RATIO, MEDIA_INTRINSIC_WIDTH } from "@exponential_app/eui"
import { mediaClock } from "@exponential_app/eui/web"
import { AVATAR_PALETTE, avatarFallbackColors, avatarHueIndex, avatarInitials } from "@exponential_app/eui/web/primitives"

/** The media box's intrinsic width (catalog/layout.json
 *  `mediaIntrinsicWidth`: max-content 320, min-content 0 — a compressible
 *  replaced element with `max-width: 100%`), so the box never waits for the
 *  picture or the browser's 300×150 video default. */
function MediaSizer() {
  return <svg className="xui-media-sizer" aria-hidden="true" width={MEDIA_INTRINSIC_WIDTH} height={1} />
}
import { bool, BuiltinIcon, num, str, useParts } from "./shared"
import { useMediaSource, useMediaSrc } from "../media"

/** `inert` on an inactive page (React 19, the peer range, takes a boolean). */
const INERT: Record<string, unknown> = { inert: true }

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
  // Round 2 §7: `aspectRatio`, else 16:9 without a height (the box never
  // waits for the picture: the img is laid over it).
  if (props.aspectRatio !== undefined) style.aspectRatio = String(num(props.aspectRatio))
  else if (props.height === undefined) style.aspectRatio = String(MEDIA_ASPECT_RATIO)
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
      <MediaSizer />
      {failed ? (
        <div {...(part(`fallback`) as Record<string, string>)} aria-hidden="true">
          {fallback ? <IconGlyph icons={ctx.host.icons} name={fallback} /> : <BuiltinIcon slot="Image.fallback" />}
        </div>
      ) : null}
    </div>
  )
}

type PartOf = (name: string) => Record<string, unknown>
/** What a media face needs besides its props: the host (the media loader)
 *  and the strings; a surface's natives and a Markdown outside a surface
 *  (#VAPP-258, an app's own text) hand them in alike. */
export interface MediaFaceProps {
  host: HostPlugin
  t: (id: string) => string
  rootProps: Record<string, unknown>
  part: PartOf
  src: string
  durationMs?: number
}

/** Video (×4 since EXP-1556): the poster (or a dark tint) with the round
 *  play button and the `durationMs` clock, as desktop, iOS and Android draw
 *  it; the press (or `autoplay`) swaps in the browser's player with its
 *  controls, playing. */
export function VideoFace({ host, t, rootProps, part, src, durationMs, poster = ``, autoplay = false, aspectRatio }: MediaFaceProps & { poster?: string; autoplay?: boolean; aspectRatio?: number }) {
  const [started, setStarted] = useState(false)
  // The video streams (no byte caps); its poster is an image (the limits hold).
  const url = useMediaSrc(host, src, `stream`)
  const posterUrl = useMediaSrc(host, poster)
  // Round 2 §7: height = width / `aspectRatio` (default 16:9), never the
  // browser's 300×150.
  const ratio = aspectRatio || MEDIA_ASPECT_RATIO
  const Play = CHROME.play
  return (
    <div {...rootProps} style={mergeStyle(rootProps, { aspectRatio: String(ratio) })}>
      <MediaSizer />
      {started || autoplay ? (
        <video {...(part(`controls`) as Record<string, string>)} src={url} poster={posterUrl} controls autoPlay muted={autoplay && !started} playsInline preload="metadata" data-duration-ms={durationMs} />
      ) : (
        <>
          {posterUrl ? <img className="xui-video-poster" src={posterUrl} alt="" aria-hidden="true" decoding="async" draggable={false} /> : null}
          <button type="button" className="xui-media-play" data-size="lg" aria-label={t(`play`)} disabled={!url} onClick={() => setStarted(true)}>
            <Play aria-hidden="true" />
          </button>
          {durationMs !== undefined ? <span className="xui-video-duration">{mediaClock(durationMs)}</span> : null}
        </>
      )}
    </div>
  )
}

export function VideoNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const durationMs = props.durationMs !== undefined ? num(props.durationMs) : undefined
  return <VideoFace host={ctx.host} t={ctx.t} rootProps={rootProps as Record<string, unknown>} part={part as PartOf} src={str(props.src)} poster={str(props.poster)} autoplay={bool(props.autoplay)} aspectRatio={num(props.aspectRatio, MEDIA_ASPECT_RATIO)} durationMs={durationMs} />
}

/** AudioPlayer (×4 since EXP-1556): the title over a bar of the round
 *  play/pause button, the progress track and `position / duration`, as
 *  desktop, iOS and Android draw it; a hidden `<audio>` plays. */
export function AudioFace({ host, t, rootProps, part, src, durationMs: given, title = `` }: MediaFaceProps & { title?: string }) {
  const url = useMediaSrc(host, src, `stream`)
  const audio = useRef<HTMLAudioElement>(null)
  const [playing, setPlaying] = useState(false)
  const [position, setPosition] = useState(0)
  const [loadedMs, setLoadedMs] = useState<number | null>(null)
  const durationMs = given ?? loadedMs ?? 0
  const Glyph = playing ? CHROME.pause : CHROME.play
  const toggle = () => {
    const el = audio.current
    if (!el) return
    if (el.paused) void el.play()?.catch(() => setPlaying(false))
    else el.pause()
  }
  return (
    <div {...rootProps} style={mergeStyle(rootProps, { flexDirection: `column`, gap: `var(--xui-spacing-xs)` })}>
      {title ? <span {...(part(`track`) as Record<string, string>)}>{title}</span> : null}
      <div {...(part(`controls`) as Record<string, string>)} data-duration-ms={given}>
        <button type="button" className="xui-media-play" data-size="sm" aria-label={t(playing ? `pause` : `play`)} disabled={!url} onClick={toggle}>
          <Glyph aria-hidden="true" />
        </button>
        <span className="xui-audio-progress" aria-hidden="true">
          <span style={{ width: `${durationMs > 0 ? Math.min(100, (position / durationMs) * 100) : 0}%` }} />
        </span>
        <span className="xui-audio-clock">{`${mediaClock(position)} / ${mediaClock(durationMs)}`}</span>
      </div>
      <audio
        ref={audio}
        src={url}
        preload="metadata"
        aria-label={title || undefined}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onTimeUpdate={(e) => setPosition(e.currentTarget.currentTime * 1000)}
        onLoadedMetadata={(e) => setLoadedMs(Number.isFinite(e.currentTarget.duration) ? e.currentTarget.duration * 1000 : null)}
      />
    </div>
  )
}

export function AudioPlayerNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  return <AudioFace host={ctx.host} t={ctx.t} rootProps={rootProps as Record<string, unknown>} part={part as PartOf} src={str(props.src)} title={str(props.title)} durationMs={props.durationMs !== undefined ? num(props.durationMs) : undefined} />
}

/** #EXP-1629 (fixtures/avatar.json): a seed (a user id) picks
 *  `AVATAR_PALETTE[fnv1a32(seed) % 8]`, the hue at 20% behind initials in
 *  it, the app's own avatar; no seed = the recipe's muted fallback. */
export function avatarTint(seed: string): { backgroundColor: string; color: string } | undefined {
  return seed ? avatarFallbackColors(AVATAR_PALETTE[avatarHueIndex(seed)]!) : undefined
}

export function AvatarNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const name = str(props.name)
  const src = str(props.src)
  const url = useMediaSrc(ctx.host, src)
  const tint = avatarTint(str(props.seed))
  return (
    <AvatarPrimitive.Root {...(rootProps as Record<string, unknown>)} aria-label={name || undefined} role="img">
      {src ? <AvatarPrimitive.Image {...(part(`image`) as Record<string, string>)} src={url} alt={name} /> : null}
      {/* No picture to wait for: the initials paint with the first frame
          (Radix holds a 0 ms delay one timer tick; Solid paints at once). */}
      <AvatarPrimitive.Fallback {...(part(`fallback`) as Record<string, string>)} delayMs={src ? 300 : undefined} style={tint}>
        {avatarInitials(name)}
      </AvatarPrimitive.Fallback>
    </AvatarPrimitive.Root>
  )
}

/** One thumbnail of the strip (round 6 §3): a button over its picture
 *  (through the host's media request); EXP-1556: the muted square shows the
 *  Image placeholder glyph until the picture paints over it (none, loading
 *  or failed), never the browser's broken-image icon. */
function CarouselThumbnail({ attrs, src, current, label, onPress }: { attrs: Record<string, unknown>; src: string; current: boolean; label: string; onPress: () => void }) {
  const ctx = useSurfaceContext()
  const url = useMediaSrc(ctx.host, src)
  const [broken, setBroken] = useState<string | null>(null)
  return (
    <button type="button" {...(attrs as Record<string, string>)} aria-label={label} aria-current={current ? `true` : undefined} data-current={current ? `true` : undefined} onClick={onPress}>
      <span className="xui-carousel-thumb-fallback" aria-hidden="true">
        <BuiltinIcon slot="Image.fallback" />
      </span>
      {src && url && broken !== url ? <img className="xui-carousel-thumb" src={url} alt="" aria-hidden="true" loading="lazy" decoding="async" draggable={false} onError={() => setBroken(url)} /> : null}
    </button>
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
      void emit(`change`, { page: next }, { page: next })
    }
  }
  const go = (next: number) => {
    const n = pages.length
    if (n === 0) return
    const target = bool(props.loop) ? ((next % n) + n) % n : Math.max(0, Math.min(n - 1, next))
    setPage(target)
    void emit(`change`, { page: target }, { page: target })
    scrollTo(target)
  }
  const many = pages.length > 1
  // EXP-1495 (round 6 §3): the counter pill and the thumbnail strip, both
  // only with two or more pages.
  const counter = many && bool(props.counter)
  const thumbnails = many && Array.isArray(props.thumbnails) ? (props.thumbnails as unknown[]) : null
  const strip = useRef<HTMLDivElement>(null)
  useEffect(() => {
    // The current thumbnail scrolled into view (the strip only: never the
    // page around it).
    const el = strip.current
    const thumb = el?.children[page] as HTMLElement | undefined
    if (!el || !thumb) return
    const a = el.getBoundingClientRect()
    const b = thumb.getBoundingClientRect()
    const delta = b.left < a.left ? b.left - a.left : b.right > a.right ? b.right - a.right : 0
    if (delta !== 0) el.scrollLeft += delta
  }, [page, thumbnails !== null])
  const trackView = (
    <div ref={track} className="xui-carousel-track" onScroll={onScroll} aria-live="polite">
      {pages.map((child, i) => {
        // Round 2 §7: only the ACTIVE page is placed; the others stay
        // mounted for swiping, hidden from AT, inert, `data-xui-inactive`.
        const active = i === page
        const attrs = part(`page`, active && `selected`) as Record<string, string>
        if (!active) delete attrs[`data-xui-id`]
        return (
          <div key={child.id} {...attrs} role="group" aria-roledescription={ctx.t(`slide`)} aria-label={ctx.t(`pageOf`, { page: i + 1, total: pages.length })} aria-hidden={active ? undefined : true} data-xui-inactive={active ? undefined : ``} {...(active ? {} : INERT)}>
            <NodeView node={child} />
          </div>
        )
      })}
    </div>
  )
  return (
    <div
      {...(rootProps as Record<string, unknown>)}
      role="region"
      aria-roledescription={ctx.t(`carousel`)}
      onKeyDown={(e) => {
        const rtl = ctx.direction === `rtl`
        if (e.key === `ArrowRight`) go(page + (rtl ? -1 : 1))
        else if (e.key === `ArrowLeft`) go(page + (rtl ? 1 : -1))
      }}
    >
      {counter ? (
        <div className="xui-carousel-stage">
          {trackView}
          <span {...(part(`counter`) as Record<string, string>)} aria-hidden="true">
            {ctx.t(`pageCounter`, { page: page + 1, total: pages.length })}
          </span>
        </div>
      ) : (
        trackView
      )}
      {thumbnails ? (
        <div ref={strip} {...(part(`thumbnails`) as Record<string, string>)}>
          {pages.map((child, i) => (
            <CarouselThumbnail key={child.id} attrs={part.at(`thumbnail`, i, i === page && `selected`)} src={typeof thumbnails[i] === `string` ? (thumbnails[i] as string) : ``} current={i === page} label={ctx.t(`pageOf`, { page: i + 1, total: pages.length })} onPress={() => go(i)} />
          ))}
        </div>
      ) : null}
      {many ? (
        <div {...withClass(part(`controls`), `xui-carousel-controls`)}>
          <button type="button" {...withClass(part(`previous`), `xui-carousel-nav`)} aria-label={ctx.t(`previous`)} disabled={!bool(props.loop) && page === 0} onClick={() => go(page - 1)}>
            <BuiltinIcon slot="Carousel.previous" />
          </button>
          {props.indicators !== false ? (
            <div {...(part(`indicator`) as Record<string, string>)} className="xui-carousel-dots" data-xui-part="Carousel/indicators" role="tablist">
              {pages.map((child, i) => (
                <button key={child.id} type="button" {...(part(`indicator`, i === page && `selected`) as Record<string, string>)} data-xui-id={undefined} role="tab" aria-selected={i === page} aria-label={ctx.t(`pageOf`, { page: i + 1, total: pages.length })} onClick={() => go(i)} />
              ))}
            </div>
          ) : null}
          <button type="button" {...withClass(part(`next`), `xui-carousel-nav`)} aria-label={ctx.t(`next`)} disabled={!bool(props.loop) && page === pages.length - 1} onClick={() => go(page + 1)}>
            <BuiltinIcon slot="Carousel.next" />
          </button>
        </div>
      ) : null}
    </div>
  )
}
