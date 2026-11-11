// #EXP-1629 (interactions-round1.json `menuPreview`; React's
// natives/menu-preview.tsx): a Menu item's `preview`, the IDE composer
// menu's side hover card. Which row's preview shows is the TS reference's
// rule (`menuPreviewInput` / `menuPreviewTick`): a keyboard highlight at
// once, a hovered row after `MENU_PREVIEW_OPEN_MS`, hidden
// `MENU_PREVIEW_CLOSE_MS` after the pointer and the highlight left the row
// and the preview. The preview is the `Embed` native (its sandbox; an inner
// event fires the Menu's `action`) in the `Menu/preview` box beside its
// row: the inline end first, `MENU_PREVIEW_OFFSET` off it, flipped without
// room, centred on the row, kept `OVERLAY_PADDING` inside the viewport.

import { Show, createContext, createEffect, createSignal, on, onCleanup, untrack, useContext, type Accessor, type JSX } from "solid-js"
import { Portal } from "solid-js/web"
import { MENU_PREVIEW_IDLE, MENU_PREVIEW_OFFSET, OVERLAY_PADDING, menuPreviewInput, menuPreviewTick, type MenuPreviewInput, type MenuPreviewState } from "@exponential_app/eui"
import type { UiNode } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { EmbedNative } from "./embed"
import type { PartFn } from "./shared"

export interface MenuPreviewApi {
  state: Accessor<MenuPreviewState>
  input: (input: MenuPreviewInput) => void
  /** The painted rows with a preview, by item path. */
  rows: Map<string, HTMLElement>
}

export const MenuPreviewContext = createContext<MenuPreviewApi | null>(null)

/** One opening's preview state: reset when the menu closes; a pending
 *  change applies on its timer. */
export function createMenuPreview(open: Accessor<boolean>): MenuPreviewApi {
  const [state, setState] = createSignal<MenuPreviewState>(MENU_PREVIEW_IDLE)
  const rows = new Map<string, HTMLElement>()
  const input = (i: MenuPreviewInput) => setState((s) => menuPreviewInput(s, i, Date.now()))
  createEffect(on(open, (o) => !o && setState(MENU_PREVIEW_IDLE), { defer: true }))
  let timer: ReturnType<typeof setTimeout> | undefined
  createEffect(
    on(
      () => state().pending,
      (pending) => {
        clearTimeout(timer)
        if (pending) timer = setTimeout(() => setState((s) => menuPreviewTick(s, Date.now())), Math.max(0, pending.at - Date.now()))
      }
    )
  )
  onCleanup(() => clearTimeout(timer))
  return { state, input, rows }
}

interface PreviewItem {
  kind?: string
  preview?: unknown
  items?: PreviewItem[]
}

/** True when an item shows a preview: an action or a checkbox with one. */
export function hasPreview(item: PreviewItem): boolean {
  return item.preview !== undefined && item.preview !== null && item.kind !== `submenu` && item.kind !== `separator` && item.kind !== `label`
}

/** Does any row (a submenu's included) show a preview? None = the menu
 *  tracks nothing (absent = unchanged). */
export function anyPreview(items: PreviewItem[]): boolean {
  return items.some((i) => hasPreview(i) || (Array.isArray(i.items) && anyPreview(i.items)))
}

/** Radix's outside-interaction guard: a press or a focus inside a preview
 *  is the menu's own. */
export function insidePreview(e: { target: EventTarget | null; preventDefault: () => void }): boolean {
  const target = e.target as Element | null
  const inside = !!target && typeof target.closest === `function` && !!target.closest(`[data-xui-menu-preview]`)
  if (inside) e.preventDefault()
  return inside
}

/** A row's tracking: `key` = its path, `has` = it has a preview (a row
 *  without one counts as no row). `keyboard` = the last input was a key. */
