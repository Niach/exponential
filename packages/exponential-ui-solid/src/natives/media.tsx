// FEED-128 (React's natives/media.tsx; VAPP-87): Image, Video,
// AudioPlayer, Avatar, Carousel. Every src goes through the host's media
// request and the media policy (`createMediaSource` / `createMediaSrc`).
// Avatar paints Radix Avatar's DOM by hand (a span root, the <img> only once
// the picture LOADED, the fallback after its delay), Kobalte's Image would
// paint its own attributes.

import { For, Show, createComputed, createRenderEffect, createEffect, createMemo, createSignal, on, onCleanup, type Component, type JSX } from "solid-js"
import { MEDIA_ASPECT_RATIO, MEDIA_INTRINSIC_WIDTH } from "@exponential_app/eui"
import type { UiNode } from "@exponential_app/eui"
import { mediaClock } from "@exponential_app/eui/web"
import { AVATAR_PALETTE, avatarFallbackColors, avatarHueIndex, avatarHueKey, avatarInitials } from "@exponential_app/eui/web/primitives"
import { useSurfaceContext } from "../context"
import { CHROME, IconGlyph, PaintIcon } from "../icons"
import { createMediaSource, createMediaSrc } from "../media"
import { NodeView, mergeStyle, withClass, type NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, parts, str, type Attrs } from "./shared"
import { createImageLoadingStatus } from "../behaviors"

/** The media box's intrinsic width (catalog/layout.json
 *  `mediaIntrinsicWidth`: max-content 320, min-content 0), so the box never
 *  waits for the picture or the browser's 300×150 video default. */
function MediaSizer(): JSX.Element {
  return <svg class="xui-media-sizer" aria-hidden="true" width={MEDIA_INTRINSIC_WIDTH} height={1} />
}

/** React's `{...spread} key={value}`: a later `undefined` REMOVES the key
 *  (Solid's merge would keep the spread's value). */
const over = (base: Attrs, extra: Attrs): Attrs => ({ ...base, ...extra })

/** Image (contract §3): `loading` lazy|eager, `focalX/focalY` (0..1 →
 *  `object-position`), and the `fallback` icon (default
 *  `builtinIcons["Image.fallback"]`) on error or without a src;
 *  `data-state` = loading | loaded | error | empty. */
export function ImageNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const src = () => str(props.props.src)
  const alt = () => str(props.props.alt)
  const fit = () => str(props.props.fit, `cover`)
  const [state, setState] = createSignal<`loading` | `loaded` | `error`>(`loading`)
  createComputed(on(src, () => setState(`loading`), { defer: true }))
  const style = createMemo(() => {
    const p = props.props
    const out: Record<string, string | number> = {}
    if (p.width !== undefined) out.width = num(p.width)
    if (p.height !== undefined) out.height = num(p.height)
    // Round 2 §7: `aspectRatio`, else 16:9 without a height.
    if (p.aspectRatio !== undefined) out.aspectRatio = String(num(p.aspectRatio))
    else if (p.height === undefined) out.aspectRatio = String(MEDIA_ASPECT_RATIO)
    return Object.keys(out).length ? out : undefined
  })
  const objectFit = () => (fit() === `scaleDown` ? `scale-down` : fit())
  const fx = () => Math.max(0, Math.min(1, num(props.props.focalX, 0.5)))
  const fy = () => Math.max(0, Math.min(1, num(props.props.focalY, 0.5)))
  // The host's media request (VAPP-91: headers → a blob url once fetched);
  // a failed fetch is the Image's error state (fallback + alt).
  const media = createMediaSource(() => ctx.host, src)
  const shown = () => (media().error ? `error` : state())
  const failed = () => !src() || shown() === `error`
  const fallback = () => str(props.props.fallback)
  return (
    <div {...over(props.rootProps, { style: mergeStyle(props.rootProps, style()), role: failed() ? `img` : undefined, "aria-label": failed() ? alt() : undefined, "data-state": src() ? shown() : `empty` })}>
      <Show when={src() && media().url && shown() !== `error`}>
        <img
          src={media().url}
          alt={alt()}
          loading={props.props.loading === `eager` ? `eager` : `lazy`}
          decoding="async"
          style={css({ objectFit: objectFit(), objectPosition: `${fx() * 100}% ${fy() * 100}%` })}
          onLoad={() => setState(`loaded`)}
          onError={() => setState(`error`)}
        />
      </Show>
      <MediaSizer />
      <Show when={failed()}>
        <div {...part(`fallback`)} aria-hidden="true">
          <Show when={fallback()} fallback={<BuiltinIcon slot="Image.fallback" />}>
            <IconGlyph icons={ctx.host.icons} name={fallback()} />
          </Show>
        </div>
      </Show>
    </div>
  )
}

