// #EXP-1629 (interactions-round1.json `menuPreview`): a Menu item's
// `preview`, the IDE composer menu's side hover card. Which row's preview
// shows is the TS reference's rule (`menuPreviewInput` / `menuPreviewTick`):
// a keyboard highlight at once, a hovered row after `MENU_PREVIEW_OPEN_MS`,
// hidden `MENU_PREVIEW_CLOSE_MS` after the pointer and the highlight left
// the row and the preview. The preview is the `Embed` native (its sandbox:
// its own data, ids and surface id; an inner event fires the Menu's
// `action`), in the `Menu/preview` box placed beside its row: the inline
// end first, `MENU_PREVIEW_OFFSET` off it, flipped without room, centred on
// the row and kept `OVERLAY_PADDING` inside the viewport.

import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { createPortal } from "react-dom"
import { MENU_PREVIEW_IDLE, MENU_PREVIEW_OFFSET, OVERLAY_PADDING, menuPreviewInput, menuPreviewTick, type MenuPreviewInput, type MenuPreviewState } from "@exponential_app/eui"
import type { UiNode } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { EmbedNative } from "./embed"
import type { PartFn } from "./shared"

export interface MenuPreviewApi {
  state: MenuPreviewState
  input: (input: MenuPreviewInput) => void
  /** The painted rows with a preview, by item path. */
  rows: Map<string, HTMLElement>
  /** The last input was a key (a focus then is a keyboard highlight). */
  keyboard: { current: boolean }
}

export const MenuPreviewContext = createContext<MenuPreviewApi | null>(null)

/** One opening's preview state: reset when the menu closes; a pending
 *  change applies on its timer. */
export function useMenuPreview(open: boolean): MenuPreviewApi {
  const [state, setState] = useState<MenuPreviewState>(MENU_PREVIEW_IDLE)
  const rows = useRef(new Map<string, HTMLElement>()).current
  const keyboard = useRef({ current: false }).current
  const input = useCallback((i: MenuPreviewInput) => setState((s) => menuPreviewInput(s, i, Date.now())), [])
  useEffect(() => {
    if (!open) setState(MENU_PREVIEW_IDLE)
  }, [open])
  const pending = state.pending
  useEffect(() => {
    if (!pending) return
    const t = setTimeout(() => setState((s) => menuPreviewTick(s, Date.now())), Math.max(0, pending.at - Date.now()))
    return () => clearTimeout(t)
  }, [pending])
  return useMemo(() => ({ state, input, rows, keyboard }), [state, input, rows, keyboard])
}

/** The content's modality tracking: a key down = keyboard, a pointer move
 *  = pointer (Radix focuses the row under the pointer too). A key on the
 *  row the pointer focused makes it the highlighted one. */
export function menuModalityProps(api: MenuPreviewApi) {
  return {
    onKeyDownCapture: () => {
      api.keyboard.current = true
      const active = typeof document === `undefined` ? null : document.activeElement
      let key: string | null = null
      for (const [k, row] of api.rows) if (row === active) key = k
      if (key !== null && api.state.highlight !== key) api.input({ highlight: key })
    },
    onPointerMoveCapture: () => void (api.keyboard.current = false),
  }
}

/** Radix's outside-interaction guard: a press or a focus inside a preview
 *  is the menu's own. */
export function insidePreview(e: { target: EventTarget | null; preventDefault: () => void }) {
  const target = e.target as Element | null
  if (target && typeof target.closest === `function` && target.closest(`[data-xui-menu-preview]`)) e.preventDefault()
}

/** The props a row hands its element: `key` = its path, `has` = it has a
 *  preview (a row without one counts as no row). */
