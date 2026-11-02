// EXP-1604: the `Motion` native's web half, shared by React, Solid and
// <exponential-surface> (eui-motion README.md › Painting a Motion). The
// framework wrappers render `<span role=img><canvas/></span>` and hand both
// elements to a `MotionView`; it owns everything else:
//
// 1. Source → Lottie: `layers` (the dialect document) → the WASM `compile`;
//    `lottie` as is; `src` fetched under the host's media policy (`.json`, or
//    a `.lottie` archive: its first animation, images inlined).
// 2. Load with slot overrides: every `$color.<token>` slot = the theme's
//    colour for the mode; `$mode: dark`; the node's `slots` last. Reloads on
//    theme / mode / slots changes.
// 3. Clock: the engine's (`setPlayback` + `tick(ms)`); `progress` scrubs,
//    `play: false` freezes; reduced motion, autoplay off, off-screen
//    (IntersectionObserver) or a hidden document → the poster, no ticking;
//    ≤ 60 fps; ≤ 8 animating per surface (tree order); a finished finite
//    loop stops.
// 4. Box: the wrapper sizes it (`motionBoxStyle`); the canvas fills it at
//    devicePixelRatio, the composition fitted (contain | cover | fill).
// 5. Interactivity (M4, EXP-1605, README › Interactivity): the state machine
//    (`stateMachine`, else a `.lottie` src's first) runs IN THE ENGINE; this
//    file only posts interactions / inputs to it (`MotionStateDriver`) and
//    turns its status into the node's `on` events (press, hover, visible,
//    complete, state). The wrappers hand an `emit` (their A2UI action
//    plumbing) and whether `on.press` makes the Motion a button.
//
// The WASM (and its JS glue) is imported lazily on the first Motion that
// paints: never in a host's initial bundle; bundlers emit the .wasm as a
// hashed asset (`new URL(…, import.meta.url)` in the glue).
//
// Without IntersectionObserver or requestAnimationFrame (SSR, jsdom) it
// paints nothing and loads nothing.

// The package root is external to the web entries (like web/media.ts):
// importing ../../theme here would bundle the whole catalog.
import { MEDIA_LIMITS, parseColor, resolveToken, type ModeName, type ResolvedTheme } from "@exponential_app/eui"
import type { HostPlugin } from "../host-types"
import { fetchLimited } from "../media"
import { mediaRequestOf, mediaUrlAllowed } from "../urls"
import { MotionCanvasPool, motionConcat, motionFitMatrix, paintMotionFrame, type MotionFit, type MotionImageSource } from "./motion-paint"

export { paintMotionFrame, motionFitMatrix, motionConcat, motionFrameSize, MotionCanvasPool } from "./motion-paint"
export type { MotionFit, MotionMatrix } from "./motion-paint"

/** At most this many Motions animate per surface (tree order). */
export const MOTION_MAX_PLAYING = 8
/** At most this many frames per second. */
export const MOTION_MAX_FPS = 60

export interface MotionEnv {
  theme: ResolvedTheme
  mode: ModeName
  reducedMotion: boolean
  host: HostPlugin
  /** A load failure (the box stays empty): the wrapper reports it as a paint error. */
  onError?: (message: string) => void
  /** The node's `on` events (the wrapper's A2UI action plumbing). */
  emit?: MotionEmit
  /** `on.press` is set: the Motion is a button (Enter / Space press it). */
  pressable?: boolean
}

/** The events a Motion fires (its `on`); `state` carries `{state}`. */
export type MotionEvent = `press` | `hover` | `visible` | `complete` | `state`
export type MotionEmit = (event: MotionEvent, payload?: Record<string, unknown>) => void

type Props = Record<string, unknown>
type Player = import("../../motion/wasm").MotionBindings[`MotionPlayer`][`prototype`]

const DIALECT_KEYS = [`size`, `fps`, `duration`, `start`, `time`, `name`, `layers`, `assets`, `markers`] as const
const PLAYBACK_KEYS = [`autoplay`, `loop`, `plays`, `direction`, `speed`, `segment`, `marker`, `poster`] as const

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** The composition size a node declares without loading anything (the
 *  dialect's `size`, an inline Lottie's `w`/`h`); null for `src`. */
