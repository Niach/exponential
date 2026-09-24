import type { LucideIcon } from "lucide-react"
import { createContext, useContext, type ReactNode } from "react"

import { cn } from "./cn"
import { GlassRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// EXP-1029 contract — sub-shell navigation for the settings shell (EXP-994's
// `GlassGroup` rows). EXP-1020 implements it and uses it for "Workflow
// settings" inside the device settings.
//
// A `SubShell` is a ROW ENTRY inside a card. Opening it slides a child page
// in place of the WHOLE card — not a nested card, not a dialog — with a back
// button on top; the child page is the same shell (its own `GlassGroup`s of
// rows), so a sub-shell may hold another sub-shell. `SubShellHost` is the
// card boundary the page replaces: it renders its children at rest and, once
// a row inside opened, the row's page instead.
//
// Platform siblings (same names): IDE `ui::sub_shell`, iOS
// `ExpUI/Sources/SubShell.swift`, Android `ui/components/SubShell.kt`.
//
// This file is the CONTRACT: the prop types, a host that only ever renders
// its card, and a row stub that never opens. `sub-shell.test.tsx` carries
// the behaviour as a skipped table.

export interface SubShellProps {
  /** The row's label. */
  label: ReactNode
  /** A muted second line under the label. */
  description?: ReactNode
  /** A leading concept glyph. */
  icon?: LucideIcon
  /** A muted trailing summary (`opus · fable`). */
  value?: ReactNode
  /** The child page's title; defaults to a string `label`. */
  title?: string
  /** Controlled open state; uncontrolled when absent. */
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** The child page: the same shell — `GlassGroup`s of rows. */
  children: ReactNode
  disabled?: boolean
  className?: string
  "data-testid"?: string
}

export interface SubShellHostProps {
  /** The card at rest: `GlassSectionHeader`s + `GlassGroup`s. */
  children: ReactNode
  className?: string
}

export interface SubShellPage {
  title: string
  content: ReactNode
}

interface SubShellContextValue {
  /** Slides `page` in place of the host's card. */
  open: (page: SubShellPage) => void
  /** Returns to the card (or the enclosing page). */
  back: () => void
  /** Whether a page is open right now. */
  isOpen: boolean
}

const SubShellContext = createContext<SubShellContextValue | null>(null)

/** Reads the enclosing host, for a page that wants to close itself. */
export function useSubShell(): SubShellContextValue {
  const value = useContext(SubShellContext)
  if (!value) throw new Error(`useSubShell needs a SubShellHost above it`)
  return value
}

const ChevronGlyph = conceptIcon(`ui-chevron-right`)

/**
 * The card boundary a sub-shell page replaces. Contract stub: renders its
 * card and hands rows a no-op host (EXP-1020 keeps the page stack here).
 */
export function SubShellHost({ children, className }: SubShellHostProps) {
  const value: SubShellContextValue = {
    open: () => {},
    back: () => {},
    isOpen: false,
  }
  return (
    <SubShellContext.Provider value={value}>
      <div data-slot="sub-shell-host" className={cn(`flex flex-col gap-6`, className)}>
        {children}
      </div>
    </SubShellContext.Provider>
  )
}

/**
 * A row entry that slides its child page in place of the whole card.
 * Contract stub: renders the row (label, description, value, chevron)
 * under `data-slot="sub-shell"`; opening does nothing until EXP-1020.
 */
export function SubShell({
  label,
  description,
  icon: Icon,
  value,
  disabled = false,
  className,
  "data-testid": testId,
}: SubShellProps) {
  return (
    <GlassRow
      interactive={!disabled}
      data-slot="sub-shell"
      data-testid={testId}
      aria-disabled={disabled || undefined}
      className={cn(`rounded-none border-0`, disabled && `opacity-50`, className)}
    >
      {Icon ? <Icon className="size-4 shrink-0 text-muted-foreground" /> : null}
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm">{label}</span>
        {description ? (
          <span className="truncate text-xs text-muted-foreground">{description}</span>
        ) : null}
      </div>
      {value ? <span className="truncate text-sm text-muted-foreground">{value}</span> : null}
      <ChevronGlyph className="size-4 shrink-0 text-muted-foreground" />
    </GlassRow>
  )
}
