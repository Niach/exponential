import * as React from "react"
import { Slot } from "radix-ui"

import { cn } from "./cn"
import { FAB_CHROME_CLASS } from "./fab-chrome"

// Thread 58cb32df (#EXP-1373, fixture `chats.json` `shell.tabs`): the PHONE
// bottom bar's tab capsule — on Android and web-mobile the look-alike of the
// iOS Liquid Glass tab bar, painted on THE floating chrome recipe
// (`FAB_CHROME_CLASS`): each tab an icon over its label, the selected one on
// the glass-active pill, an optional dot (Chats = unread). 52px tall, the
// height of the two-armed new chat / new issue capsule beside it. iOS = the
// system tab bar (ExpUI `PhoneTabBar` look), Android `ui/components`
// `PhoneTabBar`.

export const PHONE_TAB_BAR_CLASS = `pointer-events-auto flex h-[52px] items-center rounded-full p-1 ${FAB_CHROME_CLASS}`

export function PhoneTabBar({
  className,
  ...props
}: React.ComponentProps<`nav`>) {
  return (
    <nav
      aria-label="Primary"
      data-slot="phone-tab-bar"
      className={cn(PHONE_TAB_BAR_CLASS, className)}
      {...props}
    />
  )
}

/** The dot a tab wears (top-right of its icon). */
export function PhoneTabDot({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      data-slot="phone-tab-dot"
      className={cn(
        `pointer-events-none absolute right-3 top-1 size-2 rounded-full ring-2 ring-popover`,
        className
      )}
    />
  )
}

export function PhoneTab({
  label,
  icon,
  selected = false,
  dot,
  asChild = false,
  className,
  children,
  ...props
}: Omit<React.ComponentProps<`button`>, `children`> & {
  label: string
  /** The glyph (a concept icon at size-5, or the team's avatar). */
  icon: React.ReactNode
  selected?: boolean
  /** The dot's colour class (`bg-yellow-400`); absent = none. */
  dot?: string | null
  /** Render as the single child (a `Link`/`<a>`); its own children are
   *  replaced by the tab's body. */
  asChild?: boolean
  children?: React.ReactElement
}) {
  const body = (
    <>
      <span className="flex h-5 items-center justify-center">{icon}</span>
      <span className="max-w-[64px] truncate text-[10px] font-medium leading-none">{label}</span>
      {dot && <PhoneTabDot className={dot} />}
    </>
  )
  const tabClassName = cn(
    `relative flex h-11 min-w-14 flex-col items-center justify-center gap-1 rounded-full px-2 outline-none transition-colors duration-fast focus-visible:ring-[3px] focus-visible:ring-ring/50`,
    selected ? `bg-glass-active text-foreground` : `text-muted-foreground`,
    className
  )
  if (asChild && React.isValidElement<{ children?: React.ReactNode }>(children)) {
    return (
      <Slot.Root
        data-slot="phone-tab"
        data-selected={selected}
        aria-label={label}
        aria-current={selected ? `page` : undefined}
        className={tabClassName}
        {...(props as Record<string, unknown>)}
      >
        {React.cloneElement(children, undefined, body)}
      </Slot.Root>
    )
  }
  return (
    <button
      type="button"
      data-slot="phone-tab"
      data-selected={selected}
      aria-label={label}
      aria-current={selected ? `page` : undefined}
      className={tabClassName}
      {...props}
    >
      {body}
    </button>
  )
}