/** Video (×4 since EXP-1556, React's): the poster (or a dark tint) with the
 *  round play button and the `durationMs` clock; the press (or `autoplay`)
 *  swaps in the browser's player with its controls, playing. */
export function VideoNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  // The video streams (no byte caps); its poster is an image (the limits hold).
  const url = createMediaSrc(() => ctx.host, () => str(props.props.src), `stream`)
  const posterUrl = createMediaSrc(() => ctx.host, () => str(props.props.poster))
  // Round 2 §7: height = width / `aspectRatio` (default 16:9).
  const ratio = () => num(props.props.aspectRatio, MEDIA_ASPECT_RATIO) || MEDIA_ASPECT_RATIO
  const autoplay = () => bool(props.props.autoplay)
  const [started, setStarted] = createSignal(false)
  const durationMs = () => (props.props.durationMs !== undefined ? num(props.props.durationMs) : undefined)
  return (
    <div {...props.rootProps} style={mergeStyle(props.rootProps, { aspectRatio: String(ratio()) })}>
      <MediaSizer />
      <Show
        when={started() || autoplay()}
        fallback={
          <>
            <Show when={posterUrl()}>
              <img class="xui-video-poster" src={posterUrl()} alt="" aria-hidden="true" decoding="async" draggable={false} />
            </Show>
            <button type="button" class="xui-media-play" data-size="lg" aria-label={ctx.t(`play`)} disabled={!url()} onClick={() => setStarted(true)}>
              <PaintIcon icon={CHROME.play} aria-hidden="true" />
            </button>
            <Show when={durationMs() !== undefined}>
              <span class="xui-video-duration">{mediaClock(durationMs()!)}</span>
            </Show>
          </>
        }
      >
        <video
          {...part(`controls`)}
          src={url()}
          poster={posterUrl()}
          controls
          autoplay
          ref={(el) => createRenderEffect(() => (el.muted = autoplay() && !started()))}
          playsinline
          preload="metadata"
          data-duration-ms={durationMs()}
        />
      </Show>
    </div>
  )
}

/** AudioPlayer (×4 since EXP-1556, React's): the title over a bar of the
 *  round play/pause button, the progress track and `position / duration`;
 *  a hidden `<audio>` plays. */
export function AudioPlayerNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const title = () => str(props.props.title)
  const url = createMediaSrc(() => ctx.host, () => str(props.props.src), `stream`)
  let audio: HTMLAudioElement | undefined
  const [playing, setPlaying] = createSignal(false)
  const [position, setPosition] = createSignal(0)
  const [loadedMs, setLoadedMs] = createSignal<number | null>(null)
  const durationMs = () => (props.props.durationMs !== undefined ? num(props.props.durationMs) : (loadedMs() ?? 0))
  const toggle = () => {
    if (!audio) return
    if (audio.paused) void audio.play()?.catch(() => setPlaying(false))
    else audio.pause()
  }
  return (
    <div {...props.rootProps} style={mergeStyle(props.rootProps, { flexDirection: `column`, gap: `var(--xui-spacing-xs)` })}>
      <Show when={title()}>
        <span {...part(`track`)}>{title()}</span>
      </Show>
      <div {...part(`controls`)} data-duration-ms={props.props.durationMs !== undefined ? num(props.props.durationMs) : undefined}>
        <button type="button" class="xui-media-play" data-size="sm" aria-label={ctx.t(playing() ? `pause` : `play`)} disabled={!url()} onClick={toggle}>
          <PaintIcon icon={playing() ? CHROME.pause : CHROME.play} aria-hidden="true" />
        </button>
        <span class="xui-audio-progress" aria-hidden="true">
          <span style={{ width: `${durationMs() > 0 ? Math.min(100, (position() / durationMs()) * 100) : 0}%` }} />
        </span>
        <span class="xui-audio-clock">{`${mediaClock(position())} / ${mediaClock(durationMs())}`}</span>
      </div>
      <audio
        ref={audio}
        src={url()}
        preload="metadata"
        aria-label={title() || undefined}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onTimeUpdate={(e) => setPosition(e.currentTarget.currentTime * 1000)}
        onLoadedMetadata={(e) => setLoadedMs(Number.isFinite(e.currentTarget.duration) ? e.currentTarget.duration * 1000 : null)}
      />
    </div>
  )
}

/** #EXP-1629 (fixtures/avatar.json): the hue key (#EXP-1754: the seed, a
 *  user id, else the trimmed name, which may be an email) picks
 *  `AVATAR_PALETTE[fnv1a32(key) % 8]`, the hue at 20% behind initials in
 *  it, the app's own avatar; no key = the recipe's muted fallback. */
