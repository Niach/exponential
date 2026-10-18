import { useLayoutEffect, useRef, useState, type ReactNode } from "react"
import {
  ASK_DOCK_BODY_MAX_HEIGHT_FRACTION,
  ASK_DOCK_STACK_GAP,
  ASK_DOCK_TEST_IDS,
  type AskDockRow,
  type AskDockView,
} from "@exp/domain-contract/ask-dock"
import { AskCard } from "./ask-card"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

// THE ask dock ×web (fixtures/ask-dock.json ×4): the question/plan cards
// waiting on the viewer take the composer's slot, in the session view and
// the thread alike. `single`/`focused` = THE big card (the host renders it,
// `AskCard` + `AskPrompt`, the `back` arrow leading its header when
// focused); `stack` = compact AskCards, a tap grows one into the big card.
// The dock's height animates over motion `standard`; the swapped content
// enters with `motion-enter` (reduce motion: no height animation, a fade).

const OpenIcon = conceptIcon(`ui-chevron-right`)

export function AskDock({
  view,
  renderCard,
  onFocus,
  onBack,
  className,
}: {
  view: AskDockView
  /** The big card for `id`; `onBack` is set when it was focused out of a
   *  stack (the card passes it to `AskCard`). */
  renderCard: (id: string, chrome: { onBack?: () => void }) => ReactNode
  onFocus: (id: string) => void
  onBack: () => void
  className?: string
}) {
  const contentRef = useRef<HTMLDivElement | null>(null)
  const [height, setHeight] = useState<number | null>(null)
  useLayoutEffect(() => {
    const node = contentRef.current
    if (!node) return
    const measure = () => setHeight(node.offsetHeight)
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    return () => observer.disconnect()
  }, [view.mode])

  if (view.mode === `none`) return null
  // The content key: a swap (stack ⇄ a card) re-mounts and fades in.
  const contentKey = view.cardId === null ? `stack` : `card:${view.cardId}`
  return (
    <div
      data-testid={ASK_DOCK_TEST_IDS.dock}
      data-mode={view.mode}
      className={cn(
        `overflow-hidden transition-[height] duration-standard ease-standard motion-reduce:transition-none`,
        className
      )}
      style={height === null ? undefined : { height }}
    >
      <div ref={contentRef}>
        <div key={contentKey} className="motion-enter">
          {view.cardId !== null ? (
            <div data-testid={ASK_DOCK_TEST_IDS.card}>
              {renderCard(view.cardId, view.back ? { onBack } : {})}
            </div>
          ) : (
            <div className="flex flex-col" style={{ gap: ASK_DOCK_STACK_GAP }}>
              {view.rows.map((row) => (
                <AskDockStackCard key={row.id} row={row} onOpen={() => onFocus(row.id)} />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

/** A compact card in the stack: the AskCard chrome (icon + label + the
 *  session caption) over the one-line title and the open chevron. */
export function AskDockStackCard({ row, onOpen }: { row: AskDockRow; onOpen: () => void }) {
  return (
    <AskCard
      plan={row.kind === `plan`}
      label={row.label}
      meta={
        row.caption ? (
          <span className="min-w-0 truncate text-xs text-muted-foreground">{row.caption}</span>
        ) : undefined
      }
      role="button"
      tabIndex={0}
      aria-label={`${row.label}: ${row.title}`}
      data-testid={ASK_DOCK_TEST_IDS.row}
      data-kind={row.kind}
      className="cursor-pointer transition-colors duration-fast outline-none hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50"
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          onOpen()
        }
      }}
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-sm text-foreground/90">{row.title}</span>
        <OpenIcon className="size-3.5 shrink-0 text-muted-foreground" />
      </div>
    </AskCard>
  )
}

/** The big card's body inside the dock: it never grows past the fixture's
 *  `bodyMaxHeightFraction` of the viewport and scrolls inside. */
export function AskDockBody({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cn(`overflow-y-auto overscroll-contain`, className)}
      style={{ maxHeight: `${ASK_DOCK_BODY_MAX_HEIGHT_FRACTION * 100}vh` }}
    >
      {children}
    </div>
  )
}

