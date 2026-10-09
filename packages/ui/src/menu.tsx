import * as React from "react"
import type { CSSProperties, ReactElement, ReactNode, RefObject } from "react"

import { cn } from "./cn"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "./dropdown-menu"
import { conceptIcon } from "./icons.generated"
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
import type { PickerGlyph } from "./picker/picker-item"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "./sheet"
import { Switch } from "./switch"
import { useIsMobile } from "./use-mobile"

// UI cleanup batch — THE menu: ONE data-driven component for every action
// menu on the web (the issue context menu, the issue `…` menu, the action
// menus, the run switcher, the work-tab menu, the composer's "+").
//
// A menu is a list of `MenuEntry` rows — an item, a submenu (more entries, or
// a picker body: `PickerMenuRows`, a searchable `PickerList`), a toggle, a
// separator, a header band — handed to ONE renderer with three presentations:
//
//   trigger  a dropdown anchored at a trigger element (the `…` button)
//   pointer  a context menu at the pointer (`MenuGestureHost` below opens it
//            from a right-click or a touch long-press on any element that
//            opts in with `menuProps(kind, id)`)
//   sheet    a bottom sheet of the SAME rows at touch density, submenus as
//            pushed pages — chosen automatically below md unless the caller
//            forces `presentation`
//
// The rows wear the one recipe (`menu-surface.ts`: geometry from --menu-*,
// the pointer cursor, the destructive red with no divider above it), so a
// call site never builds a DropdownMenuItem by hand again. `MenuPanel` is the
// same renderer as a static surface (no portal, no behaviour) — what the
// styleguide draws, since a closed Radix portal renders nothing.

const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)
const ChevronLeftGlyph = conceptIcon(`ui-chevron-left`)
const CheckGlyph = conceptIcon(`ui-check`)

/** A row's leading glyph: a concept icon / brand mark component, or a ready
 *  node (a status glyph tinted for THIS issue, an avatar). */
export type MenuIcon = PickerGlyph | ReactElement

interface MenuEntryBase {
  /** A stable key; defaults to the row's position. */
  id?: string
  "data-testid"?: string
  className?: string
}

export interface MenuItemEntry extends MenuEntryBase {
  kind: `item`
  label: ReactNode
  icon?: MenuIcon
  /** A keyboard hint at the trailing edge ("⌘C"). */
  shortcut?: string
  /** A muted value at the trailing edge ("High"). */
  value?: ReactNode
  destructive?: boolean
  disabled?: boolean
  /** The menu stays open after the pick (a batch of toggles). */
  keepOpen?: boolean
  /** Hidden from md up — a phone-only verb ("Select"). */
  phoneOnly?: boolean
  /** A choice row (the current agent, the run on screen): a trailing check
   *  while true, `menuitemradio` either way. */
  checked?: boolean
  onSelect: () => void
}

export interface MenuSubmenuEntry extends MenuEntryBase {
  kind: `submenu`
  label: ReactNode
  icon?: MenuIcon
  /** The current value beside the label ("Backlog"); truncates, never the
   *  label. */
  value?: ReactNode
  destructive?: boolean
  disabled?: boolean
  /** The submenu's rows … */
  entries?: readonly MenuEntry[]
  /** … or a body: picker rows (`PickerMenuRows`), a searchable `PickerList`. */
  body?: ReactNode
  /** The submenu surface's width class (pointer only). */
  contentClassName?: string
}

export interface MenuToggleEntry extends MenuEntryBase {
  kind: `toggle`
  label: ReactNode
  icon?: MenuIcon
  checked: boolean
  disabled?: boolean
  onChange: (checked: boolean) => void
}

export interface MenuSeparatorEntry extends MenuEntryBase {
  kind: `separator`
}

export interface MenuHeaderEntry extends MenuEntryBase {
  kind: `header`
  /** A mono identifier over the title (the issue menu's band). */
  identifier?: string
  title: ReactNode
}

export type MenuEntry =
  | MenuItemEntry
  | MenuSubmenuEntry
  | MenuToggleEntry
  | MenuSeparatorEntry
  | MenuHeaderEntry

/** Drops separators that would lead, trail or double up — so a layout can
 *  list them unconditionally and let conditional rows come and go. */
export function tidyMenuEntries(entries: readonly MenuEntry[]): MenuEntry[] {
  const out: MenuEntry[] = []
  for (const entry of entries) {
    if (entry.kind === `separator`) {
      const last = out[out.length - 1]
      if (!last || last.kind === `separator` || last.kind === `header`) continue
    }
    out.push(entry)
  }
  while (out[out.length - 1]?.kind === `separator`) out.pop()
  return out
}

