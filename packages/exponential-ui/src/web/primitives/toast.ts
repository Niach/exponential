// FEED-131: THE toast of the primitive sets, sonner-free: the class recipe
// (the Exponential app's `TOAST_*_CLASS` look, packages/ui/src/toast.tsx,
// minus sonner's overrides) and the framework-free engine both renderers
// bind: the store (per-toast clocks that pause while the stack is expanded)
// and the stack geometry of fixtures/toast-stack.json (a copy of the app's
// contract, drift-tested from packages/ui).
//
// VAPP-106: the constants are WRITTEN here, not imported from the fixture:
// a renderer entry (`@exponential_app/ui/web/primitives`) ships no fixture
// (release/size.ts gates it); toast.test.ts holds them equal to
// fixtures/toast-stack.json `constants`.

export const TOAST_CONSTANTS = {
  width: 356,
  gap: 14,
  peek: 14,
  scaleStep: 0.05,
  visible: 3,
  durationMs: 4000,
  swipeThreshold: 45,
  viewportOffset: 24,
  mobileViewportOffset: 16,
  placement: { pointer: `bottom-right`, touch: `top-center` },
  touchMaxWidth: 600,
  kinds: [`success`, `error`, `info`, `warning`],
}

export type ToastKind = `success` | `error` | `info` | `warning`
export type ToastPlacement = `bottom-right` | `top-center`

export const TOAST_KINDS = TOAST_CONSTANTS.kinds as readonly ToastKind[]

/** Touch = up to and including `touchMaxWidth` (the fixture's phone breakpoint). */
export const TOAST_TOUCH_QUERY = `(max-width: ${TOAST_CONSTANTS.touchMaxWidth}px)`

export const TOAST_CLASSES = {
  /** The fixed region (`<section>`), per placement: `viewportOffset` from
   *  the corner on a pointer, `mobileViewportOffset` below the status bar
   *  on touch. */
  region: `pointer-events-none fixed z-[100] flex`,
  regionPlacement: {
    "bottom-right": `right-6 bottom-6`,
    "top-center": `inset-x-4 top-[calc(env(safe-area-inset-top,0px)+16px)] justify-center`,
  } satisfies Record<ToastPlacement, string>,
  /** The stack box (`<ol>`): `width` wide, its height = the geometry's. */
  list: `pointer-events-auto relative m-0 w-[356px] max-w-full list-none p-0 transition-[height] duration-standard ease-standard`,
  listPlacement: {
    "bottom-right": ``,
    "top-center": `w-full`,
  } satisfies Record<ToastPlacement, string>,
  /** The card: the opaque glass card at radius lg, no shadow; placed by its
   *  `--offset`/`--scale`. A collapsed back toast hides its contents behind
   *  the front one; a toast past `visible` draws nothing. */
  toast: `absolute inset-x-0 top-(--offset) flex items-start gap-3 rounded-lg border border-glass-stroke-card bg-glass-card-opaque p-4 font-sans text-sm text-foreground scale-(--scale) transition-[top,scale,opacity] duration-standard ease-standard data-[expanded=false]:data-[front=false]:*:opacity-0 data-[visible=false]:pointer-events-none data-[visible=false]:opacity-0`,
  /** A back card shrinks about the edge that PEEKS OUT: the top on a bottom
   *  stack, the bottom on a top stack. */
  toastOrigin: {
    bottom: `origin-top`,
    top: `origin-bottom`,
  },
  content: `flex min-w-0 flex-1 flex-col gap-0.5`,
  title: `leading-5 font-medium text-foreground`,
  description: `leading-5 text-muted-foreground`,
  action: `inline-flex h-6 shrink-0 cursor-pointer items-center rounded-md border-0 bg-primary px-2 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90`,
  /** The close glyph: last in the row. */
  close: `order-last inline-flex size-5 shrink-0 cursor-pointer items-center justify-center rounded-sm border-0 bg-transparent p-0 text-muted-foreground transition-colors hover:text-foreground [&_svg]:size-3.5`,
  iconSlot: `flex h-5 shrink-0 items-center`,
  icon: `size-4 shrink-0`,
  /** The ONLY place a toast wears its kind. */
  kindIcon: {
    success: `text-green-500`,
    error: `text-red-500`,
    info: `text-blue-500`,
    warning: `text-yellow-500`,
  } satisfies Record<ToastKind, string>,
} as const

