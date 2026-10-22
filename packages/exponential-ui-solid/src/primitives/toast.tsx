import { For, Show, createEffect, createMemo, createSignal, from, onCleanup, splitProps, type ComponentProps } from "solid-js"
import { CircleCheck, CircleX, Info, TriangleAlert, X } from "lucide-solid"
import {
  TOAST_CLASSES,
  TOAST_TOUCH_QUERY,
  createToastStore,
  toastPlacement,
  toastStackGeometry,
  type ToastKind,
  type ToastRecord,
  type ToastStore,
} from "@exponential-at/ui/web/primitives"

import { bindStyle } from "../behaviors/style-writer"
import { PaintIcon } from "../icons"
import { createMediaQuery } from "../platform"
import { css } from "../style"
import { cn } from "./cn"

// FEED-131: React's toast.tsx on Solid: the same region, stack box and
// cards, placed by the engine's geometry; hover (pointer) or a tap (touch)
// expands the stack and pauses every clock.

const KIND_ICONS = { success: CircleCheck, error: CircleX, info: Info, warning: TriangleAlert } satisfies Record<ToastKind, unknown>

/** The default store `toast` writes to and `<Toaster />` reads. */
export const toastStore: ToastStore = createToastStore()

/** `toast("…")`, `toast.success("…", { description, action })`, … on the default store. */
export const toast = toastStore.toast

export type ToasterProps = Omit<ComponentProps<`section`>, `children`> & {
  store?: ToastStore
  /** The placement input (default: the `touchMaxWidth` media query). */
  touch?: boolean
  /** The close glyph's label. */
  closeLabel?: string
}

export function Toaster(props: ToasterProps) {
  const [local, rest] = splitProps(props, [`store`, `touch`, `closeLabel`, `class`])
  const store = local.store ?? toastStore
  const toasts = from<readonly ToastRecord[]>((set) => {
    set(store.toasts())
    return store.subscribe((t) => set(t))
  })
  const list = () => toasts() ?? []
  const touchQuery = createMediaQuery(TOAST_TOUCH_QUERY)
  const placed = createMemo(() => toastPlacement(local.touch ?? touchQuery()))
  const [expanded, setExpanded] = createSignal(false)
  const [measured, setMeasured] = createSignal<Record<string, number>>({})
  const items = new Map<string, HTMLLIElement>()
  const open = () => expanded() && list().length > 0

  createEffect(() => {
    if (open()) store.pauseAll()
    else store.resumeAll()
  })

  createEffect(() => {
    const next: Record<string, number> = {}
    for (const t of list()) {
      const el = items.get(t.id)
      if (t.height === undefined && el) next[t.id] = el.getBoundingClientRect().height
    }
    const prev = measured()
    if (!(Object.keys(next).length === Object.keys(prev).length && Object.entries(next).every(([k, v]) => prev[k] === v))) setMeasured(next)
  })

  const geometry = createMemo(() =>
    toastStackGeometry(
      list().map((t) => t.height ?? measured()[t.id] ?? 0),
      open(),
      placed().anchoredBottom
    )
  )

  return (
    <section
      aria-label="Notifications"
      data-slot="toaster"
      data-placement={placed().placement}
      class={cn(TOAST_CLASSES.region, TOAST_CLASSES.regionPlacement[placed().placement], local.class)}
      {...rest}
    >
      <ol
        ref={(el) => bindStyle(el, () => ({ height: `${geometry().height}px` }))}
        data-slot="toast-list"
        data-expanded={open() ? `true` : `false`}
        class={cn(TOAST_CLASSES.list, TOAST_CLASSES.listPlacement[placed().placement])}
        onPointerEnter={(event) => {
          if (event.pointerType !== `touch`) setExpanded(true)
        }}
        onPointerLeave={(event) => {
          if (event.pointerType !== `touch`) setExpanded(false)
        }}
        onPointerDown={(event) => {
          if (event.pointerType === `touch`) setExpanded((was) => !was)
        }}
      >
        <For each={list()}>
          {(t, i) => {
            const item = () => geometry().items[i()] ?? { offset: 0, scale: 1, visible: true }
            const rank = () => list().length - 1 - i()
            let node: HTMLLIElement | undefined
            onCleanup(() => {
              if (items.get(t.id) === node) items.delete(t.id)
            })
            return (
              <li
                ref={(el) => {
                  node = el
                  items.set(t.id, el)
                  bindStyle(el, () => css({ "--offset": `${item().offset}px`, "--scale": `${item().scale}`, zIndex: i() + 1 })!)
                }}
                role="status"
                aria-live="polite"
                aria-atomic="true"
                data-slot="toast"
                data-kind={t.kind ?? `default`}
                data-front={rank() === 0 ? `true` : `false`}
                data-expanded={open() ? `true` : `false`}
                data-visible={item().visible ? `true` : `false`}
                data-index={rank()}
                class={cn(TOAST_CLASSES.toast, TOAST_CLASSES.toastOrigin[placed().anchoredBottom ? `bottom` : `top`])}
              >
                <Show when={t.kind}>
                  {(kind) => (
                    <div class={TOAST_CLASSES.iconSlot}>
                      <PaintIcon icon={KIND_ICONS[kind()]} data-toast-kind={kind()} class={cn(TOAST_CLASSES.icon, TOAST_CLASSES.kindIcon[kind()])} />
                    </div>
                  )}
                </Show>
                <div class={TOAST_CLASSES.content}>
                  <div data-slot="toast-title" class={TOAST_CLASSES.title}>
                    {t.title}
                  </div>
                  <Show when={t.description !== undefined}>
                    <div data-slot="toast-description" class={TOAST_CLASSES.description}>
                      {t.description}
                    </div>
                  </Show>
                </div>
                <Show when={t.action}>
                  {(action) => (
                    <button
                      type="button"
                      data-slot="toast-action"
                      class={TOAST_CLASSES.action}
                      onClick={(event) => {
                        action().onClick?.(event)
                        store.dismiss(t.id)
                      }}
                    >
                      {action().label}
                    </button>
                  )}
                </Show>
                <button type="button" data-slot="toast-close" aria-label={local.closeLabel ?? `Close`} class={TOAST_CLASSES.close} onClick={() => store.dismiss(t.id)}>
                  <PaintIcon icon={X} />
                </button>
              </li>
            )
          }}
        </For>
      </ol>
    </section>
  )
}

export { createToastStore, toastStackGeometry, toastPlacement, TOAST_CONSTANTS, TOAST_KINDS, TOAST_TOUCH_QUERY } from "@exponential-at/ui/web/primitives"
export type { ToastKind, ToastOptions, ToastRecord, ToastStore, ToastAction, ToastPlacement } from "@exponential-at/ui/web/primitives"