// ── density ─────────────────────────────────────────────────────────────────

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

/** Pins a menu to ONE density regardless of the viewport — the live
 *  `--menu-*` set follows the breakpoint, so this is how the styleguide sets
 *  the two side by side. */
export function menuDensityStyle(density: MenuDensity): CSSProperties {
  return Object.fromEntries(
    DENSITY_VARS.map((name) => [`--menu-${name}`, `var(--menu-${density}-${name})`])
  ) as CSSProperties
}

/** The identifier + title band a menu may open with. */
export function MenuHeaderBody({
  identifier,
  title,
}: {
  identifier?: string
  title: ReactNode
}): ReactNode {
  return (
    <div className="min-w-0">
      {identifier !== undefined && (
        <div className="truncate font-mono text-xs text-foreground/50">
          {identifier}
        </div>
      )}
      <div className="truncate text-sm font-medium text-foreground">{title}</div>
    </div>
  )
}

/** The header band's own paint over the label recipe. */
export const MENU_HEADER_CLASS = `rounded-lg bg-accent/40 py-2`

// ── the row shells ──────────────────────────────────────────────────────────

/** `dropdown` = Radix items (trigger + pointer), `sheet` = plain rows in a
 *  bottom sheet, `static` = plain rows at rest (`MenuPanel`). */
type MenuShell = `dropdown` | `sheet` | `static`

interface MenuShellValue {
  shell: MenuShell
  /** Closes the whole menu (a sheet row's pick). */
  close: () => void
  /** Pushes a submenu page (sheet only). */
  push: (index: number) => void
}

const MenuShellContext = React.createContext<MenuShellValue>({
  shell: `static`,
  close: () => {},
  push: () => {},
})

export function renderMenuIcon(icon: MenuIcon | undefined): ReactNode {
  if (icon === undefined) return null
  if (React.isValidElement(icon)) return icon
  const Glyph = icon as PickerGlyph
  return <Glyph aria-hidden />
}

export interface MenuRowProps {
  children: ReactNode
  onSelect?: () => void
  keepOpen?: boolean
  disabled?: boolean
  destructive?: boolean
  role?: `menuitem` | `menuitemradio` | `menuitemcheckbox`
  "aria-checked"?: boolean | `mixed`
  className?: string
  "data-testid"?: string
  /** Extra data attributes (a picker row's `data-value`, `data-picked`). */
  data?: Record<`data-${string}`, string | undefined>
}

/**
 * ONE menu row on whatever shell the enclosing `Menu` is: a Radix item in the
 * dropdown, a focusable row in the sheet (a pick closes it unless
 * `keepOpen`), a plain row at rest. Exported for the menu-arm bodies
 * (`PickerMenuRows`) so their rows are the menu's own.
 */
export function MenuRow({
  children,
  onSelect,
  keepOpen = false,
  disabled = false,
  destructive = false,
  role = `menuitem`,
  className,
  data,
  ...rest
}: MenuRowProps) {
  const { shell, close } = React.useContext(MenuShellContext)
  const variant = destructive ? `destructive` : `default`
  if (shell === `dropdown`) {
    return (
      <DropdownMenuItem
        role={role}
        aria-checked={rest[`aria-checked`]}
        data-testid={rest[`data-testid`]}
        variant={variant}
        disabled={disabled}
        className={className}
        onSelect={(event) => {
          if (keepOpen) event.preventDefault()
          onSelect?.()
        }}
        {...data}
      >
        {children}
      </DropdownMenuItem>
    )
  }
  const activate = () => {
    if (disabled || shell === `static`) return
    onSelect?.()
    if (!keepOpen) close()
  }
  return (
    <div
      role={role}
      aria-checked={rest[`aria-checked`]}
      aria-disabled={disabled || undefined}
      data-disabled={disabled ? `` : undefined}
      data-variant={variant}
      data-testid={rest[`data-testid`]}
      tabIndex={shell === `sheet` && !disabled ? 0 : undefined}
      className={cn(
        MENU_ITEM_CLASS,
        shell === `sheet` && `hover:bg-glass-active focus-visible:bg-glass-active`,
        className
      )}
      onClick={activate}
      onKeyDown={(event) => {
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          activate()
        }
      }}
      {...data}
    >
      {children}
    </div>
  )
}

