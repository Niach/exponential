import { useEffect, useRef } from "react"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"

import { cn } from "./cn"
import { useMediaQuery } from "./use-media-query"

export * from "@exponential_app/ui-react/primitives/skeleton"

const PLACEHOLDER = FEEDBACK_STATES.placeholder
/** A `ListRow` (list density): 12px padding around one 20px line. */
const LIST_ROW_HEIGHT = 44
/** The bars' widths per row (the second bar = the title), so a stack of
 *  placeholders never reads as one ruled block. */
const BAR_WIDTHS: readonly [number, number][] = [
  [56, 168],
  [56, 120],
  [56, 192],
]

/**
 * EXP-1275 (EXP-1259 D8.3) PLACEHOLDER: skeleton rows in the real row
 * geometry (the row's height, a dot where its glyph sits, bars where its text
 * sits, flat gapless rows like `SETTINGS_LIST_CLASS`), the whole block
 * pulsing opacity per `placeholder` (one slow ease-in-out), static under
 * reduce motion. Replaces every "Loading…" text.
 */
export function SkeletonRows({
  rows = PLACEHOLDER.rows,
  rowHeight = LIST_ROW_HEIGHT,
  dot = true,
  className,
}: {
  rows?: number
  rowHeight?: number
  dot?: boolean
  className?: string
}) {
  const node = useRef<HTMLDivElement | null>(null)
  const reduced = useMediaQuery(`(prefers-reduced-motion: reduce)`)
  const [low, high] = PLACEHOLDER.opacity

  useEffect(() => {
    const element = node.current
    if (reduced || !element || typeof element.animate !== `function`) return
    const animation = element.animate([{ opacity: low }, { opacity: high }, { opacity: low }], {
      duration: PLACEHOLDER.pulseMs,
      easing: `ease-in-out`,
      iterations: Infinity,
    })
    return () => animation.cancel()
  }, [reduced, low, high])

  const bar = { height: PLACEHOLDER.barHeight, borderRadius: PLACEHOLDER.radius }
  return (
    <div
      ref={node}
      data-slot="skeleton-rows"
      data-testid={FEEDBACK_STATES.testIds.skeleton}
      aria-busy="true"
      aria-label="Loading"
      className={cn(`flex flex-col`, className)}
      style={{ opacity: reduced ? PLACEHOLDER.reducedOpacity : low }}
    >
      {Array.from({ length: rows }, (_, index) => {
        const [lead, title] = BAR_WIDTHS[index % BAR_WIDTHS.length]
        return (
          <div
            key={index}
            data-part="row"
            className="flex items-center gap-3 px-3"
            style={{ height: rowHeight }}
          >
            {dot && (
              <span
                data-part="dot"
                className="shrink-0 rounded-full bg-muted"
                style={{ width: PLACEHOLDER.dot, height: PLACEHOLDER.dot }}
              />
            )}
            <span className="shrink-0 bg-muted" style={{ ...bar, width: lead }} />
            <span className="min-w-0 shrink bg-muted" style={{ ...bar, width: title }} />
          </div>
        )
      })}
    </div>
  )
}
