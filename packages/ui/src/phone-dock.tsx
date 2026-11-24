import * as React from "react"
import { Slot } from "radix-ui"
import { CHATS_COPY, CHATS_SHELL, CHATS_TEST_IDS } from "@exp/domain-contract/chats"
import type { CountBadgeValue } from "@exp/domain-contract/nav-badges"

import { Badge } from "./badge"
import { cn } from "./cn"
import { ExponentialLogo } from "./exponential-logo"
import { conceptIcon } from "./icons.generated"
import { MENU_SURFACE_CLASS } from "./menu-surface"

// #EXP-2109 (fixture `chats.json` `shell.dock`, owner 2026-11-24): THE phone
// bottom bar: docked, full width, one hairline on top, five equal cells:
// Chats · Teams · the Assistant (the Exponential mark on a raised disc, no
// label) · New · Settings. New opens `PhoneNewMenu` above its cell. Twins:
// Android `ui/components` `PhoneDock`, iOS `PhoneDock`.

const DOCK = CHATS_SHELL.dock
const MENU = CHATS_SHELL.newMenu

const CELL_CLASS = `flex h-full min-w-0 flex-1 basis-0 flex-col items-center justify-center outline-none transition-colors duration-fast focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50`

export function PhoneDock({ className, children, ...props }: React.ComponentProps<`nav`>) {
  return (
    <nav
      aria-label="Primary"
      data-slot="phone-dock"
      className={cn(
        `pointer-events-auto w-full border-t border-glass-stroke-card bg-popover px-1.5 pb-[env(safe-area-inset-bottom)]`,
        className
      )}
      {...props}
    >
      <div className="flex items-stretch" style={{ height: DOCK.height }}>
        {children}
      </div>
    </nav>
  )
}

type CellProps = Omit<React.ComponentProps<`button`>, `children`> & {
  selected?: boolean
  /** Render as the single child (a `Link`/`<a>`). */
  asChild?: boolean
  children?: React.ReactElement
}

function Cell({
  label,
  selected,
  asChild,
  body,
  className,
  children,
  ...props
}: CellProps & { label: string; body: React.ReactNode }) {
  const shared = {
    "data-slot": `phone-dock-tab`,
    "data-selected": selected,
    "aria-label": label,
    "aria-current": selected ? (`page` as const) : undefined,
    className: cn(CELL_CLASS, selected ? `text-foreground` : `text-muted-foreground`, className),
  }
  if (asChild && React.isValidElement<{ children?: React.ReactNode }>(children)) {
    return (
      <Slot.Root {...shared} draggable={false} {...(props as Record<string, unknown>)}>
        {React.cloneElement(children, undefined, body)}
      </Slot.Root>
    )
  }
  return (
    <button type="button" {...shared} {...props}>
      {body}
    </button>
  )
}

/** One tab: its icon (`dock.icon` px) over its label, THE count badge on it. */
export function PhoneDockTab({
  label,
  icon,
  badge,
  ...props
}: CellProps & { label: string; icon: React.ReactNode; badge?: CountBadgeValue | null }) {
  return (
    <Cell
      label={label}
      body={
        <span className="flex min-w-0 max-w-full flex-col items-center" style={{ gap: DOCK.gap }}>
          <span className="relative flex items-center justify-center" style={{ height: DOCK.icon }}>
            <span className="flex [&>svg]:size-full" style={{ width: DOCK.icon, height: DOCK.icon }}>
              {icon}
            </span>
            {badge && (
              <Badge
                count={badge.count}
                tone={badge.tone}
                data-slot="phone-dock-badge"
                className="absolute bottom-[calc(100%-10px)] left-[calc(50%+4px)]"
              />
            )}
          </span>
          <span className="min-w-0 max-w-full truncate font-medium leading-none" style={{ fontSize: DOCK.label }}>
            {label}
          </span>
        </span>
      }
      {...props}
    />
  )
}