export function motionDeclaredSize(props: Props): [number, number] | null {
  if (Array.isArray(props.layers)) {
    const s = props.size
    if (Array.isArray(s) && typeof s[0] === `number` && typeof s[1] === `number` && s[0] > 0 && s[1] > 0) return [s[0], s[1]]
    return null
  }
  if (isObj(props.lottie)) {
    const { w, h } = props.lottie
    if (typeof w === `number` && typeof h === `number` && w > 0 && h > 0) return [w, h]
  }
  return null
}

/** Contract §4 (shared with eui-core's layout): an unstyled box is the
 *  composition's px width, shrinking to the available width, aspect kept;
 *  a style width or height wins (the aspect still fills the other edge). */
export function motionBoxStyle(size: [number, number] | null, nodeStyle: unknown): Record<string, string> {
  const style = isObj(nodeStyle) ? nodeStyle : {}
  const hasW = style.width !== undefined
  const hasH = style.height !== undefined
  const out: Record<string, string> = {}
  if (!size) return out
  if (!hasW && !hasH) {
    out.width = `${size[0]}px`
    out.maxWidth = `100%`
  }
  if (!(hasW && hasH)) out.aspectRatio = `${size[0]} / ${size[1]}`
  return out
}

/** The fit prop, defaulted. */
export function motionFit(value: unknown): MotionFit {
  return value === `cover` || value === `fill` ? value : `contain`
}

/** Contract §6: role img named by `label`; hidden (decorative) without one.
 *  M4 §4: with `on.press` (`pressable`) a focusable button named by `label`. */
export function motionA11y(label: unknown, pressable = false): { role?: `img` | `button`; tabIndex?: number; "aria-label"?: string; "aria-hidden"?: `true` } {
  const named = typeof label === `string` && label
  if (pressable) return named ? { role: `button`, tabIndex: 0, "aria-label": label } : { role: `button`, tabIndex: 0 }
  return named ? { role: `img`, "aria-label": label } : { "aria-hidden": `true` }
}

/** The theme's slot overrides for `slotIds` (contract §2). */
export function motionThemeSlots(slotIds: readonly string[], theme: ResolvedTheme, mode: ModeName, own: unknown): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const id of slotIds) {
    if (!id.startsWith(`$color.`)) continue
    const c = parseColor(resolveToken(theme, id, mode))
    if (c) out[id] = { p: { a: 0, k: [c.r / 255, c.g / 255, c.b / 255, c.a] } }
  }
  if (mode === `dark`) out.$mode = `dark`
  if (isObj(own)) Object.assign(out, own)
  return out
}

// --- Surfaces: ≤ 8 animating, tree order -------------------------------------

const surfaces = new Map<Node, Set<MotionView>>()
let hiddenListeners = 0
const hiddenWatchers = new Set<MotionView>()
const onVisibility = () => hiddenWatchers.forEach((v) => v.schedule())

function surfaceOf(el: Element): Node {
  return el.closest(`[data-xui-surface]`) ?? el.ownerDocument
}

function granted(view: MotionView, surface: Node): boolean {
  const set = surfaces.get(surface)
  if (!set) return false
  const live = [...set].filter((v) => v.root.isConnected)
  if (live.length <= MOTION_MAX_PLAYING) return set.has(view)
  live.sort((a, b) => (a.root.compareDocumentPosition(b.root) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1))
  return live.indexOf(view) < MOTION_MAX_PLAYING
}

/** Whether this environment paints Motions at all. */
export function motionCanPaint(): boolean {
  return typeof IntersectionObserver === `function` && typeof requestAnimationFrame === `function` && typeof Path2D === `function`
}

// --- dotLottie (zip) ---------------------------------------------------------

async function inflate(data: Uint8Array, method: number): Promise<Uint8Array> {
  if (method === 0) return data
  if (method !== 8 || typeof DecompressionStream !== `function`) throw new Error(`unsupported .lottie compression`)
  const stream = new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream(`deflate-raw`))
  return new Uint8Array(await new Response(stream).arrayBuffer())
}