function MenuTrailing({
  value,
  shortcut,
}: {
  value?: ReactNode
  shortcut?: string
}) {
  return (
    <>
      {value !== undefined && value !== null && (
        <span className={cn(MENU_SHORTCUT_CLASS, MENU_VALUE_CLASS)}>{value}</span>
      )}
      {shortcut !== undefined && (
        <span className={MENU_SHORTCUT_CLASS}>{shortcut}</span>
      )}
    </>
  )
}

/** Keeps a submenu body's keys its own: Radix's menu typeahead would move
 *  focus off a search field on every character, and ←/→ would close the
 *  submenu under the caret. Escape still closes. */
function SubmenuBody({ children }: { children: ReactNode }) {
  return (
    <div
      data-slot="menu-submenu-body"
      onKeyDown={(event) => {
        if (event.key !== `Escape`) event.stopPropagation()
      }}
    >
      {children}
    </div>
  )
}

function DropdownEntries({ entries }: { entries: readonly MenuEntry[] }) {
  return (
    <>
      {tidyMenuEntries(entries).map((entry, index) => {
        const key = entry.id ?? `${entry.kind}-${index}`
        switch (entry.kind) {
          case `separator`:
            return <DropdownMenuSeparator key={key} />
          case `header`:
            return (
              <DropdownMenuLabel
                key={key}
                className={cn(MENU_HEADER_CLASS, entry.className)}
                data-testid={entry[`data-testid`]}
              >
                <MenuHeaderBody identifier={entry.identifier} title={entry.title} />
              </DropdownMenuLabel>
            )
          case `item`:
            return (
              <MenuRow
                key={key}
                onSelect={entry.onSelect}
                role={entry.checked === undefined ? `menuitem` : `menuitemradio`}
                aria-checked={entry.checked}
                keepOpen={entry.keepOpen}
                disabled={entry.disabled}
                destructive={entry.destructive}
                className={cn(entry.phoneOnly && `md:hidden`, entry.className)}
                data-testid={entry[`data-testid`]}
              >
                <ItemBody entry={entry} />
              </MenuRow>
            )
          case `toggle`:
            return (
              <MenuRow
                key={key}
                role="menuitemcheckbox"
                aria-checked={entry.checked}
                keepOpen
                disabled={entry.disabled}
                className={entry.className}
                data-testid={entry[`data-testid`]}
                onSelect={() => entry.onChange(!entry.checked)}
              >
                <ToggleBody entry={entry} />
              </MenuRow>
            )
          case `submenu`:
            return (
              <DropdownMenuSub key={key}>
                <DropdownMenuSubTrigger
                  variant={entry.destructive ? `destructive` : `default`}
                  disabled={entry.disabled}
                  className={entry.className}
                  data-testid={entry[`data-testid`]}
                >
                  {renderMenuIcon(entry.icon)}
                  {entry.label}
                  {entry.value !== undefined && entry.value !== null && (
                    <span className={cn(MENU_SHORTCUT_CLASS, MENU_VALUE_CLASS)}>
                      {entry.value}
                    </span>
                  )}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent
                  className={entry.contentClassName ?? `w-[14rem]`}
                >
                  {entry.body !== undefined ? (
                    <SubmenuBody>{entry.body}</SubmenuBody>
                  ) : (
                    <DropdownEntries entries={entry.entries ?? []} />
                  )}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            )
        }
      })}
    </>
  )
}

function ItemBody({ entry }: { entry: MenuItemEntry }) {
  return (
    <>
      {renderMenuIcon(entry.icon)}
      <span className="min-w-0 truncate">{entry.label}</span>
      <MenuTrailing value={entry.value} shortcut={entry.shortcut} />
      {entry.checked && (
        <CheckGlyph
          aria-hidden
          data-selected-glyph="check"
          className="ml-auto size-3.5 shrink-0 text-muted-foreground"
        />
      )}
    </>
  )
}

function ToggleBody({ entry }: { entry: MenuToggleEntry }) {
  return (
    <>
      {renderMenuIcon(entry.icon)}
      <span className="min-w-0 truncate">{entry.label}</span>
      {/* A picture of the state, not a second control: the ROW toggles. */}
      <Switch
        size="sm"
        checked={entry.checked}
        tabIndex={-1}
        aria-hidden
        className="pointer-events-none ml-auto"
      />
    </>
  )
}

