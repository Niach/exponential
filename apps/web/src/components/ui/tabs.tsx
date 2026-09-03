import * as React from "react"
import { Tabs as TabsPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

// EXP-698 — the ONE segmented control on the web. The Radix tabs below and
// the route-link strips that can't use Radix semantics (team settings nav,
// admin nav) all read these two strings, so the capsule can't drift into
// three hand-mirrored copies again. The strips supply their own active /
// inactive link colours (`activeProps`), which is the only part Radix owns
// through `data-[state=active]` instead.
export const SEGMENTED_LIST = `inline-flex h-9 w-fit items-center justify-center rounded-full border border-glass-stroke-section bg-glass-section p-[3px] text-muted-foreground`

export const SEGMENTED_ITEM = `inline-flex shrink-0 items-center justify-center gap-1.5 rounded-full border border-transparent px-3 py-1 text-sm font-medium whitespace-nowrap transition-colors focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1 focus-visible:outline-ring [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4`

function Tabs({
  className,
  ...props
}: React.ComponentProps<typeof TabsPrimitive.Root>) {
  return (
    <TabsPrimitive.Root
      data-slot="tabs"
      className={cn(`flex flex-col gap-2`, className)}
      {...props}
    />
  )
}

function TabsList({
  className,
  ...props
}: React.ComponentProps<typeof TabsPrimitive.List>) {
  return (
    <TabsPrimitive.List
      data-slot="tabs-list"
      className={cn(SEGMENTED_LIST, className)}
      {...props}
    />
  )
}

function TabsTrigger({
  className,
  ...props
}: React.ComponentProps<typeof TabsPrimitive.Trigger>) {
  return (
    <TabsPrimitive.Trigger
      data-slot="tabs-trigger"
      className={cn(
        SEGMENTED_ITEM,
        `h-[calc(100%-1px)] flex-1 text-foreground dark:text-muted-foreground dark:data-[state=active]:border-glass-stroke-active dark:data-[state=active]:bg-glass-active dark:data-[state=active]:text-foreground disabled:pointer-events-none disabled:opacity-50`,
        className
      )}
      {...props}
    />
  )
}

function TabsContent({
  className,
  ...props
}: React.ComponentProps<typeof TabsPrimitive.Content>) {
  return (
    <TabsPrimitive.Content
      data-slot="tabs-content"
      className={cn(`flex-1 outline-none`, className)}
      {...props}
    />
  )
}

export { Tabs, TabsList, TabsTrigger, TabsContent }
