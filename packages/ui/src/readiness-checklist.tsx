import type { ReactNode } from "react"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"

// SLOP-7: the "Ready to code?" checklist's CHROME, shared by the web app
// (`components/coding-readiness-checklist.tsx`) and the styleguide specimen.
// The MODEL (steps, states, copy) is the fixture-locked ×4
// `coding-readiness.json` (`lib/coding-readiness.ts`, desktop
// `domain::coding_readiness`, iOS `CodingReadiness.swift`, Android
// `CodingReadiness.kt`); this file draws one step row, the fix pills and the
// three-slice progress strip, and knows nothing about where a fix leads.
//
// Three row states: `met` (green tick, muted title, a right-aligned detail),
// `current` (amber ring, the FIRST unmet step, its fixes inline on an amber
// wash), `pending` (dashed grey ring, unmet behind the current one, no fixes).

export type ReadinessRowState = `met` | `current` | `pending`

export interface ReadinessRowProps {
  /** The step's concept glyph while unmet (`ui-github`, `ui-branch`, `ui-device`). */
  icon: ReturnType<typeof conceptIcon>
  state: ReadinessRowState
  title: string
  body?: string | null
  /** Right-aligned detail of a met row (the account, the repo, the device). */
  detail?: string | null
  /** The fixes (or the inline picker that replaces them) of the CURRENT row. */
  children?: ReactNode
  /** Forwarded as `data-step` for tests and capture recipes. */
  stepKey?: string
  className?: string
}

const CheckIcon = conceptIcon(`ui-check`)

/** The amber the whole not-ready language shares (dot, capsule, ring). */
export const READINESS_AMBER = `var(--color-amber-400)`

export function ReadinessRowGlyph({
  icon: Glyph,
  state,
}: {
  icon: ReturnType<typeof conceptIcon>
  state: ReadinessRowState
}) {
  if (state === `met`) {
    return (
      <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-400">
        <CheckIcon className="size-4" />
      </span>
    )
  }
  return (
    <span
      className={cn(
        `flex size-7 shrink-0 items-center justify-center rounded-full border-[1.5px]`,
        state === `current`
          ? `border-amber-400 text-amber-400`
          : `border-dashed border-muted-foreground/50 text-muted-foreground`
      )}
    >
      <Glyph className="size-3.5" />
    </span>
  )
}

export function ReadinessRow({
  icon,
  state,
  title,
  body,
  detail,
  children,
  stepKey,
  className,
}: ReadinessRowProps) {
  return (
    <div
      data-step={stepKey}
      data-state={state}
      className={cn(
        `flex gap-3 px-4 py-3`,
        state === `met` ? `items-center` : `items-start`,
        state === `current` && `bg-amber-400/[0.06]`,
        className
      )}
    >
      <ReadinessRowGlyph icon={icon} state={state} />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              `min-w-0 flex-1 truncate text-sm`,
              state === `met`
                ? `text-muted-foreground`
                : `font-medium text-foreground`
            )}
          >
            {title}
          </span>
          {detail && (
            <span className="max-w-[45%] shrink-0 truncate text-xs text-muted-foreground">
              {detail}
            </span>
          )}
        </div>
        {body && (
          <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
            {body}
          </p>
        )}
        {children}
      </div>
    </div>
  )
}

/** The hairline-divided stack of rows. */
export function ReadinessRows({
  children,
  className,
  ...props
}: React.ComponentProps<`div`>) {
  return (
    <div
      className={cn(`flex flex-col divide-y divide-glass-stroke`, className)}
      data-testid="coding-readiness-steps"
      {...props}
    >
      {children}
    </div>
  )
}

/** The fix pills of the current row: the first one is the one to press. */
export function ReadinessFixes({ children }: { children: ReactNode }) {
  return <div className="mt-2 flex flex-wrap items-center gap-2">{children}</div>
}

export function ReadinessFixPill({
  primary,
  icon: Glyph,
  children,
  asChild,
  ...props
}: Omit<React.ComponentProps<typeof Pill>, `size` | `mode` | `primary`> & {
  primary?: boolean
  icon?: ReturnType<typeof conceptIcon>
}) {
  return (
    <Pill size="sm" mode="action" primary={primary} asChild={asChild} {...props}>
      {asChild ? (
        children
      ) : (
        <>
          {Glyph && <Glyph />}
          {children}
        </>
      )}
    </Pill>
  )
}

/** Green met, amber current, grey pending: one slice per step. */
export function ReadinessProgress({
  states,
  className,
}: {
  states: readonly ReadinessRowState[]
  className?: string
}) {
  return (
    <div className={cn(`flex gap-1`, className)} aria-hidden>
      {states.map((state, index) => (
        <span
          key={index}
          data-state={state}
          className={cn(
            `h-1 flex-1 rounded-full`,
            state === `met`
              ? `bg-emerald-400`
              : state === `current`
                ? `bg-amber-400`
                : `bg-glass-stroke-strong`
          )}
        />
      ))}
    </div>
  )
}