export function useMenuPreviewRow(key: string, has: boolean) {
  const api = useContext(MenuPreviewContext)
  const own = has ? key : null
  return useMemo(() => {
    if (!api) return {}
    return {
      ref: (el: HTMLElement | null) => {
        if (el && has) api.rows.set(key, el)
        else api.rows.delete(key)
      },
      onPointerEnter: () => api.input({ hover: own }),
      onPointerLeave: () => {
        if (api.state.hover === own) api.input({ hover: null })
      },
      onFocus: () => {
        if (api.keyboard.current) api.input({ highlight: own })
      },
      onBlur: () => {
        if (api.state.highlight !== null && api.state.highlight === own) api.input({ highlight: null })
      },
    }
  }, [api, key, has, own])
}

interface PreviewItem {
  kind?: string
  preview?: unknown
  items?: PreviewItem[]
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

/** True when an item shows a preview: an action or a checkbox with one. */
export function hasPreview(item: PreviewItem): boolean {
  return item.preview !== undefined && item.preview !== null && item.kind !== `submenu` && item.kind !== `separator` && item.kind !== `label`
}

/** Does any row (a submenu's included) show a preview? None = the menu
 *  tracks nothing (absent = unchanged). */
export function anyPreview(items: PreviewItem[]): boolean {
  return items.some((i) => hasPreview(i) || (Array.isArray(i.items) && anyPreview(i.items)))
}

/** The shown preview beside its row (nothing while none shows). */
export function MenuPreviewPanel({ api, items, part, emit }: { api: MenuPreviewApi; items: PreviewItem[]; part: PartFn; emit: NativeProps[`emit`] }) {
  const ctx = useSurfaceContext()
  const key = api.state.shown
  const item = key === null ? undefined : itemAt(items, key)
  const box = useRef<HTMLDivElement | null>(null)
  const [at, setAt] = useState<{ left: number; top: number } | null>(null)
  useLayoutEffect(() => {
    if (key === null) return setAt(null)
    const row = api.rows.get(key)
    const el = box.current
    if (!row || !el || typeof window === `undefined`) return
    const r = row.getBoundingClientRect()
    const w = el.offsetWidth
    const h = el.offsetHeight
    const vw = window.innerWidth
    const vh = window.innerHeight
    const rtl = ctx.direction === `rtl`
    const after = r.right + MENU_PREVIEW_OFFSET
    const before = r.left - MENU_PREVIEW_OFFSET - w
    const fitsAfter = after + w <= vw - OVERLAY_PADDING
    const fitsBefore = before >= OVERLAY_PADDING
    const left = rtl ? (fitsBefore || !fitsAfter ? before : after) : fitsAfter || !fitsBefore ? after : before
    const top = Math.min(Math.max(OVERLAY_PADDING, r.top + r.height / 2 - h / 2), Math.max(OVERLAY_PADDING, vh - OVERLAY_PADDING - h))
    setAt({ left: Math.max(OVERLAY_PADDING, left), top })
  }, [key, api.rows, ctx.direction])
  const forward = useCallback((event: string, payload?: Record<string, unknown>) => emit(event, payload), [emit])
  if (key === null || !item) return null
  const id = `${part.domId}.preview.${key}`
  const node = { id, component: `Embed`, props: { surface: item.preview }, children: [] } as unknown as UiNode
  const panel: ReactNode = (
    <div
      ref={box}
      {...(part.at(`preview`, key, `open`) as Record<string, string>)}
      data-xui-menu-preview={key}
      style={{ position: `fixed`, left: at?.left ?? 0, top: at?.top ?? 0, visibility: at || ctx.inlineOverlays ? `visible` : `hidden`, pointerEvents: `auto` }}
      onPointerEnter={() => api.input({ over: true })}
      onPointerLeave={() => api.input({ over: false })}
    >
      <EmbedNative {...({ node, props: { surface: item.preview }, rootProps: { className: `xui-menu-preview-surface` }, emit: forward, domId: `${id}.surface`, scope: ``, slots: {} } as unknown as NativeProps)} />
    </div>
  )
  if (ctx.inlineOverlays) return panel
  const container = ctx.portal ?? (typeof document === `undefined` ? null : document.body)
  return container ? createPortal(panel, container) : panel
}