/** The entries of a zip archive (central directory). */
async function unzip(bytes: Uint8Array): Promise<Map<string, Uint8Array>> {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let eocd = -1
  for (let k = bytes.length - 22; k >= Math.max(0, bytes.length - 65557); k--) {
    if (dv.getUint32(k, true) === 0x06054b50) {
      eocd = k
      break
    }
  }
  if (eocd < 0) throw new Error(`not a .lottie archive`)
  const count = dv.getUint16(eocd + 10, true)
  let p = dv.getUint32(eocd + 16, true)
  const out = new Map<string, Uint8Array>()
  const dec = new TextDecoder()
  for (let k = 0; k < count; k++) {
    if (dv.getUint32(p, true) !== 0x02014b50) break
    const method = dv.getUint16(p + 10, true)
    const size = dv.getUint32(p + 20, true)
    const nameLen = dv.getUint16(p + 28, true)
    const extraLen = dv.getUint16(p + 30, true)
    const commentLen = dv.getUint16(p + 32, true)
    const local = dv.getUint32(p + 42, true)
    const name = dec.decode(bytes.subarray(p + 46, p + 46 + nameLen))
    const start = local + 30 + dv.getUint16(local + 26, true) + dv.getUint16(local + 28, true)
    if (!name.endsWith(`/`)) out.set(name, await inflate(bytes.subarray(start, start + size), method))
    p += 46 + nameLen + extraLen + commentLen
  }
  return out
}

function base64(bytes: Uint8Array): string {
  let s = ``
  for (let k = 0; k < bytes.length; k += 0x8000) s += String.fromCharCode(...bytes.subarray(k, k + 0x8000))
  return btoa(s)
}

/** A .lottie archive's first animation as Lottie JSON, its images inlined. */
export async function motionFromDotLottie(bytes: Uint8Array): Promise<string> {
  return (await motionDotLottie(bytes)).lottie
}

/** A .lottie archive's first animation (Lottie JSON, images inlined) and its
 *  first state machine (dotLottie 2 `s/<id>.json`, v1 `states/`), or null. */
export async function motionDotLottie(bytes: Uint8Array): Promise<{ lottie: string; stateMachine: Record<string, unknown> | null }> {
  const files = await unzip(bytes)
  const dec = new TextDecoder()
  const manifest = files.get(`manifest.json`) ? (JSON.parse(dec.decode(files.get(`manifest.json`))) as { animations?: { id?: string }[]; stateMachines?: { id?: string }[]; states?: (string | { id?: string })[] }) : {}
  const id = manifest.animations?.[0]?.id
  const anims = [...files.keys()].filter((n) => /^(animations|a)\/[^/]+\.json$/.test(n))
  const name = (id && anims.find((n) => n.endsWith(`/${id}.json`))) || anims[0]
  if (!name) throw new Error(`the .lottie archive has no animation`)
  const lottie = JSON.parse(dec.decode(files.get(name))) as { assets?: Record<string, unknown>[] }
  for (const a of lottie.assets ?? []) {
    const p = a.p
    if (typeof p !== `string` || p.startsWith(`data:`) || a.layers) continue
    const file = files.get(`images/${p}`) ?? files.get(`i/${p}`)
    if (!file) continue
    const ext = p.split(`.`).pop()?.toLowerCase()
    const mime = ext === `jpg` || ext === `jpeg` ? `image/jpeg` : ext === `webp` ? `image/webp` : ext === `svg` ? `image/svg+xml` : `image/png`
    a.p = `data:${mime};base64,${base64(file)}`
    a.u = ``
    a.e = 1
  }
  const first = manifest.stateMachines?.[0]?.id ?? manifest.states?.map((s) => (typeof s === `string` ? s : s.id))[0]
  const machines = [...files.keys()].filter((n) => /^(s|states)\/[^/]+\.json$/.test(n))
  const smName = (first && machines.find((n) => n.endsWith(`/${first}.json`))) || machines[0]
  let stateMachine: Record<string, unknown> | null = null
  if (smName) {
    try {
      const v = JSON.parse(dec.decode(files.get(smName))) as unknown
      if (isObj(v)) stateMachine = v
    } catch {
      // A broken machine: the animation still plays on its props.
    }
  }
  return { lottie: JSON.stringify(lottie), stateMachine }
}

// --- State machines (M4) -----------------------------------------------------

/** The engine's state-machine API (the WASM `MotionPlayer`). */
export interface MotionMachinePlayer {
  setStateMachine(json: string): boolean
  smPost(kind: string, layer: string): boolean
  smSetInputs(json: string): boolean
  smStatus(): string
}

