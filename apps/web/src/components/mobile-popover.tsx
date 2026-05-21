import * as React from "react"
import {
  Popover,
  PopoverContent as RawPopoverContent,
  PopoverTrigger as RawPopoverTrigger,
} from "@/components/ui/popover"
import {
  Sheet,
  SheetContent,
  SheetTrigger,
} from "@/components/ui/sheet"
import { useIsMobile } from "@/hooks/use-mobile"
import { cn } from "@/lib/utils"

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
        showCloseButton={false}
        className={cn(
          `flex flex-col gap-0 max-h-[85dvh] p-0 pb-[env(safe-area-inset-bottom)] rounded-t-xl`
        )}
      >
        {mobileTitle && (
          <div className="px-4 pt-3 pb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
            {mobileTitle}
          </div>
        )}
        {children}
      </SheetContent>
    )
  }
  return (
    <RawPopoverContent className={className} {...props}>
      {children}
    </RawPopoverContent>
  )
}
