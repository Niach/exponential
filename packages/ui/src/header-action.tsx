import type { ComponentProps } from "react"
import { cn } from "./cn"
import { FAB_CHROME_CLASS, FabButton } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"

// #EXP-2109, #EXP-2168 (fixtures/header-actions.json): THE round icon button
// of every page header, 44px on phones, 32px from md up: every trailing
// action (a menu's trigger too: pass it as the trigger, refs pass through)
// AND the leading back / close (`HeaderBack`). The guard
// (apps/web/src/lib/header-guard.test.ts) keeps the class in this file: a
// header never restyles another button into the circle.
const HEADER_ACTION_CLASS = `size-11 md:size-8 shrink-0 rounded-full p-0 text-foreground ${FAB_CHROME_CLASS} hover:bg-glass-active active:bg-glass-active [&_svg]:size-5 md:[&_svg]:size-4`

export function HeaderAction({ className, ...props }: ComponentProps<typeof FabButton>) {
  return <FabButton emphasis="primary" className={cn(HEADER_ACTION_CLASS, className)} {...props} />
}

const BackIcon = conceptIcon(`ui-back`)
const CloseIcon = conceptIcon(`ui-close`)

/** header-actions.json `leading`: the back (or, `close`, the close of a bar
 *  replacing the header): THE round button at the header's left edge. */
export function HeaderBack({
  close = false,
  label = close ? `Close` : `Back`,
  ...props
}: Omit<ComponentProps<typeof FabButton>, `children`> & { close?: boolean; label?: string }) {
  const Icon = close ? CloseIcon : BackIcon
  return (
    <HeaderAction aria-label={label} title={label} {...props}>
      <Icon />
    </HeaderAction>
  )
}