/** A Motion's state machine half (README › Interactivity §1–4). The engine
 *  interprets the machine; this posts interactions and inputs to it, restarts
 *  the clock when the state changes and turns its status into events:
 *  `state` for every entered state (`{state}`), `complete` once per state
 *  when its play finished or it is final. Without a machine it still reports
 *  `complete` for a finished finite play. */
export class MotionStateDriver {
  private machineKey = ``
  private inputsKey = ``
  private completeSent = false
  running = false

  constructor(
    private readonly player: MotionMachinePlayer,
    private readonly emit: MotionEmit,
    /** The state changed: restart the clock at 0. */
    private readonly restart: () => void
  ) {}

  /** Start (or restart, when its JSON changed) the machine; null = none.
   *  Returns whether one runs (its state's playback replaces the props'). */
  setMachine(machine: unknown): boolean {
    const key = isObj(machine) ? JSON.stringify(machine) : ``
    if (key === this.machineKey) return this.running
    this.machineKey = key
    this.inputsKey = ``
    // `{}` parses to no machine: the engine drops the old one.
    this.running = this.player.setStateMachine(key || `{}`)
    if (this.running) this.changed()
    this.drain()
    return this.running
  }

  /** The node's `inputs` (literal or bound): written on every change. */
  setInputs(inputs: unknown): void {
    if (!this.running || !isObj(inputs)) return
    const key = JSON.stringify(inputs)
    if (key === this.inputsKey) return
    this.inputsKey = key
    if (this.player.smSetInputs(key)) this.changed()
    this.drain()
  }

  /** An interaction (`click`, `pointerDown`, `pointerUp`, `pointerEnter`, `pointerExit`). */
  post(kind: string, layer = ``): void {
    if (!this.running) return
    if (this.player.smPost(kind, layer)) this.changed()
    this.drain()
  }

  /** The clock reported `done` for the current play. */
  done(): void {
    if (!this.completeSent) {
      this.completeSent = true
      this.emit(`complete`)
    }
    this.post(`complete`)
  }

  /** A new play without a machine (the playback props changed). */
  replay(): void {
    if (!this.running) this.completeSent = false
  }

  private changed(): void {
    this.completeSent = false
    this.restart()
  }

  private drain(): void {
    if (!this.running) return
    const status = JSON.parse(this.player.smStatus()) as { entered?: string[]; final?: boolean } | null
    for (const state of status?.entered ?? []) this.emit(`state`, { state })
    if (status?.final && !this.completeSent) {
      this.completeSent = true
      this.emit(`complete`)
    }
  }
}

// --- The view ----------------------------------------------------------------

let themeIds = new WeakMap<object, number>()
let nextThemeId = 1
const themeId = (t: object) => {
  let id = themeIds.get(t)
  if (!id) themeIds.set(t, (id = nextThemeId++))
  return id
}

/** One Motion on the page: load, clock, paint. `update` on every prop or
 *  environment change; `destroy` on unmount. */
export class MotionView {
  private props: Props = {}
  private nodeStyle: unknown = undefined
  private env: MotionEnv | null = null
  private sourceKey = ``
  private slotsKey = ``
  private playbackKey = ``
  private generation = 0
  private lottie: string | null = null
  /** A `.lottie` src's first state machine. */
  private fileMachine: Record<string, unknown> | null = null
  private driver: MotionStateDriver | null = null
  private halfSeen = false
  private visibleSent = false
  private slotIds: string[] = []
  private player: Player | null = null
  private images: (MotionImageSource | null)[] = []
  private pool = new MotionCanvasPool()
  private io: IntersectionObserver | null = null
  private ro: ResizeObserver | null = null
  private visible = false
  private surface: Node | null = null
  private raf = 0
  private last = 0
  private lastPaint = -Infinity
  private elapsed = 0
  private finished = false
  private restarts = 0
  private inTick = false
  private painted = ``
  private destroyed = false