/** The middle cell: the white Exponential mark on its raised disc. */
export function PhoneDockAssistant({ selected, ...props }: CellProps) {
  return (
    <Cell
      label={CHATS_COPY.tabs.assistant}
      selected={selected}
      className="text-foreground"
      body={
        <span
          data-slot="phone-dock-disc"
          className="flex shrink-0 items-center justify-center rounded-full border border-glass-stroke-card bg-card text-foreground"
          style={{
            width: DOCK.disc,
            height: DOCK.disc,
            // The disc's top edge sits `raise` above the bar's.
            transform: `translateY(${-(DOCK.raise + (DOCK.height - DOCK.disc) / 2)}px)`,
            boxShadow: `0 0 0 ${DOCK.ring}px var(--background)${
              selected ? `, 0 0 ${DOCK.glow}px ${DOCK.ring}px color-mix(in oklab, var(--foreground) 18%, transparent)` : ``
            }`,
          }}
        >
          <ExponentialLogo variant="light" size={DOCK.mark} />
        </span>
      }
      {...props}
    />
  )
}

export type PhoneNewItem = `issue` | `chat`

/** The New cell's popup (`shell.newMenu`): a scrim over the page and the glass
 *  menu `gap` above the bar, ending over the New cell. Mount it inside a
 *  positioned wrapper around the dock. */
export function PhoneNewMenu({
  open,
  onOpenChange,
  onSelect,
  disabled,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSelect: (item: PhoneNewItem) => void
  disabled?: Partial<Record<PhoneNewItem, boolean>>
}) {
  const panel = React.useRef<HTMLDivElement | null>(null)
  React.useEffect(() => {
    if (!open) return
    panel.current?.querySelector<HTMLElement>(`[role=menuitem]:not(:disabled)`)?.focus({ preventScroll: true })
    const onKey = (event: KeyboardEvent) => {
      if (event.key === `Escape`) {
        event.preventDefault()
        onOpenChange(false)
        return
      }
      if (event.key !== `ArrowDown` && event.key !== `ArrowUp`) return
      event.preventDefault()
      const items = [...(panel.current?.querySelectorAll<HTMLElement>(`[role=menuitem]:not(:disabled)`) ?? [])]
      const at = items.indexOf(document.activeElement as HTMLElement)
      const step = event.key === `ArrowDown` ? 1 : -1
      items[(at + step + items.length) % items.length]?.focus()
    }
    window.addEventListener(`keydown`, onKey)
    return () => window.removeEventListener(`keydown`, onKey)
  }, [open, onOpenChange])
  if (!open) return null
  return (
    <>
      <div
        aria-hidden
        data-slot="phone-new-menu-scrim"
        className="pointer-events-auto fixed inset-0 -z-10 bg-black/60"
        onClick={() => onOpenChange(false)}
      />
      <div
        ref={panel}
        role="menu"
        aria-label={CHATS_COPY.tabs.new}
        data-testid={CHATS_TEST_IDS.newMenu}
        // The New cell is the fourth of five: its centre sits 30% from the end.
        className={cn(
          MENU_SURFACE_CLASS,
          `pointer-events-auto absolute right-[30%] translate-x-1/2 overflow-hidden border p-1.5 motion-enter-pop`
        )}
        style={{ bottom: `calc(100% + ${MENU.gap}px)`, width: MENU.width, borderRadius: MENU.radius }}
      >
        {(MENU.items as PhoneNewItem[]).map((item) => {
          const Icon = conceptIcon(MENU.icons[item] as Parameters<typeof conceptIcon>[0])
          return (
            <button
              key={item}
              type="button"
              role="menuitem"
              disabled={disabled?.[item]}
              data-testid={CHATS_TEST_IDS.newMenuRow}
              data-item={item}
              className="flex w-full items-center gap-3.5 px-3.5 text-left text-[15px] text-foreground outline-none transition-colors duration-fast active:bg-glass-active/60 focus-visible:bg-glass-active/60 disabled:opacity-50"
              style={{ height: MENU.rowHeight, borderRadius: MENU.radius - 6 }}
              onClick={() => {
                onOpenChange(false)
                onSelect(item)
              }}
            >
              <Icon className="size-5 shrink-0" />
              <span className="min-w-0 truncate">{CHATS_COPY.newMenu[item]}</span>
            </button>
          )
        })}
      </div>
    </>
  )
}