export function menuPreviewRow(api: MenuPreviewApi | null, key: string, has: boolean, keyboard: () => boolean) {
  const own = has ? key : null
  return {
    ref: (el: HTMLElement) => {
      if (api && has) api.rows.set(key, el)
    },
    enter: () => api?.input({ hover: own }),
    leave: () => {
      if (api && untrack(api.state).hover === own) api.input({ hover: null })
    },
    focus: () => {
      if (api && keyboard()) api.input({ highlight: own })
    },
    blur: () => {
      if (api && untrack(api.state).highlight !== null && untrack(api.state).highlight === own) api.input({ highlight: null })
    },
    /** A key while the pointer-focused row is this one: it is highlighted. */
    key: () => {
      if (api && own !== null && untrack(api.state).highlight !== own) api.input({ highlight: own })
    },
  }
}

/** The item at path `key` (`"3.0"`). */
function itemAt(items: PreviewItem[], key: string): PreviewItem | undefined {
  let list: PreviewItem[] | undefined = items
  let item: PreviewItem | undefined
  for (const i of key.split(`.`)) {
    item = list?.[Number(i)]
    list = Array.isArray(item?.items) ? item.items : undefined
  }
  return item
}

/** The shown preview beside its row (nothing while none shows). */
export function MenuPreviewPanel(props: { api: MenuPreviewApi; items: PreviewItem[]; part: PartFn; emit: NativeProps[`emit`] }): JSX.Element {
  const ctx = useSurfaceContext()
  const key = () => props.api.state().shown
  return (
    <Show when={key() !== null && itemAt(props.items, key()!) ? key() : null} keyed>
      {(k) => {
        const item = itemAt(props.items, k)!
        const [at, setAt] = createSignal<{ left: number; top: number } | null>(null)
        let box: HTMLDivElement | undefined
        const place = () => {
          const row = props.api.rows.get(k)
          if (!row || !box) return
          const r = row.getBoundingClientRect()
          const w = box.offsetWidth
          const h = box.offsetHeight
          const vw = window.innerWidth
          const vh = window.innerHeight
          const after = r.right + MENU_PREVIEW_OFFSET
          const before = r.left - MENU_PREVIEW_OFFSET - w
          const fitsAfter = after + w <= vw - OVERLAY_PADDING
          const fitsBefore = before >= OVERLAY_PADDING
          const left = ctx.direction === `rtl` ? (fitsBefore || !fitsAfter ? before : after) : fitsAfter || !fitsBefore ? after : before
          const top = Math.min(Math.max(OVERLAY_PADDING, r.top + r.height / 2 - h / 2), Math.max(OVERLAY_PADDING, vh - OVERLAY_PADDING - h))
          setAt({ left: Math.max(OVERLAY_PADDING, left), top })
        }
        queueMicrotask(place)
        const id = `${props.part.domId}.preview.${k}`
        const node = { id, component: `Embed`, props: { surface: item.preview }, children: [] } as unknown as UiNode
        const embed = { node, props: { surface: item.preview }, rootProps: { class: `xui-menu-preview-surface` }, emit: (event: string, payload?: Record<string, unknown>) => props.emit(event, payload), domId: `${id}.surface`, scope: ``, slots: {} } as unknown as NativeProps
        const panel = (
          <div
            ref={(el) => (box = el)}
            {...props.part.at(`preview`, k, `open`)}
            data-xui-menu-preview={k}
            style={{ position: `fixed`, left: `${at()?.left ?? 0}px`, top: `${at()?.top ?? 0}px`, visibility: at() ? `visible` : `hidden`, "pointer-events": `auto` }}
            onPointerEnter={() => props.api.input({ over: true })}
            onPointerLeave={() => props.api.input({ over: false })}
          >
            <EmbedNative {...embed} />
          </div>
        )
        return <Portal mount={ctx.portal ?? document.body}>{panel}</Portal>
      }}
    </Show>
  )
}

export const useMenuPreviewApi = () => useContext(MenuPreviewContext) ?? null