  constructor(
    readonly root: HTMLElement,
    readonly canvas: HTMLCanvasElement
  ) {
    // M4 §3–4: interactions reach the machine and the node's `on` even
    // where nothing paints (the events need no player).
    root.addEventListener(`click`, this.onClick)
    root.addEventListener(`keydown`, this.onKey)
    root.addEventListener(`pointerdown`, this.onPointer)
    root.addEventListener(`pointerup`, this.onPointer)
    root.addEventListener(`pointerenter`, this.onPointer)
    root.addEventListener(`pointerleave`, this.onPointer)
    if (!motionCanPaint()) return
    this.io = new IntersectionObserver(
      (entries) => {
        const e = entries[entries.length - 1]
        this.visible = Boolean(e?.isIntersecting)
        // `visible`: the first time half of it is on screen.
        if (e && e.isIntersecting && e.intersectionRatio >= 0.5) this.halfSeen = true
        this.flushVisible()
        this.schedule()
      },
      { threshold: [0, 0.5] }
    )
    this.io.observe(root)
    if (typeof ResizeObserver === `function`) {
      this.ro = new ResizeObserver(() => this.repaint())
      this.ro.observe(root)
    }
    hiddenWatchers.add(this)
    if (hiddenListeners++ === 0) document.addEventListener(`visibilitychange`, onVisibility)
  }

  /** `nodeStyle` = the node's own `style` (a style width/height wins over the composition size). */
  update(props: Props, env: MotionEnv, nodeStyle?: unknown): void {
    if (this.destroyed) return
    this.props = props
    this.nodeStyle = nodeStyle
    this.env = env
    if (!this.io) return
    this.flushVisible()
    const sourceKey = Array.isArray(props.layers)
      ? `d:${JSON.stringify(Object.fromEntries(DIALECT_KEYS.map((k) => [k, props[k]])))}`
      : isObj(props.lottie)
        ? `l:${JSON.stringify(props.lottie)}`
        : typeof props.src === `string` && props.src
          ? `s:${props.src}`
          : ``
    const slotsKey = `${themeId(env.theme)}:${env.mode}:${JSON.stringify(props.slots ?? null)}`
    if (sourceKey !== this.sourceKey) {
      this.sourceKey = sourceKey
      this.slotsKey = slotsKey
      void this.load()
      return
    }
    if (slotsKey !== this.slotsKey) {
      this.slotsKey = slotsKey
      this.instantiate()
      return
    }
    this.applyMachine()
    this.applyPlayback()
    this.schedule()
  }

  private emit(event: MotionEvent, payload?: Record<string, unknown>): void {
    if (!this.destroyed) this.env?.emit?.(event, payload)
  }

  /** `on.visible` once, as soon as there is an env to emit through. */
  private flushVisible(): void {
    if (!this.halfSeen || this.visibleSent || !this.env) return
    this.visibleSent = true
    this.emit(`visible`)
  }

  private onClick = (): void => {
    this.driver?.post(`click`)
    this.emit(`press`)
  }

  private onKey = (e: KeyboardEvent): void => {
    if (!this.env?.pressable || (e.key !== `Enter` && e.key !== ` `) || e.target !== this.root) return
    e.preventDefault()
    this.onClick()
  }

  private onPointer = (e: PointerEvent): void => {
    const kind = e.type === `pointerdown` ? `pointerDown` : e.type === `pointerup` ? `pointerUp` : e.type === `pointerenter` ? `pointerEnter` : `pointerExit`
    this.driver?.post(kind)
    if (kind === `pointerEnter`) this.emit(`hover`)
  }

  /** The machine (the prop, else the file's) and its inputs → the engine. */
  private applyMachine(): void {
    const d = this.driver
    if (!d) return
    const was = d.running
    const running = d.setMachine(isObj(this.props.stateMachine) ? this.props.stateMachine : this.fileMachine)
    if (was && !running) this.playbackKey = `` // back on the playback props
    d.setInputs(this.props.inputs)
  }

  /** The state changed: the clock restarts at 0. */
  private restartClock = (): void => {
    this.elapsed = 0
    this.finished = false
    this.restarts++
    this.painted = ``
    if (!this.inTick) this.schedule()
  }

  destroy(): void {
    this.destroyed = true
    for (const t of [`click`, `keydown`, `pointerdown`, `pointerup`, `pointerenter`, `pointerleave`] as const) this.root.removeEventListener(t, (t === `click` ? this.onClick : t === `keydown` ? this.onKey : this.onPointer) as EventListener)
    this.generation++
    this.stop()
    this.leaveSurface()
    this.io?.disconnect()
    this.ro?.disconnect()
    if (this.io) {
      hiddenWatchers.delete(this)
      if (--hiddenListeners === 0) document.removeEventListener(`visibilitychange`, onVisibility)
    }
    this.player?.free()
    this.player = null
  }

