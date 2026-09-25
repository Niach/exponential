import type { CSSProperties, ReactNode } from "react"
import { ChevronRightIcon, type LucideIcon } from "lucide-react"

import { cn } from "./cn"
import {
  MENU_CHEVRON_CLASS,
  MENU_CONTENT_CLASS,
  MENU_ITEM_CLASS,
  MENU_LABEL_CLASS,
  MENU_SEPARATOR_CLASS,
  MENU_SHORTCUT_CLASS,
  MENU_SUB_TRIGGER_CLASS,
  MENU_SURFACE_CLASS,
  MENU_VALUE_CLASS,
} from "./menu-surface"

// EXP-1074 — a menu at REST, for the styleguide.
//
// A closed Radix portal renders nothing to static markup, so the styleguide's
// menu entries could only ever show a lookalike. This draws the menu's rows
// on plain elements wearing the SAME class constants the Radix items wear
// (`menu-surface.ts`): the surface, the geometry, the paint and the item
// order are the product's; only the behaviour is missing. The app never
// renders it.

export type MenuSpecimenRow =
  | { kind: `header`; identifier: string; title: string }
  | { kind: `separator` }
  | {
      kind: `item`
      label: string
      icon?: LucideIcon
      destructive?: boolean
      shortcut?: string
    }
  | {
      kind: `submenu`
      label: string
      icon?: LucideIcon
      destructive?: boolean
      /** The current value beside the label ("Backlog"). */
      value?: string
    }

export type MenuDensity = `pointer` | `touch`

const DENSITY_VARS = [
  `item-height`,
  `item-padding-x`,
  `item-gap`,
  `icon-size`,
  `surface-padding`,
  `min-width`,
  `max-width`,
] as const

/** Pins a specimen to ONE density regardless of the viewport — the live
 *  `--menu-*` set follows the breakpoint, so this is how the two sit side
 *  by side on one page. */
export function menuDensityStyle(density: MenuDensity): CSSProperties {
  return Object.fromEntries(
    DENSITY_VARS.map((name) => [`--menu-${name}`, `var(--menu-${density}-${name})`])
  ) as CSSProperties
}

/** The identifier + title band the issue menu opens with. */
export function MenuHeaderBody({
  identifier,
  title,
}: {
  identifier: string
  title: string
}): ReactNode {
  return (
    <div className="min-w-0">
      <div className="truncate font-mono text-xs text-foreground/50">
        {identifier}
      </div>
      <div className="truncate text-sm font-medium text-foreground">{title}</div>
    </div>
  )
}

/** The header band's own paint over the label recipe. */
export const MENU_HEADER_CLASS = `rounded-lg bg-accent/40 py-2`

export function MenuSpecimen({
  rows,
  density,
  className,
}: {
  rows: readonly MenuSpecimenRow[]
  density?: MenuDensity
  className?: string
}): ReactNode {
  return (
    <div
      data-slot="menu-specimen"
      style={density ? menuDensityStyle(density) : undefined}
      className={cn(
        MENU_SURFACE_CLASS,
        MENU_CONTENT_CLASS,
        `w-(--menu-max-width)`,
        className
      )}
    >
      {rows.map((row, index) => {
        switch (row.kind) {
          case `header`:
            return (
              <div key={index} className={cn(MENU_LABEL_CLASS, MENU_HEADER_CLASS)}>
                <MenuHeaderBody identifier={row.identifier} title={row.title} />
              </div>
            )
          case `separator`:
            return <div key={index} className={MENU_SEPARATOR_CLASS} />
          case `item`: {
            const Icon = row.icon
            return (
              <div
                key={index}
                className={MENU_ITEM_CLASS}
                data-variant={row.destructive ? `destructive` : `default`}
              >
                {Icon && <Icon />}
                {row.label}
                {row.shortcut !== undefined && (
                  <span className={MENU_SHORTCUT_CLASS}>{row.shortcut}</span>
                )}
              </div>
            )
          }
          case `submenu`: {
            const Icon = row.icon
            return (
              <div
                key={index}
                className={MENU_SUB_TRIGGER_CLASS}
                data-variant={row.destructive ? `destructive` : `default`}
              >
                {Icon && <Icon />}
                {row.label}
                {row.value !== undefined && (
                  <span className={cn(MENU_SHORTCUT_CLASS, MENU_VALUE_CLASS)}>
                    {row.value}
                  </span>
                )}
                <ChevronRightIcon className={MENU_CHEVRON_CLASS} />
              </div>
            )
          }
        }
      })}
    </div>
  )
}
