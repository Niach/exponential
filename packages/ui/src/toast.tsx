import type { CSSProperties, ReactNode } from "react"
import { Toaster as Sonner, type ToasterProps } from "sonner"
import toastStack from "@exp/domain-contract/fixtures/toast-stack.json"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { useMediaQuery } from "./use-media-query"

// EXP-1031 — THE toast. One component over sonner 2.0.7, the numbers from
// packages/domain-contract/fixtures/toast-stack.json (desktop, iOS and
// Android draw the same card from the same file). The card is the opaque
// glass card fill with the card hairline at radius lg, NO shadow; the kind's
// colour sits on the ICON alone, the text stays foreground. Sonner keeps the
// behaviour: the stack, hover expand, swipe and the paused clock. The class
// constants below are shared by the live toast (`toastOptions.classNames`)
// and the styleguide's `ToastSpecimen`, so the two cannot disagree.

export { toast } from "sonner"

export const TOAST_CONSTANTS = toastStack.constants

export type ToastKind = `success` | `error` | `info` | `warning`

export const TOAST_KINDS = TOAST_CONSTANTS.kinds as readonly ToastKind[]

/** The card. `w-[356px]` = `constants.width` (a literal, so Tailwind scans
 *  it); sonner's own phone rule narrows it to the viewport minus the mobile
 *  offset. A collapsed back toast hides its contents behind the front one. */
export const TOAST_CLASS = `flex w-[356px] items-start gap-3 rounded-lg border border-glass-stroke-card bg-glass-card-opaque p-4 font-sans text-sm text-foreground data-[expanded=false]:data-[front=false]:*:opacity-0`

export const TOAST_CONTENT_CLASS = `flex min-w-0 flex-1 flex-col gap-0.5`

export const TOAST_TITLE_CLASS = `font-medium leading-5 text-foreground`

export const TOAST_DESCRIPTION_CLASS = `leading-5 text-muted-foreground`

export const TOAST_ACTION_CLASS = `inline-flex h-6 shrink-0 cursor-pointer items-center rounded-md bg-primary px-2 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90`

/** The close glyph: last in the row. The `!`s beat sonner's dark-theme
 *  close-button paint, which applies even to an unstyled toast. */
export const TOAST_CLOSE_CLASS = `order-last inline-flex size-5 shrink-0 cursor-pointer items-center justify-center rounded-sm border-0! bg-transparent! text-muted-foreground! transition-colors hover:text-foreground! [&_svg]:size-3.5`

export const TOAST_ICON_SLOT_CLASS = `flex h-5 shrink-0 items-center`

export const TOAST_ICON_CLASS = `size-4 shrink-0`

/** The ONLY place a toast wears its kind. */
export const TOAST_KIND_ICON_CLASS: Record<ToastKind, string> = {
  success: `text-green-500`,
  error: `text-red-500`,
  info: `text-blue-500`,
  warning: `text-yellow-500`,
}

export const TOAST_KIND_CONCEPT = {
  success: `ui-success`,
  error: `ui-error`,
  info: `ui-info`,
  warning: `ui-warning`,
} as const satisfies Record<ToastKind, string>

export function ToastKindIcon({ kind }: { kind: ToastKind }): ReactNode {
  const Icon = conceptIcon(TOAST_KIND_CONCEPT[kind])
  return (
    <Icon
      data-toast-kind={kind}
      aria-hidden
      className={cn(TOAST_ICON_CLASS, TOAST_KIND_ICON_CLASS[kind])}
    />
  )
}

const CloseIcon = conceptIcon(`ui-close`)

type ToastPosition = NonNullable<ToasterProps["position"]>

/** Placement per input (fixture `placement`): bottom-right on a pointer,
 *  top-centre on touch, i.e. up to and including `touchMaxWidth` (sonner's own phone
 *  breakpoint, where it lays the card full width minus `mobileOffset`). */
export const TOAST_PLACEMENT = TOAST_CONSTANTS.placement as { pointer: ToastPosition; touch: ToastPosition }

export const TOAST_TOUCH_QUERY = `(max-width: ${TOAST_CONSTANTS.touchMaxWidth}px)`

/** Everything the app's one `<Toaster />` passes to sonner (the pointer
 *  placement; `Toaster` swaps in `TOASTER_TOUCH_PROPS` on touch). */
export const TOASTER_PROPS = {
  theme: `dark`,
  position: TOAST_PLACEMENT.pointer,
  gap: TOAST_CONSTANTS.gap,
  visibleToasts: TOAST_CONSTANTS.visible,
  duration: TOAST_CONSTANTS.durationMs,
  offset: TOAST_CONSTANTS.viewportOffset,
  // `mobileViewportOffset` from every edge; the top one also clears the
  // status bar (the app runs `viewport-fit=cover`).
  mobileOffset: {
    top: `calc(env(safe-area-inset-top, 0px) + ${TOAST_CONSTANTS.mobileViewportOffset}px)`,
    right: TOAST_CONSTANTS.mobileViewportOffset,
    bottom: TOAST_CONSTANTS.mobileViewportOffset,
    left: TOAST_CONSTANTS.mobileViewportOffset,
  },
  closeButton: true,
  icons: {
    success: <ToastKindIcon kind="success" />,
    error: <ToastKindIcon kind="error" />,
    info: <ToastKindIcon kind="info" />,
    warning: <ToastKindIcon kind="warning" />,
    close: <CloseIcon aria-hidden />,
  },
  toastOptions: {
    unstyled: true,
    classNames: {
      toast: TOAST_CLASS,
      content: TOAST_CONTENT_CLASS,
      title: TOAST_TITLE_CLASS,
      description: TOAST_DESCRIPTION_CLASS,
      icon: TOAST_ICON_SLOT_CLASS,
      actionButton: TOAST_ACTION_CLASS,
      cancelButton: TOAST_ACTION_CLASS,
      closeButton: TOAST_CLOSE_CLASS,
    },
  },
} satisfies ToasterProps