  private fail(message: string): void {
    this.root.dataset.state = `error`
    this.env?.onError?.(message)
  }

  private async load(): Promise<void> {
    const gen = ++this.generation
    this.free()
    this.lottie = null
    this.fileMachine = null
    this.root.dataset.state = `loading`
    const props = this.props
    const env = this.env!
    try {
      const wasm = await (await import("../../motion/wasm")).loadMotionWasm()
      let lottie: string
      if (Array.isArray(props.layers)) {
        const doc = Object.fromEntries(DIALECT_KEYS.filter((k) => props[k] !== undefined).map((k) => [k, props[k]]))
        const out = JSON.parse(wasm.compile(JSON.stringify(doc))) as { lottie?: unknown; errors?: { path?: string; message?: string }[] }
        if (!out.lottie) throw new Error(`Motion: ${(out.errors ?? []).map((e) => `${e.path ?? ``} ${e.message ?? ``}`.trim()).join(`; `) || `invalid layers`}`)
        lottie = JSON.stringify(out.lottie)
      } else if (isObj(props.lottie)) lottie = JSON.stringify(props.lottie)
      else if (typeof props.src === `string` && props.src) {
        const req = mediaRequestOf(env.host, props.src)
        if (!req) throw new Error(`Motion: src denied by the media policy`)
        const blob = await fetchLimited(req.url, req.headers ?? {}, MEDIA_LIMITS, false, (u) => mediaUrlAllowed(env.host, u))
        const bytes = new Uint8Array(await blob.arrayBuffer())
        if (bytes[0] === 0x50 && bytes[1] === 0x4b) {
          const dot = await motionDotLottie(bytes)
          lottie = dot.lottie
          this.fileMachine = dot.stateMachine
        } else lottie = new TextDecoder().decode(bytes)
      } else throw new Error(`Motion: no source (layers, lottie or src)`)
      if (gen !== this.generation) return
      this.lottie = lottie
      const slots = (JSON.parse(lottie) as { slots?: unknown }).slots
      this.slotIds = isObj(slots) ? Object.keys(slots) : []
      this.instantiate()
    } catch (e) {
      if (gen !== this.generation) return
      this.fail(e instanceof Error ? e.message : String(e))
    }
  }

  private free(): void {
    this.stop()
    this.driver = null
    this.player?.free()
    this.player = null
    this.images = []
    this.painted = ``
  }

  /** (Re)build the player from the loaded Lottie with the current slots. */
  private instantiate(): void {
    if (!this.lottie || !this.env) return
    void import("../../motion/wasm").then(async (m) => {
      const wasm = await m.loadMotionWasm()
      if (this.destroyed || !this.lottie || !this.env) return
      this.free()
      const slots = motionThemeSlots(this.slotIds, this.env.theme, this.env.mode, this.props.slots)
      try {
        this.player = new wasm.MotionPlayer(this.lottie, JSON.stringify(slots))
      } catch (e) {
        this.fail(`Motion: ${e instanceof Error ? e.message : String(e)}`)
        return
      }
      this.root.dataset.state = `ready`
      if (!motionDeclaredSize(this.props)) {
        // `src`: the box learns the composition size once loaded (§4).
        const w = this.player.width(), h = this.player.height()
        if (w > 0 && h > 0) Object.assign(this.root.style, motionBoxStyle([w, h], this.nodeStyle))
      }
      this.loadImages()
      this.playbackKey = ``
      this.applyPlayback()
      this.driver = new MotionStateDriver(this.player, (event, payload) => this.emit(event, payload), this.restartClock)
      this.applyMachine()
      this.schedule()
    })
  }

  private loadImages(): void {
    const player = this.player
    if (!player || !this.env) return
    const env = this.env
    player.images().forEach((loc, k) => {
      this.images[k] = null
      let url: string | null = loc
      if (!loc.startsWith(`data:`)) url = mediaRequestOf(env.host, loc)?.url ?? null
      if (!url) return
      const img = new Image()
      img.decoding = `async`
      img.onload = () => {
        if (this.player !== player) return
        this.images[k] = img
        this.repaint()
      }
      img.src = url
    })
  }

