import * as React from "react"

import { cn } from "@/lib/utils"
import { conceptIcon } from "@/lib/icons.generated"
import { Button } from "@/components/ui/button"

// EXP-790: the send glyph is ONE concept everywhere a composer renders —
// comments, support replies, steering, the chat launcher — and a working
// agent swaps it for the stop glyph (`ui-stop`). Both resolve through the
// shared registry (packages/icons/icons.json), so the natives draw the same.
const UiSubmitIcon = conceptIcon(`ui-submit`)
const UiStopIcon = conceptIcon(`ui-stop`)

// EXP-698 — the ONE composer card. Comments, agent steering and support
// replies are the same object: a 16-radius glass card laid out as a COLUMN —
// an optional leading row (the support Reply/Note toggle), an optional strip
// (pending attachments / images), the field itself, then the tool row with the
// submit glyph pushed to the far right. It owns CHROME AND LAYOUT ONLY; every
// caller keeps its own field, upload and send logic.
//
// `opaque` is for a composer that FLOATS over content (a popover, a sheet):
// the card fill composited over the solid popover surface instead of over
// whatever scrolls behind it, under the stronger hairline.

interface ComposerProps extends React.ComponentProps<`div`> {
  /** Floating over content: solid card composite + the strong hairline. */
  opaque?: boolean
  /** A row above the field — the support composer's Reply/Note pills. */
  leading?: React.ReactNode
  /** Pending attachments / images, above the field. */
  strip?: React.ReactNode
  /** The left tool cluster (`ComposerTool` buttons). */
  tools?: React.ReactNode
  /** The right cluster — the round submit glyph, and an inline Cancel. */
  submit?: React.ReactNode
  /** The field. */
  children?: React.ReactNode
}

function Composer({
  opaque = false,
  leading,
  strip,
  tools,
  submit,
  className,
  children,
  ...props
}: ComposerProps) {
  return (
    <div
      data-slot="composer"
      className={cn(
        `flex flex-col rounded-xl border`,
        opaque
          ? `border-glass-stroke-strong bg-glass-card-opaque`
          : `border-glass-stroke-card bg-glass-card`,
        className
      )}
      {...props}
    >
      {leading && (
        <div className="flex flex-wrap items-center gap-1 px-1.5 pt-1.5">
          {leading}
        </div>
      )}
      {strip}
      {children}
      <div className="flex items-center gap-0.5 px-1.5 pb-1.5">
        {tools}
        <div className="ml-auto flex items-center gap-1">{submit}</div>
      </div>
    </div>
  )
}

/** One 24px tool button in the composer's tool row. */
function ComposerTool({
  className,
  ...props
}: React.ComponentProps<typeof Button>) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      className={cn(`text-muted-foreground`, className)}
      {...props}
    />
  )
}

interface ComposerSubmitProps extends React.ComponentProps<typeof Button> {
  /** EXP-790: the agent is working and there is nothing to send — the glyph
   *  is Stop (`ui-stop`) and the click interrupts the turn. */
  stop?: boolean
}

/** The round send glyph. `ui-submit` is itself a circled arrow — a filled
 *  button around it would draw a second ring, so the chrome stays a ghost
 *  circle and the glyph carries the tint. The glyph is the component's own
 *  (`ui-submit`, or `ui-stop` with `stop`); `children` overrides it for a
 *  caller's transient state, like the chat launcher's spinner. */
function ComposerSubmit({
  stop = false,
  className,
  children,
  ...props
}: ComposerSubmitProps) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={stop ? `Stop` : `Send`}
      title={stop ? `Stop` : `Send`}
      className={cn(
        `shrink-0 rounded-full disabled:opacity-40`,
        stop
          ? `text-foreground hover:text-foreground`
          : `text-primary hover:text-primary`,
        className
      )}
      {...props}
    >
      {children ??
        (stop ? (
          <UiStopIcon className="!size-6" />
        ) : (
          <UiSubmitIcon className="!size-6" />
        ))}
    </Button>
  )
}

export { Composer, ComposerSubmit, ComposerTool }
export type { ComposerProps, ComposerSubmitProps }
