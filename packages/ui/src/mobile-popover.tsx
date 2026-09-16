import * as React from "react"
import {
  Popover,
  PopoverContent as RawPopoverContent,
  PopoverTrigger as RawPopoverTrigger,
} from "./popover"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "./sheet"
import { useIsMobile } from "./use-mobile"
import { cn } from "./cn"

const MobileCtx = React.createContext(false)

export function MobilePopover(props: React.ComponentProps<typeof Popover>) {
  const isMobile = useIsMobile()
  return (
    <MobileCtx.Provider value={isMobile}>
      {isMobile ? <Sheet {...props} /> : <Popover {...props} />}
    </MobileCtx.Provider>
  )
}

export function MobilePopoverTrigger(
  props: React.ComponentProps<typeof RawPopoverTrigger>
) {
  const isMobile = React.useContext(MobileCtx)
  return isMobile ? (
    <SheetTrigger {...props} />
  ) : (
    <RawPopoverTrigger {...props} />
  )
}

type ContentProps = React.ComponentProps<typeof RawPopoverContent> & {
  mobileTitle?: string
  /** Kept on BOTH shells: the overlay is one surface, so a test (or an
   *  assistive tree) must not have to know which one is up. */
  "data-testid"?: string
}

export function MobilePopoverContent({
  className,
  mobileTitle,
  children,
  ...props
}: ContentProps) {
  const isMobile = React.useContext(MobileCtx)
  if (isMobile) {
    return (
      <SheetContent
        side="bottom"
        aria-label={props[`aria-label`]}
        data-testid={props[`data-testid`]}
        className={cn(`flex flex-col gap-0 p-0 pb-[env(safe-area-inset-bottom)]`)}
      >
        {/* Every sheet gets the same header (EXP-687): left, headline, never
            the old micro-caps label. A picker with no title still needs a
            Radix title for the a11y tree. */}
        <SheetHeader className="pb-2">
          <SheetTitle className={mobileTitle ? undefined : `sr-only`}>
            {mobileTitle ?? `Options`}
          </SheetTitle>
        </SheetHeader>
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </SheetContent>
    )
  }
  return (
    <RawPopoverContent className={className} {...props}>
      {children}
    </RawPopoverContent>
  )
}
