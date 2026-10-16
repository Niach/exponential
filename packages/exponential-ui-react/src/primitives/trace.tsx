import * as React from "react"
import { ChevronRight } from "lucide-react"

import { cn } from "./cn"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "./collapsible"

// EXP-1324 — THE Trace: shadcn's Trace (radix-nova) ported onto our
// Collapsible. A LINE = [icon][title][description] in one row; `status`
// `running` shimmers the title, `error` tints the line destructive,
// `pending` dims it. A COLLAPSIBLE line appends the chevron and opens a
// railed content column (tool output, a queue's items). Two sizes, the
// geometry in packages/domain-contract/fixtures/trace.json: `default`
// (text-sm) and `sm` (the session transcript + issue activity scale), set
// ONCE on the `Trace` and read by every part under it. The glyphs come IN as
// children (a concept icon); only the chevron is drawn here.

type TraceSize = `default` | `sm`

function Trace({
  className,
  size = `default`,
  ...props
}: React.ComponentProps<`div`> & { size?: TraceSize }) {
  return (
    <div
      data-slot="trace"
      data-size={size}
      className={cn(
        `group/trace flex w-full min-w-0 flex-col gap-2 data-[size=sm]:gap-1.5 *:has-[>[data-slot=trace-line]]:flex *:has-[>[data-slot=trace-line]]:w-fit *:has-[>[data-slot=trace-line]]:max-w-full *:has-[>[data-slot=trace-line]]:min-w-0 *:has-[>[data-slot=trace-line]]:rounded-md *:has-[>[data-slot=trace-line]]:text-left *:has-[>[data-slot=trace-line]]:outline-none *:has-[>[data-slot=trace-line]]:focus-visible:ring-3 *:has-[>[data-slot=trace-line]]:focus-visible:ring-ring/50`,
        className
      )}
      {...props}
    />
  )
}

function TraceLine({
  className,
  status,
  ...props
}: React.ComponentProps<`span`> & {
  status?: `running` | `error` | `pending` | null
}) {
  return (
    <span
      data-slot="trace-line"
      data-status={status ?? undefined}
      className={cn(
        `flex w-fit max-w-full min-w-0 items-center gap-2 text-sm text-muted-foreground transition-colors duration-fast group-data-[size=sm]/trace:gap-1.5 group-data-[size=sm]/trace:text-xs group-data-[size=sm]/trace:leading-4.5 in-[a,button]:cursor-pointer in-[a:hover,button:hover]:text-foreground data-[status=error]:text-destructive data-[status=error]:in-[a:hover,button:hover]:text-destructive data-[status=pending]:opacity-60 data-[status=running]:*:data-[slot=trace-title]:shimmer`,
        className
      )}
      {...props}
    />
  )
}

function TraceIcon({ className, ...props }: React.ComponentProps<`span`>) {
  return (
    <span
      data-slot="trace-icon"
      aria-hidden="true"
      className={cn(
        `flex size-4 shrink-0 items-center justify-center group-data-[size=sm]/trace:size-3.5 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-full [[data-slot=trace-title]~&]:-ml-1`,
        className
      )}
      {...props}
    />
  )
}

function TraceTitle({ className, ...props }: React.ComponentProps<`span`>) {
  return (
    <span
      data-slot="trace-title"
      className={cn(`min-w-0 truncate motion-reduce:shimmer-none`, className)}
      {...props}
    />
  )
}

function TraceDescription({
  className,
  ...props
}: React.ComponentProps<`span`>) {
  return (
    <span
      data-slot="trace-description"
      className={cn(`shrink-0 text-muted-foreground/70 tabular-nums`, className)}
      {...props}
    />
  )
}

function TraceCollapsible({
  className,
  ...props
}: Omit<React.ComponentProps<typeof Collapsible>, `asChild` | `open`> &
  (
    | { open?: undefined }
    | {
        open: boolean
        onOpenChange: NonNullable<
          React.ComponentProps<typeof Collapsible>[`onOpenChange`]
        >
      }
  )) {
  return (
    <Collapsible
      data-slot="trace-collapsible"
      className={cn(`flex w-full min-w-0 flex-col`, className)}
      {...props}
    />
  )
}

function TraceCollapsibleTrigger({
  className,
  children,
  ...props
}: Omit<React.ComponentProps<typeof CollapsibleTrigger>, `asChild`>) {
  return (
    <CollapsibleTrigger
      data-slot="trace-collapsible-trigger"
      className={cn(
        `group/trace-collapsible-trigger flex w-fit max-w-full min-w-0 cursor-pointer items-center gap-1 rounded-md text-left text-muted-foreground transition-colors duration-fast outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 has-[[data-status=error]]:text-destructive`,
        className
      )}
      {...props}
    >
      {children}
      <TraceIcon
        data-slot="trace-chevron"
        className="transition-transform duration-standard ease-standard motion-reduce:transition-none group-aria-expanded/trace-collapsible-trigger:rotate-90"
      >
        <ChevronRight />
      </TraceIcon>
    </CollapsibleTrigger>
  )
}

function TraceCollapsibleContent({
  className,
  children,
  ...props
}: Omit<React.ComponentProps<typeof CollapsibleContent>, `asChild`>) {
  return (
    <CollapsibleContent
      data-slot="trace-collapsible-content"
      className="min-w-0 overflow-hidden duration-standard data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down motion-reduce:animate-none"
      {...props}
    >
      <div
        data-slot="trace-collapsible-content-inner"
        className={cn(
          `mt-2 mb-1 ml-[calc(--spacing(2)-1px)] min-w-0 border-l pr-1 pl-4 text-sm leading-5 wrap-break-word text-muted-foreground group-data-[size=sm]/trace:mt-1.5 group-data-[size=sm]/trace:ml-[calc(--spacing(1.75)-1px)] group-data-[size=sm]/trace:pl-[calc(--spacing(3.25))] group-data-[size=sm]/trace:text-xs group-data-[size=sm]/trace:leading-4.5`,
          className
        )}
      >
        {children}
      </div>
    </CollapsibleContent>
  )
}

export {
  Trace,
  TraceLine,
  TraceIcon,
  TraceTitle,
  TraceDescription,
  TraceCollapsible,
  TraceCollapsibleTrigger,
  TraceCollapsibleContent,
}
export type { TraceSize }