  private applyPlayback(): void {
    // A running machine's state owns the playback (M4 §1).
    if (!this.player || this.driver?.running) return
    const key = JSON.stringify(Object.fromEntries(PLAYBACK_KEYS.map((k) => [k, this.props[k]])))
    if (key === this.playbackKey) return
    this.playbackKey = key
    this.player.setPlayback(key)
    this.elapsed = 0
    this.finished = false
    this.driver?.replay()
  }

  private wantsToAnimate(): boolean {
    const p = this.props
    // With a machine its state's `autoplay` decides (the engine's clock holds the poster).
    const autoplay = this.driver?.running || p.autoplay !== false
    return Boolean(this.player) && typeof p.progress !== `number` && autoplay && p.play !== false && !this.env?.reducedMotion && !this.finished && this.visible && !document.hidden
  }

  private leaveSurface(): void {
    if (!this.surface) return
    const set = surfaces.get(this.surface)
    set?.delete(this)
    if (set && set.size === 0) surfaces.delete(this.surface)
    set?.forEach((v) => v.schedule())
    this.surface = null
  }

  /** Start or stop the clock to match the state; paint the still frame when stopped. */
  schedule(): void {
    if (this.destroyed || !this.io) return
    if (this.wantsToAnimate()) {
      if (!this.surface) {
        this.surface = surfaceOf(this.root)
        let set = surfaces.get(this.surface)
        if (!set) surfaces.set(this.surface, (set = new Set()))
        set.add(this)
        set.forEach((v) => v !== this && v.schedule())
      }
      if (granted(this, this.surface)) {
        if (!this.raf) {
          this.last = -1
          this.raf = requestAnimationFrame(this.tick)
        }
        return
      }
    } else this.leaveSurface()
    this.stop()
    this.repaint()
  }

  private stop(): void {
    if (this.raf) cancelAnimationFrame(this.raf)
    this.raf = 0
  }

  private tick = (now: number): void => {
    this.raf = 0
    if (!this.player || this.destroyed) return
    if (this.last >= 0) this.elapsed += now - this.last
    this.last = now
    if (this.player.done(this.elapsed)) {
      // `complete` once; a machine may move on (its clock restarts at 0).
      const before = this.restarts
      this.inTick = true
      this.driver?.done()
      if (!this.driver) this.emit(`complete`)
      this.inTick = false
      if (this.restarts === before || !this.player) {
        this.finished = true
        this.schedule()
        return
      }
    }
    if (now - this.lastPaint >= 1000 / MOTION_MAX_FPS - 1) {
      this.lastPaint = now
      this.paint(this.player.tick(this.elapsed))
    }
    if (!this.raf) this.raf = requestAnimationFrame(this.tick)
  }

  /** The frame shown while the clock is stopped. */
  private stillFrame(): number {
    const p = this.player!
    const progress = this.props.progress
    if (typeof progress === `number`) return p.progressFrame(Math.max(0, Math.min(1, progress)))
    if (this.finished || (this.elapsed > 0 && !this.env?.reducedMotion && (this.driver?.running || this.props.autoplay !== false))) return p.tick(this.elapsed)
    return p.posterFrame()
  }

  private repaint(): void {
    if (!this.player || this.raf) {
      if (this.raf) this.painted = ``
      return
    }
    this.painted = ``
    this.paint(this.stillFrame())
  }

  private paint(frame: number): void {
    const player = this.player
    if (!player) return
    const rect = this.root.getBoundingClientRect()
    const dpr = (typeof window !== `undefined` && window.devicePixelRatio) || 1
    const w = Math.max(1, Math.round(rect.width * dpr))
    const h = Math.max(1, Math.round(rect.height * dpr))
    const fit = motionFit(this.props.fit)
    const key = `${frame}|${w}|${h}|${fit}`
    if (key === this.painted) return
    this.painted = key
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w
      this.canvas.height = h
    }
    const ctx = this.canvas.getContext(`2d`)
    if (!ctx) return
    const base = motionConcat([dpr, 0, 0, dpr, 0, 0], motionFitMatrix(player.width(), player.height(), w / dpr, h / dpr, fit))
    paintMotionFrame(ctx, w, h, player.sample(frame), base, this.images, this.pool)
  }
}

/** Test seam: forget the per-theme ids (a fresh module state). */
export function resetMotionThemeIds(): void {
  themeIds = new WeakMap()
  nextThemeId = 1
}
