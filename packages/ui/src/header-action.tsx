import type { ComponentProps } from "react"
import { cn } from "./cn"
import { FAB_CHROME_CLASS, FabButton } from "./fab-chrome"

// #EXP-2109 (fixtures/header-actions.json): THE round icon button every page
// header's trailing action is, 44px on phones, 32px from md up. A menu's own
// `Button` trigger wears the class; a plain action is the component.
export const HEADER_ACTION_CLASS = `size-11 md:size-8 shrink-0 rounded-full p-0 text-foreground ${FAB_CHROME_CLASS} hover:bg-glass-active active:bg-glass-active [&_svg]:size-5 md:[&_svg]:size-4`

export function HeaderAction({ className, ...props }: ComponentProps<typeof FabButton>) {
  return <FabButton emphasis="primary" className={cn(HEADER_ACTION_CLASS, className)} {...props} />
}