/** The Lucide glyph each kind paints (the app's concept icons
 *  `ui-success`/`ui-error`/`ui-info`/`ui-warning`, packages/icons/icons.json). */
export const TOAST_KIND_GLYPH = {
  success: `circle-check`,
  error: `circle-x`,
  info: `info`,
  warning: `triangle-alert`,
} as const satisfies Record<ToastKind, string>

/** The fixture's `placement`: bottom-right on a pointer, top-centre on touch;
 *  a bottom stack measures its peek above the front toast. */
export function toastPlacement(isTouch: boolean): { placement: ToastPlacement; anchoredBottom: boolean } {
  const placement = (isTouch ? TOAST_CONSTANTS.placement.touch : TOAST_CONSTANTS.placement.pointer) as ToastPlacement
  return { placement, anchoredBottom: placement.startsWith(`bottom`) }
}

export interface ToastStackItem {
  /** The item's top inside the stack box. */
  offset: number
  /** Its width factor. */
  scale: number
  /** Whether it draws at all. */
  visible: boolean
}

/** The numbers a stack is drawn from (the toast's, the Dock pile's). */
export interface StackConstants {
  /** How far an older item peeks out behind the one in front of it. */
  peek: number
  /** How much smaller each rank behind the front draws. */
  scaleStep: number
  /** How many items draw at all while collapsed. */
  visible: number
  /** The space between items while expanded. */
  gap: number
}

/** THE stack geometry ×4 (fixtures/toast-stack.json `geometry`; N-12: the
 *  Dock pile draws it too): `heights` oldest first (the newest is in front).
 *  Collapsed, the item of rank r (0 = front) sits r × `peek` from the
 *  anchoring edge, shrunk by `scaleStep` per rank (clamped at the last
 *  visible rank), `visible` of them drawn; expanded, every item at full
 *  size, `gap` apart. */
export function stackGeometry(
  heights: readonly number[],
  expanded: boolean,
  anchoredBottom: boolean,
  constants: StackConstants
): { height: number; items: ToastStackItem[] } {
  const { gap, peek, scaleStep, visible } = constants
  const n = heights.length
  if (n === 0) return { height: 0, items: [] }
  const rank = (i: number) => n - 1 - i
  if (expanded) {
    const height = heights.reduce((sum, h) => sum + h, 0) + (n - 1) * gap
    // The distance of each item from the anchoring edge: the newer ones first.
    const fromEdge: number[] = new Array(n)
    let at = 0
    for (let i = n - 1; i >= 0; i--) {
      fromEdge[i] = at
      at += heights[i]! + gap
    }
    return { height, items: heights.map((h, i) => ({ offset: anchoredBottom ? height - fromEdge[i]! - h : fromEdge[i]!, scale: 1, visible: true })) }
  }
  const height = Math.max(...heights.map((h, i) => rank(i) * peek + h))
  const items = heights.map((h, i) => {
    const r = rank(i)
    const scale = Math.round((1 - Math.min(r, visible - 1) * scaleStep) * 100) / 100
    return { offset: anchoredBottom ? height - r * peek - h : r * peek, scale, visible: r < visible }
  })
  return { height, items }
}

/** The toast's stack (`stackGeometry` with the toast's numbers). */
export function toastStackGeometry(heights: readonly number[], expanded: boolean, anchoredBottom: boolean): { height: number; items: ToastStackItem[] } {
  return stackGeometry(heights, expanded, anchoredBottom, TOAST_CONSTANTS)
}

export interface ToastAction {
  label: string
  onClick?: (event: Event) => void
}

export interface ToastOptions {
  /** A fixed id (else the store generates `toast-<n>`); re-using a live id
   *  replaces that toast. */
  id?: string
  kind?: ToastKind
  description?: string
  action?: ToastAction
  /** The auto-dismiss clock in ms (default `durationMs`); `Infinity` = sticky. */
  duration?: number
  /** A fixed height for the stack geometry (else the renderer measures). */
  height?: number
  onDismiss?: (toast: ToastRecord) => void
}

