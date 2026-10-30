import * as React from "react"
import { CircleCheck, CircleX, Info, TriangleAlert, XIcon } from "lucide-react"
import {
  TOAST_CLASSES,
  TOAST_TOUCH_QUERY,
  createToastStore,
  layoutSize,
  toastPlacement,
  toastStackGeometry,
  type ToastKind,
  type ToastRecord,
  type ToastStore,
} from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import { useMediaQuery } from "./use-media-query"

// FEED-131: THE toast without sonner: the stack of fixtures/toast-stack.json
// (the engine in @exponential_app/eui/web/primitives/toast) painted on plain
// elements. A region `<section>` holds the stack box `<ol>`, one `<li>` per
// toast placed by `--offset`/`--scale`; hover (pointer) or a tap (touch)
// expands the stack and pauses every clock. The Solid twin paints the same.

const KIND_ICONS: Record<ToastKind, React.ComponentType<React.SVGProps<SVGSVGElement>>> = {
  success: CircleCheck,
  error: CircleX,
  info: Info,
  warning: TriangleAlert,
}

/** The default store `toast` writes to and `<Toaster />` reads. */
export const toastStore: ToastStore = createToastStore()

/** `toast("…")`, `toast.success("…", { description, action })`, … on the default store. */
export const toast = toastStore.toast

export type ToasterProps = Omit<React.ComponentProps<`section`>, `children`> & {
  store?: ToastStore
  /** The placement input (default: the `touchMaxWidth` media query). */
  touch?: boolean
  /** The close glyph's label. */
  closeLabel?: string
}

function useToasts(store: ToastStore): readonly ToastRecord[] {
  return React.useSyncExternalStore(store.subscribe, store.toasts, store.toasts)
}

export function Toaster({ store = toastStore, touch, closeLabel = `Close`, className, ...props }: ToasterProps) {
  const toasts = useToasts(store)
  const touchQuery = useMediaQuery(TOAST_TOUCH_QUERY)
  const { placement, anchoredBottom } = toastPlacement(touch ?? touchQuery)
  const [expanded, setExpanded] = React.useState(false)
  const [measured, setMeasured] = React.useState<Record<string, number>>({})
  const items = React.useRef(new Map<string, HTMLLIElement>())
  const open = expanded && toasts.length > 0

  React.useEffect(() => {
    if (open) store.pauseAll()
    else store.resumeAll()
  }, [open, store])

  React.useLayoutEffect(() => {
    const next: Record<string, number> = {}
    for (const t of toasts) {
      const el = items.current.get(t.id)
      if (t.height === undefined && el) next[t.id] = layoutSize(el).height
    }
    setMeasured((prev) => (Object.keys(next).length === Object.keys(prev).length && Object.entries(next).every(([k, v]) => prev[k] === v) ? prev : next))
  }, [toasts])

  const geometry = toastStackGeometry(
    toasts.map((t) => t.height ?? measured[t.id] ?? 0),
    open,
    anchoredBottom
  )

  return (
    <section
      aria-label="Notifications"
      data-slot="toaster"
      data-placement={placement}
      className={cn(TOAST_CLASSES.region, TOAST_CLASSES.regionPlacement[placement], className)}
      {...props}
    >
      <ol
        data-slot="toast-list"
        data-expanded={open ? `true` : `false`}
        className={cn(TOAST_CLASSES.list, TOAST_CLASSES.listPlacement[placement])}
        style={{ height: geometry.height }}
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
        {toasts.map((t, i) => {
          const item = geometry.items[i]!
          const rank = toasts.length - 1 - i
          const Icon = t.kind ? KIND_ICONS[t.kind] : null
          return (
            <li
              key={t.id}
              ref={(el) => {
                if (el) items.current.set(t.id, el)
                else items.current.delete(t.id)
              }}
              role="status"
              aria-live="polite"
              aria-atomic="true"
              data-slot="toast"
              data-kind={t.kind ?? `default`}
              data-front={rank === 0 ? `true` : `false`}
              data-expanded={open ? `true` : `false`}
              data-visible={item.visible ? `true` : `false`}
              data-index={rank}
              className={cn(TOAST_CLASSES.toast, TOAST_CLASSES.toastOrigin[anchoredBottom ? `bottom` : `top`])}
              style={{ "--offset": `${item.offset}px`, "--scale": `${item.scale}`, zIndex: i + 1 } as React.CSSProperties}
            >
              {Icon && (
                <div className={TOAST_CLASSES.iconSlot}>
                  <Icon data-toast-kind={t.kind} className={cn(TOAST_CLASSES.icon, TOAST_CLASSES.kindIcon[t.kind!])} />
                </div>
              )}
              <div className={TOAST_CLASSES.content}>
                <div data-slot="toast-title" className={TOAST_CLASSES.title}>
                  {t.title}
                </div>
                {t.description !== undefined && (
                  <div data-slot="toast-description" className={TOAST_CLASSES.description}>
                    {t.description}
                  </div>
                )}
              </div>
              {t.action && (
                <button
                  type="button"
                  data-slot="toast-action"
                  className={TOAST_CLASSES.action}
                  onClick={(event) => {
                    t.action?.onClick?.(event.nativeEvent)
                    store.dismiss(t.id)
                  }}
                >
                  {t.action.label}
                </button>
              )}
              <button type="button" data-slot="toast-close" aria-label={closeLabel} className={TOAST_CLASSES.close} onClick={() => store.dismiss(t.id)}>
                <XIcon />
              </button>
            </li>
          )
        })}
      </ol>
    </section>
  )
}

export { createToastStore, toastStackGeometry, toastPlacement, TOAST_CONSTANTS, TOAST_KINDS, TOAST_TOUCH_QUERY } from "@exponential_app/eui/web/primitives"
export type { ToastKind, ToastOptions, ToastRecord, ToastStore, ToastAction, ToastPlacement } from "@exponential_app/eui/web/primitives"
