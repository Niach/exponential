import * as React from "react"
import { Button as BaseButton, buttonVariants } from "@exponential_app/ui-react/primitives/button"

import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"

import { Spinner, useDelayedFlag } from "./feedback"

export { buttonVariants }

type BaseButtonProps = React.ComponentProps<typeof BaseButton>

/** The icon-only sizes: pending swaps the glyph for the spinner, no words. */
const ICON_SIZES = new Set([`icon`, `icon-sm`, `icon-xs`, `icon-lg`])

/**
 * The shadcn Button + EXP-1275's PENDING state (feedback-states.json D8.2):
 * once `pending` has held for `pending.delayMs` the icon slot becomes the
 * Spinner and the label becomes `pendingLabel` (the progressive word,
 * "Starting"), at the idle width as a minimum; it refuses clicks the moment
 * `pending` turns true and is disabled while the state shows.
 */
function Button({
  pending = false,
  pendingLabel,
  pendingDelayMs,
  children,
  disabled,
  onClick,
  style,
  ref,
  ...props
}: BaseButtonProps & {
  pending?: boolean
  pendingLabel?: string
  /** Overrides `pending.delayMs`; `0` shows at once (the styleguide's
   *  static specimen). */
  pendingDelayMs?: number
}) {
  const shown = useDelayedFlag(pending, pendingDelayMs)
  const node = React.useRef<HTMLButtonElement | null>(null)
  const idleWidth = React.useRef<number | null>(null)

  // Measure while idle so the pending state keeps the width it replaced.
  React.useLayoutEffect(() => {
    if (!shown && node.current) idleWidth.current = node.current.offsetWidth || null
  })

  const setRef = React.useCallback(
    (element: HTMLButtonElement | null) => {
      node.current = element
      if (typeof ref === `function`) ref(element)
      else if (ref) ref.current = element
    },
    [ref]
  )

  if (props.asChild) {
    return (
      <BaseButton ref={ref} disabled={disabled} onClick={onClick} style={style} {...props}>
        {children}
      </BaseButton>
    )
  }

  const content = shown ? pendingContent(children, pendingLabel, props.size) : children

  return (
    <BaseButton
      ref={setRef}
      data-pending={shown || undefined}
      data-testid={shown ? FEEDBACK_STATES.testIds.pending : undefined}
      aria-busy={pending || undefined}
      disabled={disabled || shown}
      onClick={pending ? (event) => event.preventDefault() : onClick}
      style={shown && idleWidth.current ? { ...style, minWidth: idleWidth.current } : style}
      {...props}
    >
      {content}
    </BaseButton>
  )
}

function pendingContent(
  children: React.ReactNode,
  pendingLabel: string | undefined,
  size: BaseButtonProps[`size`]
): React.ReactNode {
  const spinner = <Spinner key="spinner" size={FEEDBACK_STATES.pending.spinner} className="text-current" />
  if (size && ICON_SIZES.has(size)) return spinner
  if (pendingLabel !== undefined) return [spinner, <span key="label">{pendingLabel}</span>]
  // No progressive word: the leading glyph (the icon slot) gives way to the
  // spinner and the words stay.
  const parts = React.Children.toArray(children)
  const rest = parts.length > 1 && React.isValidElement(parts[0]) ? parts.slice(1) : parts
  return [spinner, ...rest]
}

export { Button }