export interface ToastRecord {
  id: string
  kind?: ToastKind
  title: string
  description?: string
  action?: ToastAction
  duration: number
  height?: number
}

export interface ToastFn {
  (title: string, options?: ToastOptions): string
  (options: ToastOptions & { title: string }): string
  success: (title: string, options?: Omit<ToastOptions, `kind`>) => string
  error: (title: string, options?: Omit<ToastOptions, `kind`>) => string
  info: (title: string, options?: Omit<ToastOptions, `kind`>) => string
  warning: (title: string, options?: Omit<ToastOptions, `kind`>) => string
}

export interface ToastStore {
  toast: ToastFn
  /** Removes one toast, or every toast without an id. */
  dismiss: (id?: string) => void
  /** The live toasts, oldest first. */
  toasts: () => readonly ToastRecord[]
  /** Called with the toasts on every change; returns the unsubscribe. */
  subscribe: (listener: (toasts: readonly ToastRecord[]) => void) => () => void
  /** Stops every clock (the stack expanded), keeping what is left of each. */
  pauseAll: () => void
  /** Restarts every clock with its remaining time. */
  resumeAll: () => void
  readonly paused: boolean
}

interface Clock {
  remaining: number
  startedAt: number
  timer?: ReturnType<typeof setTimeout>
}

/** A toast store: the queue, the per-toast clocks and their pause. */
export function createToastStore(): ToastStore {
  let list: ToastRecord[] = []
  const options = new Map<string, ToastOptions>()
  const clocks = new Map<string, Clock>()
  const listeners = new Set<(toasts: readonly ToastRecord[]) => void>()
  let paused = false
  let counter = 0
  const emit = () => {
    for (const l of [...listeners]) l(list)
  }
  const stop = (id: string) => {
    const clock = clocks.get(id)
    if (!clock?.timer) return
    clearTimeout(clock.timer)
    clock.timer = undefined
    clock.remaining = Math.max(0, clock.remaining - (Date.now() - clock.startedAt))
  }
  const start = (id: string) => {
    const clock = clocks.get(id)
    if (!clock || clock.timer || !Number.isFinite(clock.remaining)) return
    clock.startedAt = Date.now()
    clock.timer = setTimeout(() => dismiss(id), clock.remaining)
  }
  const dismiss = (id?: string) => {
    const gone = id === undefined ? list : list.filter((t) => t.id === id)
    if (gone.length === 0) return
    for (const t of gone) {
      const clock = clocks.get(t.id)
      if (clock?.timer) clearTimeout(clock.timer)
      clocks.delete(t.id)
    }
    list = id === undefined ? [] : list.filter((t) => t.id !== id)
    emit()
    for (const t of gone) {
      const o = options.get(t.id)
      options.delete(t.id)
      o?.onDismiss?.(t)
    }
  }
  const add = (title: string, o: ToastOptions = {}): string => {
    const id = o.id ?? `toast-${++counter}`
    const record: ToastRecord = { id, kind: o.kind, title, description: o.description, action: o.action, duration: o.duration ?? TOAST_CONSTANTS.durationMs, height: o.height }
    const old = clocks.get(id)
    if (old?.timer) clearTimeout(old.timer)
    options.set(id, o)
    clocks.set(id, { remaining: record.duration, startedAt: 0 })
    const at = list.findIndex((t) => t.id === id)
    list = at >= 0 ? list.map((t, i) => (i === at ? record : t)) : [...list, record]
    if (!paused) start(id)
    emit()
    return id
  }
  const toast = ((first: string | (ToastOptions & { title: string }), o?: ToastOptions) => (typeof first === `string` ? add(first, o) : add(first.title, first))) as ToastFn
  for (const kind of TOAST_KINDS) toast[kind] = (title, o) => add(title, { ...o, kind })
  return {
    toast,
    dismiss,
    toasts: () => list,
    subscribe: (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    pauseAll: () => {
      if (paused) return
      paused = true
      for (const id of clocks.keys()) stop(id)
    },
    resumeAll: () => {
      if (!paused) return
      paused = false
      for (const id of clocks.keys()) start(id)
    },
    get paused() {
      return paused
    },
  }
}
