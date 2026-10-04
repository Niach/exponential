import type * as React from "react"
import { cn } from "./cn"
import { FAB_CHROME_CLASS } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"

// EXP-1191 — the ONE "jump to the newest row" control of a bottom-anchored
// feed (the run transcript), ×4: a 32px glass circle holding only the down
// arrow, centred 12px above the composer. No label, no shadow: the
// floating-glass hairline is its edge (`FAB_CHROME_CLASS`). It fades and
// scales in while the reader is scrolled up and out once they are back at
// the tail. Desktop `jump_to_bottom_button`, iOS `JumpToBottomButton`,
// Android `JumpToBottomButton`.

const ArrowDownIcon = conceptIcon(`ui-arrow-down`)

export const JUMP_TO_BOTTOM_LABEL = `Jump to bottom`

export function JumpToBottomButton({
  visible,
  className,
  ...props
}: Omit<React.ComponentProps<`button`>, `children`> & {
  /** Scrolled away from the tail — the circle shows. */
  visible: boolean
}) {
  return (
    <button
      type="button"
      aria-label={JUMP_TO_BOTTOM_LABEL}
      title={JUMP_TO_BOTTOM_LABEL}
      aria-hidden={!visible}
      tabIndex={visible ? undefined : -1}
      data-testid="jump-to-bottom"
      data-visible={visible}
      className={cn(
        FAB_CHROME_CLASS,
        `flex size-8 items-center justify-center rounded-full text-foreground/70 outline-none transition-[opacity,scale,background-color,color] duration-fast ease-standard hover:text-foreground active:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50 motion-reduce:transition-none`,
        visible ? `scale-100 opacity-100` : `pointer-events-none scale-90 opacity-0`,
        className
      )}
      {...props}
    >
      <ArrowDownIcon className="size-4" />
    </button>
  )
}