function PlainEntries({ entries }: { entries: readonly MenuEntry[] }) {
  const { shell, push } = React.useContext(MenuShellContext)
  const tidy = tidyMenuEntries(entries)
  return (
    <>
      {tidy.map((entry, index) => {
        const key = entry.id ?? `${entry.kind}-${index}`
        switch (entry.kind) {
          case `separator`:
            return <div key={key} role="separator" className={MENU_SEPARATOR_CLASS} />
          case `header`:
            return (
              <div
                key={key}
                className={cn(MENU_LABEL_CLASS, MENU_HEADER_CLASS, entry.className)}
                data-testid={entry[`data-testid`]}
              >
                <MenuHeaderBody identifier={entry.identifier} title={entry.title} />
              </div>
            )
          case `item`:
            return (
              <MenuRow
                key={key}
                onSelect={entry.onSelect}
                role={entry.checked === undefined ? `menuitem` : `menuitemradio`}
                aria-checked={entry.checked}
                keepOpen={entry.keepOpen}
                disabled={entry.disabled}
                destructive={entry.destructive}
                className={entry.className}
                data-testid={entry[`data-testid`]}
              >
                <ItemBody entry={entry} />
              </MenuRow>
            )
          case `toggle`:
            return (
              <MenuRow
                key={key}
                role="menuitemcheckbox"
                aria-checked={entry.checked}
                keepOpen
                disabled={entry.disabled}
                className={entry.className}
                data-testid={entry[`data-testid`]}
                onSelect={() => entry.onChange(!entry.checked)}
              >
                <ToggleBody entry={entry} />
              </MenuRow>
            )
          case `submenu`: {
            // A sheet pushes the submenu as a page; a static panel just
            // draws the trigger row.
            const realIndex = entries.indexOf(entry)
            return (
              <MenuRow
                key={key}
                keepOpen
                disabled={entry.disabled}
                destructive={entry.destructive}
                className={cn(MENU_SUB_TRIGGER_CLASS, entry.className)}
                data-testid={entry[`data-testid`]}
                data={{ "data-submenu": `true` }}
                onSelect={() => {
                  if (shell === `sheet`) push(realIndex)
                }}
              >
                {renderMenuIcon(entry.icon)}
                {entry.label}
                {entry.value !== undefined && entry.value !== null && (
                  <span className={cn(MENU_SHORTCUT_CLASS, MENU_VALUE_CLASS)}>
                    {entry.value}
                  </span>
                )}
                <ChevronRightGlyph className={MENU_CHEVRON_CLASS} />
              </MenuRow>
            )
          }
        }
      })}
    </>
  )
}

/** The submenu a page path names, re-resolved from the CURRENT entries on
 *  every render, so a live value under an open page stays current. */
function resolvePage(
  entries: readonly MenuEntry[],
  path: readonly number[]
): MenuSubmenuEntry | null {
  let level: readonly MenuEntry[] = entries
  let page: MenuSubmenuEntry | null = null
  for (const index of path) {
    const entry = level[index]
    if (!entry || entry.kind !== `submenu`) return null
    page = entry
    level = entry.entries ?? []
  }
  return page
}

function SheetPages({ entries }: { entries: readonly MenuEntry[] }) {
  const parent = React.useContext(MenuShellContext)
  const [path, setPath] = React.useState<number[]>([])
  const page = path.length > 0 ? resolvePage(entries, path) : null
  const value = React.useMemo<MenuShellValue>(
    () => ({
      shell: `sheet`,
      close: parent.close,
      push: (index) => setPath((current) => [...current, index]),
    }),
    [parent.close]
  )
  return (
    <MenuShellContext.Provider value={value}>
      {page ? (
        <div className="flex flex-col">
          <MenuRow
            keepOpen
            className="text-muted-foreground"
            data-testid="menu-sheet-back"
            onSelect={() => setPath((current) => current.slice(0, -1))}
          >
            <ChevronLeftGlyph aria-hidden />
            <span className="min-w-0 truncate">{page.label}</span>
          </MenuRow>
          <div role="separator" className={MENU_SEPARATOR_CLASS} />
          {page.body !== undefined ? (
            page.body
          ) : (
            <PlainEntries entries={page.entries ?? []} />
          )}
        </div>
      ) : (
        <PlainEntries entries={entries} />
      )}
    </MenuShellContext.Provider>
  )
}

// ── Menu ────────────────────────────────────────────────────────────────────

/** Where a pointer menu hangs: 0×0 at the pointer, or the element's box for
 *  a keyboard-invoked menu. */
export interface MenuAnchor {
  x: number
  y: number
  width: number
  height: number
}

export type MenuMode = `trigger` | `pointer` | `sheet`