export function avatarTint(seed: string, name?: string): { backgroundColor: string; color: string } | undefined {
  const key = avatarHueKey(seed, name)
  return key === undefined ? undefined : avatarFallbackColors(AVATAR_PALETTE[avatarHueIndex(key)]!)
}

export function AvatarNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const name = () => str(props.props.name)
  const src = () => str(props.props.src)
  const url = createMediaSrc(() => ctx.host, src)
  const tint = () => {
    const colors = avatarTint(str(props.props.seed), name())
    return colors ? css(colors) : undefined
  }
  // AvatarPrimitive.Image is only mounted with a src.
  const status = createImageLoadingStatus(() => (src() ? url() : undefined))
  const imageStatus = () => (src() ? status() : `idle`)
  // AvatarPrimitive.Fallback `delayMs={src ? 300 : 0}`: a 0 delay paints at
  // once (Radix's 0 ms timer has fired by the first frame).
  const delay = () => (src() ? 300 : 0)
  const [canRender, setCanRender] = createSignal(false)
  createComputed(on(delay, (ms) => ms === 0 && setCanRender(true)))
  createEffect(
    on(delay, (ms) => {
      if (ms === 0) return
      const timer = setTimeout(() => setCanRender(true), ms)
      onCleanup(() => clearTimeout(timer))
    })
  )
  return (
    <span {...over(props.rootProps, { "aria-label": name() || undefined, role: `img` })}>
      <Show when={src() && imageStatus() === `loaded`}>
        <img {...part(`image`)} src={url()} alt={name()} />
      </Show>
      <Show when={canRender() && imageStatus() !== `loaded`}>
        <span {...part(`fallback`)} style={tint()}>
          {avatarInitials(name())}
        </span>
      </Show>
    </span>
  )
}

/** One thumbnail of the strip (round 6 §3): a button over its picture
 *  (through the host's media request), the empty muted square without one. */
function CarouselThumbnail(props: { attrs: Attrs; src: string; current: boolean; label: string; onPress: () => void }): JSX.Element {
  const ctx = useSurfaceContext()
  const url = createMediaSrc(() => ctx.host, () => props.src)
  // EXP-1556: the placeholder glyph until the picture paints over it.
  const [broken, setBroken] = createSignal<string | null>(null)
  return (
    <button type="button" {...props.attrs} aria-label={props.label} aria-current={props.current ? `true` : undefined} data-current={props.current ? `true` : undefined} onClick={() => props.onPress()}>
      <span class="xui-carousel-thumb-fallback" aria-hidden="true">
        <BuiltinIcon slot="Image.fallback" />
      </span>
      <Show when={props.src && url() && broken() !== url()}>
        <img class="xui-carousel-thumb" src={url()} alt="" aria-hidden="true" loading="lazy" decoding="async" draggable={false} onError={() => setBroken(url() ?? null)} />
      </Show>
    </button>
  )
}

