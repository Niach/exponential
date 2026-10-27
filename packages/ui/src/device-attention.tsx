import type { ReactNode } from "react"

import { Button } from "./button"
import { cn } from "./cn"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./hover-card"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"
import { Popover, PopoverContent, PopoverTrigger } from "./popover"
import { useIsMobile } from "./use-mobile"

// A-16: THE attention icon a Devices row carries instead of writing its
// warnings inline (`fixtures/devices-list.json`). The row passes the items
// `deviceListRow` derived; nothing renders when there are none. A pointer
// opens the card on hover, touch on tap; each line is the item's text and at
// most one action pill. Hand-mirrored ×4 (desktop `machines.rs`, iOS
// `DevicesList`, Android `DevicesList`).

const WarningIcon = conceptIcon(`ui-warning`)

export interface DeviceAttentionLine {
  key: string
  text: string
  action?: { label: string; onSelect: () => void; busy?: boolean } | null
}

function AttentionCard({ items }: { items: readonly DeviceAttentionLine[] }) {
  return (
    <div data-slot="device-attention-card" className="flex flex-col gap-1">
      {items.map((item) => (
        <div key={item.key} className="flex min-w-0 items-center gap-2 py-0.5">
          <WarningIcon className="size-3.5 shrink-0 text-amber-400" />
          <span className="min-w-0 flex-1 text-xs" title={item.text}>
            {item.text}
          </span>
          {item.action && (
            <Pill
              mode="action"
              disabled={item.action.busy}
              onClick={item.action.onSelect}
            >
              {item.action.label}
            </Pill>
          )}
        </div>
      ))}
    </div>
  )
}

export function DeviceAttention({
  items,
  label,
  defaultOpen,
  className,
}: {
  items: readonly DeviceAttentionLine[]
  /** The trigger's accessible name (`Needs attention`). */
  label: string
  /** Styleguide specimens paint the card open. */
  defaultOpen?: boolean
  className?: string
}) {
  const touch = useIsMobile()
  if (items.length === 0) return null
  const trigger: ReactNode = (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      title={touch ? undefined : label}
      className={cn(`shrink-0 text-amber-400 hover:text-amber-400`, className)}
      onClick={(event) => event.stopPropagation()}
    >
      <WarningIcon />
    </Button>
  )
  const contentClass = `w-80 p-2.5`
  if (touch) {
    return (
      <Popover defaultOpen={defaultOpen}>
        <PopoverTrigger asChild>{trigger}</PopoverTrigger>
        <PopoverContent align="end" className={contentClass}>
          <AttentionCard items={items} />
        </PopoverContent>
      </Popover>
    )
  }
  return (
    <HoverCard openDelay={100} closeDelay={150} defaultOpen={defaultOpen}>
      <HoverCardTrigger asChild>{trigger}</HoverCardTrigger>
      <HoverCardContent align="end" sideOffset={6} className={contentClass}>
        <AttentionCard items={items} />
      </HoverCardContent>
    </HoverCard>
  )
}
