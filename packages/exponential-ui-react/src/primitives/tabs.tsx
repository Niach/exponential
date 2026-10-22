import * as React from "react"
import { Tabs as TabsPrimitive } from "radix-ui"
import { TABS_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

export { SEGMENTED_ITEM, SEGMENTED_LIST, SEGMENTED_ROW, SEGMENTED_ROW_COMPACT, SEGMENTED_TAB } from "@exponential-at/ui/web/primitives"

function Tabs({
  className,
  ...props
}: React.ComponentProps<typeof TabsPrimitive.Root>) {
  return (
    <TabsPrimitive.Root
      data-slot="tabs"
      className={cn(TABS_CLASSES.root, className)}
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
      className={cn(TABS_CLASSES.list, className)}
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
      className={cn(TABS_CLASSES.trigger, className)}
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
      className={cn(TABS_CLASSES.content, className)}
      {...props}
    />
  )
}

export { Tabs, TabsList, TabsTrigger, TabsContent }