export interface MenuProps {
  /** The rows … */
  entries?: readonly MenuEntry[]
  /** … or ONE body instead of them: a menu that is picker rows and nothing
   *  else (the bulk bar's Status / Priority, `PickerMenuRows`). */
  body?: ReactNode
  /** `trigger` (default with a `trigger`), `pointer` (default with an
   *  `anchor`), or `sheet` (always the bottom sheet). */
  mode?: MenuMode
  /** The element the dropdown hangs off. ONE element, wrapped `asChild`. */
  trigger?: ReactElement
  /** The pointer menu's anchor. */
  anchor?: MenuAnchor
  /** `auto` (default) = the bottom sheet below md; `menu` keeps the
   *  dropdown on a phone too. */
  presentation?: `auto` | `menu`
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** The sheet's headline on a phone (sr-only when absent). */
  title?: string
  "aria-label"?: string
  "data-testid"?: string
  align?: `start` | `center` | `end`
  side?: `top` | `right` | `bottom` | `left`
  sideOffset?: number
  /** The dropdown surface's width class. */
  contentClassName?: string
  /** Focus returns here when the menu closes (pointer mode: the row the
   *  gesture landed on; never the invisible anchor). */
  returnFocus?: HTMLElement | null
  /** Pins the rows to one density (the styleguide). */
  density?: MenuDensity
}

/** A right-click opens on the mouse DOWN, beside the cursor; a Radix item
 *  selects on a pointer-UP it did not see the down of. A pointer-up this
 *  soon after opening is the opening gesture's own release, never a pick. */
const POINTER_UP_GRACE_MS = 300