/** The touch arm: a top stack, the swipe that dismisses goes up or
 *  sideways (sonner's default for `top-center` is up only). */
export const TOASTER_TOUCH_PROPS = {
  ...TOASTER_PROPS,
  position: TOAST_PLACEMENT.touch,
  swipeDirections: [`top`, `left`, `right`],
} satisfies ToasterProps

export function Toaster(): ReactNode {
  const touch = useMediaQuery(TOAST_TOUCH_QUERY)
  return <Sonner {...(touch ? TOASTER_TOUCH_PROPS : TOASTER_PROPS)} />
}

// ── The specimen ────────────────────────────────────────────────────────
//
// A live `<Toaster>` renders nothing to static markup, so the styleguide
// draws the toast AT REST on plain elements wearing the same constants:
// three equal cards, oldest first, from the fixture's numbers (`peek`,
// `scaleStep`, `gap`). `collapsed`/`expanded` = the pointer placement,
// BOTTOM-anchored: an older card shrinks from its bottom edge and peeks out
// ABOVE the front one. `collapsed-touch` = the touch placement, TOP-anchored:
// the front card on top, an older one shrinks from its top edge and peeks
// out BELOW it. The app never renders it.

/** The specimen's card height (the fixture's equal-height cases use 60). */
export const TOAST_SPECIMEN_HEIGHT = 60

export type ToastStackItem = { offset: number; scale: number }

/** Stack geometry for `count` equal cards of `height`, oldest first;
 *  `offset` = the card's top inside the box. Bottom-anchored (pointer)
 *  unless `fromTop` (touch: the older cards peek below the front one). */
export function toastSpecimenStack(
  expanded: boolean,
  count = 3,
  height = TOAST_SPECIMEN_HEIGHT,
  fromTop = false
): { height: number; items: ToastStackItem[] } {
  const { peek, gap, scaleStep } = TOAST_CONSTANTS
  const items = Array.from({ length: count }, (_, i) => {
    const rank = count - 1 - i
    if (expanded) return { offset: (fromTop ? rank : i) * (height + gap), scale: 1 }
    return { offset: (fromTop ? rank : i) * peek, scale: Math.round((1 - rank * scaleStep) * 100) / 100 }
  })
  const box = expanded ? count * height + (count - 1) * gap : height + (count - 1) * peek
  return { height: box, items }
}

type SpecimenCard = { kind?: ToastKind; title: string; description?: string; action?: string }

const STACK_CARDS: SpecimenCard[] = [
  { kind: `info`, title: `Session started on studio-mac` },
  { kind: `error`, title: `Could not merge the pull request` },
  { kind: `success`, title: `Issue EXP-1031 created` },
]

function ToastCard({
  card,
  hidden,
  style,
  className,
}: {
  card: SpecimenCard
  hidden?: boolean
  style?: CSSProperties
  className?: string
}): ReactNode {
  const body = hidden ? `opacity-0` : undefined
  return (
    <div data-slot="toast" data-kind={card.kind ?? `default`} style={style} className={cn(TOAST_CLASS, className)}>
      {card.kind && (
        <div className={cn(TOAST_ICON_SLOT_CLASS, body)}>
          <ToastKindIcon kind={card.kind} />
        </div>
      )}
      <div className={cn(TOAST_CONTENT_CLASS, body)}>
        <div className={TOAST_TITLE_CLASS}>{card.title}</div>
        {card.description !== undefined && (
          <div className={TOAST_DESCRIPTION_CLASS}>{card.description}</div>
        )}
      </div>
      {card.action !== undefined && (
        <span className={cn(TOAST_ACTION_CLASS, body)}>{card.action}</span>
      )}
      <span className={cn(TOAST_CLOSE_CLASS, body)}>
        <CloseIcon aria-hidden />
      </span>
    </div>
  )
}

export type ToastSpecimenProps =
  | { stack: `collapsed` | `expanded` | `collapsed-touch`; className?: string }
  | {
      stack?: undefined
      kind?: ToastKind
      title: string
      description?: string
      /** The action button's label. */
      action?: string
      className?: string
    }

export function ToastSpecimen(props: ToastSpecimenProps): ReactNode {
  if (props.stack) {
    const expanded = props.stack === `expanded`
    const fromTop = props.stack === `collapsed-touch`
    const layout = toastSpecimenStack(expanded, 3, TOAST_SPECIMEN_HEIGHT, fromTop)
    return (
      <div
        data-slot="toast-specimen"
        data-stack={props.stack}
        className={cn(`relative w-[356px]`, props.className)}
        style={{ height: layout.height }}
      >
        {layout.items.map((item, index) => {
          const front = index === layout.items.length - 1
          return (
            <ToastCard
              key={index}
              card={STACK_CARDS[index]!}
              hidden={!expanded && !front}
              className="absolute left-0"
              style={{
                top: item.offset,
                height: TOAST_SPECIMEN_HEIGHT,
                zIndex: index + 1,
                transform: `scale(${item.scale})`,
                transformOrigin: fromTop ? `top center` : `bottom center`,
              }}
            />
          )
        })}
      </div>
    )
  }
  return (
    <div data-slot="toast-specimen" className={props.className}>
      <ToastCard
        card={{
          kind: props.kind,
          title: props.title,
          description: props.description,
          action: props.action,
        }}
      />
    </div>
  )
}
