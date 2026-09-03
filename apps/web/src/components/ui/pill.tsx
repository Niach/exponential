import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

// EXP-698 — the ONE small capsule. Every chip, badge, header button, filter
// pill, property chip and role label on the web is a `Pill`:
//
//   size  md = 32 tall (`size.controlMd`), sm = 24 tall (`size.controlSm`)
//   mode  action   — a button: card fill under a card hairline, hover active
//         select   — the same button carrying `selected` (active fill + stroke)
//         readonly — a span with the rest chrome; never a target
//
// Label at 70% foreground, 12px medium at `sm` / 14px at `md`; `leading` is a
// glyph slot and `dot` a coloured 6px disc (issue labels). The styleguide's
// `#pill` entry renders the same matrix from the tokens.
//
// The matrix is size × mode; `primary` is a PAINT flag orthogonal to both —
// the accent fill for the one call to action in a row (Create, Start coding,
// Watch). It is meant for `mode="action"`: only there does it take a hover,
// because only there is it a target. Mirrored on the natives as `.primary()` /
// `primary:` / `primary =`.

const pillVariants = cva(
  `inline-flex shrink-0 items-center justify-center whitespace-nowrap rounded-full border border-glass-stroke-card bg-glass-card font-medium text-foreground/70 transition-colors duration-fast [&_svg]:pointer-events-none [&_svg]:shrink-0`,
  {
    variants: {
      size: {
        md: `h-8 gap-1.5 px-3 text-sm [&_svg:not([class*='size-'])]:size-4`,
        sm: `h-6 gap-1 px-2 text-xs [&_svg:not([class*='size-'])]:size-3`,
      },
      mode: {
        action: `cursor-pointer outline-none hover:bg-glass-active hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50`,
        select: `cursor-pointer outline-none hover:bg-glass-active hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 data-[selected=true]:border-glass-stroke-active data-[selected=true]:bg-glass-active data-[selected=true]:text-foreground`,
        readonly: ``,
      },
      primary: {
        true: `border-transparent bg-primary text-primary-foreground`,
        false: ``,
      },
    },
    compoundVariants: [
      {
        mode: `action`,
        primary: true,
        class: `hover:bg-primary/90 hover:text-primary-foreground`,
      },
    ],
    defaultVariants: {
      size: `sm`,
      mode: `readonly`,
      primary: false,
    },
  }
)

type PillVariants = VariantProps<typeof pillVariants>

type PillProps = Omit<React.ComponentProps<`button`>, `children`> &
  PillVariants & {
    /** `select` mode only: the active state. */
    selected?: boolean
    /** A glyph before the label. */
    leading?: React.ReactNode
    /** A 6px coloured disc before the label (issue labels). */
    dot?: string
    /** Render as the single child (a `Link`/`<a>`), like `Button`. */
    asChild?: boolean
    children?: React.ReactNode
  }

function Pill({
  className,
  size = `sm`,
  mode = `readonly`,
  primary = false,
  selected = false,
  leading,
  dot,
  asChild = false,
  children,
  type,
  ...props
}: PillProps) {
  const body = (
    <>
      {dot !== undefined && (
        <span
          aria-hidden
          className="size-1.5 shrink-0 rounded-full"
          style={{ backgroundColor: dot }}
        />
      )}
      {leading}
      {children}
    </>
  )
  const pillClassName = cn(pillVariants({ size, mode, primary }), className)

  if (mode === `readonly`) {
    const { onClick: _onClick, disabled: _disabled, ...spanProps } =
      props as React.ComponentProps<`span`> & {
        onClick?: unknown
        disabled?: unknown
      }
    return (
      <span
        data-slot="pill"
        data-size={size}
        data-mode={mode}
        className={pillClassName}
        {...spanProps}
      >
        {body}
      </span>
    )
  }

  if (asChild) {
    return (
      <Slot.Root
        data-slot="pill"
        data-size={size}
        data-mode={mode}
        data-selected={mode === `select` ? selected : undefined}
        className={pillClassName}
        {...props}
      >
        {React.isValidElement<{ children?: React.ReactNode }>(children)
          ? React.cloneElement(children, undefined, dot !== undefined || leading ? (
              <>
                {dot !== undefined && (
                  <span
                    aria-hidden
                    className="size-1.5 shrink-0 rounded-full"
                    style={{ backgroundColor: dot }}
                  />
                )}
                {leading}
                {children.props.children}
              </>
            ) : children.props.children)
          : children}
      </Slot.Root>
    )
  }

  return (
    <button
      type={type ?? `button`}
      data-slot="pill"
      data-size={size}
      data-mode={mode}
      data-selected={mode === `select` ? selected : undefined}
      aria-pressed={mode === `select` ? selected : undefined}
      className={pillClassName}
      {...props}
    >
      {body}
    </button>
  )
}

export { Pill, pillVariants }
export type { PillProps }