export function Menu(props: MenuProps) {
  const {
    entries = [],
    body,
    trigger,
    anchor,
    presentation = `auto`,
    open: openProp,
    onOpenChange,
    title,
    align = `start`,
    side,
    sideOffset,
    contentClassName,
    returnFocus,
    density,
  } = props
  const mode: MenuMode = props.mode ?? (anchor ? `pointer` : `trigger`)
  const isMobile = useIsMobile()
  const asSheet = mode === `sheet` || (presentation === `auto` && isMobile)

  const [uncontrolledOpen, setUncontrolledOpen] = React.useState(false)
  const open = openProp ?? uncontrolledOpen
  const setOpen = React.useCallback(
    (next: boolean) => {
      if (openProp === undefined) setUncontrolledOpen(next)
      onOpenChange?.(next)
    },
    [openProp, onOpenChange]
  )
  const close = React.useCallback(() => setOpen(false), [setOpen])
  const openedAt = React.useRef(0)
  React.useEffect(() => {
    if (open) openedAt.current = performance.now()
  }, [open])

  const densityStyle = density ? menuDensityStyle(density) : undefined

  if (asSheet) {
    return (
      <Sheet open={open} onOpenChange={setOpen}>
        {trigger && <SheetTrigger asChild>{trigger}</SheetTrigger>}
        <SheetContent
          side="bottom"
          aria-label={props[`aria-label`]}
          data-testid={props[`data-testid`]}
          data-slot="menu-sheet"
          style={densityStyle}
          className="flex flex-col gap-0 p-0 pb-[env(safe-area-inset-bottom)]"
          onCloseAutoFocus={(event) => {
            if (returnFocus !== undefined) {
              event.preventDefault()
              returnFocus?.focus?.({ preventScroll: true })
            }
          }}
        >
          <SheetHeader className="pb-1">
            <SheetTitle className={title ? undefined : `sr-only`}>
              {title ?? props[`aria-label`] ?? `Menu`}
            </SheetTitle>
          </SheetHeader>
          <div
            role="menu"
            aria-label={props[`aria-label`]}
            className="min-h-0 flex-1 overflow-y-auto px-(--menu-surface-padding) pb-2"
          >
            <MenuShellContext.Provider
              value={{ shell: `sheet`, close, push: () => {} }}
            >
              {body ?? <SheetPages entries={entries} />}
            </MenuShellContext.Provider>
          </div>
        </SheetContent>
      </Sheet>
    )
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      {mode === `pointer` && anchor ? (
        <DropdownMenuTrigger asChild>
          {/* Nothing to see, nothing to focus: a point at the pointer (the
              element's box for a keyboard-invoked menu) — what Radix's own
              ContextMenu anchors to. */}
          <span
            aria-hidden
            tabIndex={-1}
            data-testid="menu-pointer-anchor"
            style={{
              position: `fixed`,
              left: anchor.x,
              top: anchor.y,
              width: anchor.width,
              height: anchor.height,
              pointerEvents: `none`,
            }}
          />
        </DropdownMenuTrigger>
      ) : trigger ? (
        <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
      ) : null}
      <DropdownMenuContent
        aria-label={props[`aria-label`]}
        data-testid={props[`data-testid`]}
        side={side ?? (mode === `pointer` && anchor && anchor.width === 0 ? `right` : `bottom`)}
        align={align}
        sideOffset={sideOffset ?? (mode === `pointer` ? 2 : 4)}
        collisionPadding={12}
        style={densityStyle}
        className={contentClassName}
        onCloseAutoFocus={(event) => {
          if (returnFocus !== undefined || mode === `pointer`) {
            event.preventDefault()
            returnFocus?.focus?.({ preventScroll: true })
          }
        }}
        onPointerUpCapture={(event) => {
          if (
            mode === `pointer` &&
            performance.now() - openedAt.current < POINTER_UP_GRACE_MS
          ) {
            event.stopPropagation()
          }
        }}
      >
        <MenuShellContext.Provider
          value={{ shell: `dropdown`, close, push: () => {} }}
        >
          {body ?? <DropdownEntries entries={entries} />}
        </MenuShellContext.Provider>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/**
 * The entries as Radix items, for a host that already owns a
 * `DropdownMenu` root and its content (the work face strip's run caret).
 * New menus render `Menu` itself.
 */
export function MenuContentEntries({ entries }: { entries: readonly MenuEntry[] }) {
  return (
    <MenuShellContext.Provider value={{ shell: `dropdown`, close: () => {}, push: () => {} }}>
      <DropdownEntries entries={entries} />
    </MenuShellContext.Provider>
  )
}

/**
 * The SAME rows as a static surface: no portal, no behaviour — what the
 * styleguide renders, since a closed Radix portal renders nothing to static
 * markup. `look` paints the dropdown surface or the bottom sheet's.
 */
export function MenuPanel({
  entries,
  look = `dropdown`,
  density,
  title,
  className,
}: {
  entries: readonly MenuEntry[]
  look?: `dropdown` | `sheet`
  density?: MenuDensity
  title?: string
  className?: string
}) {
  return (
    <div
      data-slot="menu-panel"
      data-look={look}
      role="menu"
      style={density ? menuDensityStyle(density) : undefined}
      className={cn(
        look === `dropdown`
          ? cn(MENU_SURFACE_CLASS, MENU_CONTENT_CLASS, `w-(--menu-max-width)`)
          : `flex w-full flex-col rounded-t-3xl border-t border-glass-stroke-card bg-glass-bottom px-(--menu-surface-padding) pt-2 pb-4`,
        className
      )}
    >
      {look === `sheet` && (
        <>
          <div aria-hidden className="mx-auto mb-2 h-1 w-9 rounded-full bg-foreground/20" />
          {title && (
            <div className="px-(--menu-item-padding-x) pb-1 text-base font-semibold text-foreground">
              {title}
            </div>
          )}
        </>
      )}
      <MenuShellContext.Provider value={{ shell: `static`, close: () => {}, push: () => {} }}>
        <PlainEntries entries={entries} />
      </MenuShellContext.Provider>
    </div>
  )
}

// ── the pointer gesture host ────────────────────────────────────────────────
//
// Any element opts into a context menu with `menuProps(kind, id)`; ONE host
// per layout (`MenuGestureHost`) listens on the document — a right-click (or
// the keyboard's context-menu key), a touch long-press (Radix's
// ContextMenuTrigger rules: 700 ms, cancelled by a slide of a few px) — and
// renders the menu its `menus[kind]` builds for that id. No wrapper, no
// handler props, nothing for a memoized row to compare: `grep data-menu-id`
// is the whole surface. Issues opt in today; actions, runs and work tabs opt
// in the same way.

export const MENU_KIND_ATTR = `data-menu`
export const MENU_ID_ATTR = `data-menu-id`
/** The `?from=` origin an "Open" verb carries (EXP-851). */
export const MENU_FROM_ATTR = `data-menu-from`

export interface MenuTargetProps {
  "data-menu": string
  "data-menu-id": string
  "data-menu-from"?: string
}

export function menuProps(kind: string, id: string, from?: string): MenuTargetProps {
  return from
    ? { [MENU_KIND_ATTR]: kind, [MENU_ID_ATTR]: id, [MENU_FROM_ATTR]: from }
    : { [MENU_KIND_ATTR]: kind, [MENU_ID_ATTR]: id }
}

export interface MenuHit {
  element: HTMLElement
  kind: string
  id: string
  from?: string
}

export interface MenuTarget {
  kind: string
  id: string
  from?: string
  /** The element the gesture landed on; focus returns here on close. */
  origin: HTMLElement | null
  anchor: MenuAnchor
}

function hitOf(element: HTMLElement): MenuHit {
  const from = element.getAttribute(MENU_FROM_ATTR)
  return {
    element,
    kind: element.getAttribute(MENU_KIND_ATTR) ?? ``,
    id: element.getAttribute(MENU_ID_ATTR) ?? ``,
    ...(from ? { from } : {}),
  }
}

/** The opted-in element the event landed in, innermost first (a chip inside
 *  a row wins over the row). */
export function findMenuElement(target: EventTarget | null): MenuHit | null {
  if (!(target instanceof Element)) return null
  const element = target.closest<HTMLElement>(`[${MENU_ID_ATTR}]`)
  return element ? hitOf(element) : null
}

function insideRect(rect: DOMRect, x: number, y: number): boolean {
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

/** A point clipped away by a scrolling ancestor is not on the element. */
function visibleAt(element: HTMLElement, x: number, y: number): boolean {
  for (let node = element.parentElement; node !== null; node = node.parentElement) {
    const style = getComputedStyle(node)
    if (style.overflowX === `visible` && style.overflowY === `visible`) continue
    if (!insideRect(node.getBoundingClientRect(), x, y)) return false
  }
  return true
}

/** The opted-in element under a viewport point, by geometry. While a modal
 *  menu is up the body has `pointer-events: none`, so the browser hit-tests
 *  every event to `<html>` — this is how a right-click on another row still
 *  finds that row. Innermost = the smallest matching box. */
export function menuElementAt(x: number, y: number): MenuHit | null {
  let best: { element: HTMLElement; area: number } | null = null
  for (const element of document.querySelectorAll<HTMLElement>(`[${MENU_ID_ATTR}]`)) {
    const rect = element.getBoundingClientRect()
    if (rect.width === 0 || rect.height === 0 || !insideRect(rect, x, y)) continue
    const area = rect.width * rect.height
    if ((best === null || area < best.area) && visibleAt(element, x, y)) {
      best = { element, area }
    }
  }
  return best ? hitOf(best.element) : null
}

const LONG_PRESS_MS = 700
const LONG_PRESS_SLOP_PX = 10
/** How long after a `contextmenu` a `click` on the same element is the
 *  gesture's tail rather than a new click. */
const CONTEXT_CLICK_TAIL_MS = 100

function isEditable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    target.closest(`input, textarea, [contenteditable=""], [contenteditable="true"]`) !== null
  )
}

function isTouchOrPen(event: PointerEvent): boolean {
  return event.pointerType === `touch` || event.pointerType === `pen`
}

/** `<html>`/`<body>` as the target means a modal layer owns the pointer. */
function isRootTarget(target: EventTarget | null): boolean {
  return target === document.documentElement || target === document.body
}

function targetOf(hit: MenuHit, point: { x: number; y: number }): MenuTarget {
  const rect = hit.element.getBoundingClientRect()
  const atPointer = insideRect(rect, point.x, point.y)
  return {
    kind: hit.kind,
    id: hit.id,
    ...(hit.from ? { from: hit.from } : {}),
    origin: hit.element,
    anchor: atPointer
      ? { x: point.x, y: point.y, width: 0, height: 0 }
      : { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
  }
}

/**
 * The document listeners. `accepts(kind)` gates which opted-in elements this
 * host answers for (an unknown kind keeps the browser's own menu).
 */
export function useMenuGestures(
  onOpen: (target: MenuTarget) => void,
  openRef: RefObject<boolean>,
  accepts: (kind: string) => boolean = () => true
): void {
  const onOpenRef = React.useRef(onOpen)
  onOpenRef.current = onOpen
  const acceptsRef = React.useRef(accepts)
  acceptsRef.current = accepts

  React.useEffect(() => {
    let timer: number | null = null
    let press: { x: number; y: number; hit: MenuHit } | null = null
    const clearPress = () => {
      if (timer !== null) {
        window.clearTimeout(timer)
        timer = null
      }
      press = null
    }
    const accepted = (hit: MenuHit | null) =>
      hit !== null && acceptsRef.current(hit.kind) ? hit : null

    // A macOS ctrl+click is a right-click, and Firefox follows the
    // `contextmenu` with a `click` on the same element — which would open the
    // row under the menu that just appeared. The tail is swallowed at the
    // document in the capture phase, for one click within the tail window.
    let tail: { onClick: (event: MouseEvent) => void; timer: number } | null = null
    const clearTail = () => {
      if (!tail) return
      document.removeEventListener(`click`, tail.onClick, { capture: true })
      window.clearTimeout(tail.timer)
      tail = null
    }
    const swallowClickTail = (element: HTMLElement) => {
      clearTail()
      const onClick = (click: MouseEvent) => {
        if (!(click.target instanceof Node) || !element.contains(click.target)) return
        click.stopPropagation()
        click.preventDefault()
        clearTail()
      }
      tail = { onClick, timer: window.setTimeout(clearTail, CONTEXT_CLICK_TAIL_MS) }
      document.addEventListener(`click`, onClick, { capture: true })
    }

    const openFromContextMenu = (hit: MenuHit, point: { x: number; y: number }) => {
      onOpenRef.current(targetOf(hit, point))
      swallowClickTail(hit.element)
    }

    const onContextMenu = (event: MouseEvent) => {
      clearPress()
      if (event.defaultPrevented) return
      const point = { x: event.clientX, y: event.clientY }
      const layered = openRef.current || isRootTarget(event.target)
      if (layered) {
        // Ours is up (or fading out): never the browser's menu on top of it,
        // and never the item under the release. A right-click on another row
        // moves the menu there.
        event.preventDefault()
        if (event.target instanceof Element && event.target.closest(`[role="menu"]`)) return
        const hit = accepted(findMenuElement(event.target) ?? menuElementAt(point.x, point.y))
        if (hit) openFromContextMenu(hit, point)
        return
      }
      if (isEditable(event.target)) return
      const hit = accepted(findMenuElement(event.target))
      if (!hit) return
      event.preventDefault()
      openFromContextMenu(hit, point)
    }

    const onPointerDown = (event: PointerEvent) => {
      if (!isTouchOrPen(event)) return
      const hit = accepted(findMenuElement(event.target))
      if (!hit) return
      clearPress()
      press = { x: event.clientX, y: event.clientY, hit }
      timer = window.setTimeout(() => {
        const current = press
        clearPress()
        if (current) onOpenRef.current(targetOf(current.hit, { x: current.x, y: current.y }))
      }, LONG_PRESS_MS)
    }

    const onPointerMove = (event: PointerEvent) => {
      if (!press || !isTouchOrPen(event)) return
      if (Math.hypot(event.clientX - press.x, event.clientY - press.y) > LONG_PRESS_SLOP_PX) {
        clearPress()
      }
    }

    const onPointerEnd = () => clearPress()

    document.addEventListener(`contextmenu`, onContextMenu)
    document.addEventListener(`pointerdown`, onPointerDown, { passive: true })
    document.addEventListener(`pointermove`, onPointerMove, { passive: true })
    document.addEventListener(`pointerup`, onPointerEnd, { passive: true })
    document.addEventListener(`pointercancel`, onPointerEnd, { passive: true })
    return () => {
      clearPress()
      clearTail()
      document.removeEventListener(`contextmenu`, onContextMenu)
      document.removeEventListener(`pointerdown`, onPointerDown)
      document.removeEventListener(`pointermove`, onPointerMove)
      document.removeEventListener(`pointerup`, onPointerEnd)
      document.removeEventListener(`pointercancel`, onPointerEnd)
    }
  }, [openRef])
}

/** What a kind's builder gets: the target, and the open state it owns. */
export interface MenuSessionProps {
  target: MenuTarget
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * ONE context-menu host per layout. `menus[kind]` builds the menu for a
 * target (typically a component rendering `<Menu mode="pointer"
 * anchor={target.anchor} … />` from live data). Every open is a FRESH
 * session (keyed by kind + id + a sequence number), so the previous menu's
 * exit animation never cross-fades into the next; the target is kept after
 * a close on purpose — a menu's deferred dialogs outlive the menu.
 */
export function MenuGestureHost({
  menus,
  children,
}: {
  menus: Record<string, (session: MenuSessionProps) => ReactNode>
  children?: ReactNode
}) {
  const [target, setTarget] = React.useState<(MenuTarget & { seq: number }) | null>(null)
  const [open, setOpen] = React.useState(false)
  const openRef = React.useRef(false)
  openRef.current = open
  const seqRef = React.useRef(0)
  const menusRef = React.useRef(menus)
  menusRef.current = menus

  useMenuGestures(
    React.useCallback((next: MenuTarget) => {
      seqRef.current += 1
      setTarget({ ...next, seq: seqRef.current })
      setOpen(true)
    }, []),
    openRef,
    React.useCallback((kind: string) => kind in menusRef.current, [])
  )

  const build = target ? menus[target.kind] : undefined
  return (
    <>
      {children}
      {target && build && (
        <React.Fragment key={`${target.kind}:${target.id}:${target.seq}`}>
          {build({ target, open, onOpenChange: setOpen })}
        </React.Fragment>
      )}
    </>
  )
}