export function CarouselNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  let track: HTMLDivElement | undefined
  const pages = () => props.node.children
  const external = () => num(props.props.page, 0)
  const [page, setPage] = createBoundState(() => props.node, () => props.scope, `page`, external)
  const scrollTo = (i: number) => {
    const el = track
    if (el && el.children[i] && typeof el.scrollTo === `function`) el.scrollTo({ left: (el.children[i] as HTMLElement).offsetLeft, behavior: ctx.reducedMotion ? `auto` : `smooth` })
  }
  createEffect(on(external, (p) => scrollTo(p)))
  const onScroll = () => {
    const el = track
    if (!el || el.clientWidth === 0) return
    const next = Math.round(Math.abs(el.scrollLeft) / el.clientWidth)
    if (next !== page()) {
      setPage(next)
      void props.emit(`change`, { page: next }, { page: next })
    }
  }
  const go = (next: number) => {
    const n = pages().length
    if (n === 0) return
    const target = bool(props.props.loop) ? ((next % n) + n) % n : Math.max(0, Math.min(n - 1, next))
    setPage(target)
    void props.emit(`change`, { page: target }, { page: target })
    scrollTo(target)
  }
  const many = () => pages().length > 1
  const byId = createMemo(() => new Map(pages().map((n, i) => [n.id, { node: n, index: i }])))
  const ids = createMemo(() => pages().map((n) => n.id), undefined, { equals: (a, b) => a.length === b.length && a.every((x, i) => x === b[i]) })
  const pageOf = (i: number) => ctx.t(`pageOf`, { page: i + 1, total: pages().length })
  // EXP-1495 (round 6 §3): the counter pill and the thumbnail strip, both
  // only with two or more pages.
  const counter = () => many() && bool(props.props.counter)
  const thumbnails = createMemo(() => (many() && Array.isArray(props.props.thumbnails) ? (props.props.thumbnails as unknown[]) : null))
  let strip: HTMLDivElement | undefined
  createEffect(() => {
    // The current thumbnail scrolled into view (the strip only: never the
    // page around it).
    const p = page()
    thumbnails()
    const el = strip
    const thumb = el?.children[p] as HTMLElement | undefined
    if (!el || !thumb) return
    const a = el.getBoundingClientRect()
    const b = thumb.getBoundingClientRect()
    const delta = b.left < a.left ? b.left - a.left : b.right > a.right ? b.right - a.right : 0
    if (delta !== 0) el.scrollLeft += delta
  })
  const trackView = (
    <div ref={track} class="xui-carousel-track" onScroll={onScroll} aria-live="polite">
      <For each={ids()}>
        {(id) => {
          const entry = createMemo<{ node: UiNode; index: number }>((prev) => byId().get(id) ?? prev!)
          // Round 2 §7: only the ACTIVE page is placed; the others stay
          // mounted for swiping, hidden from AT, inert, `data-xui-inactive`.
          // `inert` as the ATTRIBUTE (React 19 writes `inert=""`; Solid's
          // `inert={true}` sets only the property).
          const active = () => entry().index === page()
          const attrs = () => {
            const a = { ...part(`page`, active() && `selected`) }
            if (!active()) delete a[`data-xui-id`]
            return a
          }
          return (
            <div {...attrs()} role="group" aria-roledescription={ctx.t(`slide`)} aria-label={pageOf(entry().index)} aria-hidden={active() ? undefined : true} data-xui-inactive={active() ? undefined : ``} ref={(el) => createRenderEffect(() => el.toggleAttribute(`inert`, !active()))}>
              <NodeView node={entry().node} />
            </div>
          )
        }}
      </For>
    </div>
  )
  return (
    <div
      {...props.rootProps}
      role="region"
      aria-roledescription={ctx.t(`carousel`)}
      onKeyDown={(e) => {
        const rtl = ctx.direction === `rtl`
        if (e.key === `ArrowRight`) go(page() + (rtl ? -1 : 1))
        else if (e.key === `ArrowLeft`) go(page() + (rtl ? 1 : -1))
      }}
    >
      <Show when={counter()} fallback={trackView}>
        <div class="xui-carousel-stage">
          {trackView}
          <span {...part(`counter`)} aria-hidden="true">
            {ctx.t(`pageCounter`, { page: page() + 1, total: pages().length })}
          </span>
        </div>
      </Show>
      <Show when={thumbnails()}>
        {(urls) => (
          <div ref={strip} {...part(`thumbnails`)}>
            <For each={ids()}>
              {(id) => {
                const index = () => byId().get(id)?.index ?? 0
                const current = () => index() === page()
                const src = () => {
                  const u = urls()[index()]
                  return typeof u === `string` ? u : ``
                }
                return <CarouselThumbnail attrs={part.at(`thumbnail`, index(), current() && `selected`)} src={src()} current={current()} label={pageOf(index())} onPress={() => go(index())} />
              }}
            </For>
          </div>
        )}
      </Show>
      <Show when={many()}>
        <div {...withClass(part(`controls`), `xui-carousel-controls`)}>
          <button type="button" {...withClass(part(`previous`), `xui-carousel-nav`)} aria-label={ctx.t(`previous`)} disabled={!bool(props.props.loop) && page() === 0} onClick={() => go(page() - 1)}>
            <BuiltinIcon slot="Carousel.previous" />
          </button>
          <Show when={props.props.indicators !== false}>
            <div {...part(`indicator`)} class="xui-carousel-dots" data-xui-part="Carousel/indicators" role="tablist">
              <For each={ids()}>
                {(id) => {
                  const index = () => byId().get(id)?.index ?? 0
                  const attrs = () => {
                    const a = { ...part(`indicator`, index() === page() && `selected`) }
                    delete a[`data-xui-id`]
                    return a
                  }
                  return <button type="button" {...attrs()} role="tab" aria-selected={index() === page()} aria-label={pageOf(index())} onClick={() => go(index())} />
                }}
              </For>
            </div>
          </Show>
          <button type="button" {...withClass(part(`next`), `xui-carousel-nav`)} aria-label={ctx.t(`next`)} disabled={!bool(props.props.loop) && page() === pages().length - 1} onClick={() => go(page() + 1)}>
            <BuiltinIcon slot="Carousel.next" />
          </button>
        </div>
      </Show>
    </div>
  )
}

export const NATIVES_MEDIA: Record<string, Component<NativeProps>> = {
  Image: ImageNative,
  Video: VideoNative,
  AudioPlayer: AudioPlayerNative,
  Avatar: AvatarNative,
  Carousel: CarouselNative,
}
