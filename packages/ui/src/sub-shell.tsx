import type { LucideIcon } from "lucide-react"
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { createPortal } from "react-dom"

import { cn } from "./cn"
import { GlassRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// EXP-1029 contract, implemented by EXP-1020 — sub-shell navigation for the
// settings shell (EXP-994's `GlassGroup` rows).
//
// A `SubShell` is a ROW ENTRY inside a card. Opening it slides a child page
// in place of the WHOLE card — not a nested card, not a dialog — with a back
// button on top; the child page is the same shell (its own `GlassGroup`s of
// rows), so a sub-shell may hold another sub-shell. `SubShellHost` is the
// card boundary the page replaces: it renders its children at rest and, once
// a row inside opened, the row's page instead.
//
// How it works: every LEVEL (the host, and each open page) renders its own
// content, hidden while one of its rows is open, plus an empty SLOT after
// it. A row that opens portals its page into its level's slot, so the page
// sits BESIDE the card in the DOM while the card's rows stay MOUNTED — their
// props (a live picker value, a draft) keep flowing into the open page. A
// page is itself a level, which is what makes nesting fall out for free: a
// deeper page hides its parent's rows AND its parent's back header.
//
// Platform siblings (same names): IDE `ui::sub_shell`, iOS
// `ExpUI/Sources/SubShell.swift`, Android `ui/components/SubShell.kt`.

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

/** What a level hands the rows inside it: the slot a page portals into, and
 *  the one row id that is open. */
interface SubShellLevelValue extends SubShellContextValue {
  slot: HTMLElement | null
  openId: string | null
  setOpenId: (id: string | null) => void
}

const SubShellLevel = createContext<SubShellLevelValue | null>(null)

/** Reads the enclosing host, for a page that wants to close itself. */
export function useSubShell(): SubShellContextValue {
  const value = useContext(SubShellLevel)
  if (!value) throw new Error(`useSubShell needs a SubShellHost above it`)
  return value
}

const ChevronGlyph = conceptIcon(`ui-chevron-right`)
const BackGlyph = conceptIcon(`ui-chevron-left`)

/** A swipe rightwards returns one level (phone). */
const SWIPE_BACK_PX = 56

/**
 * One level of the shell: its content (the back header, when it is a page),
 * hidden while one of its rows is open, plus the slot that row's page
 * portals into. The host and every page are the same thing.
 */
function ShellLevel({
  children,
  className,
  slotName,
  page,
}: {
  children: ReactNode
  className?: string
  slotName: string
  /** Set when this level IS a page: its title and the way back. */
  page?: { title: string; onBack: () => void }
}) {
  const [openId, setOpenId] = useState<string | null>(null)
  const [adhoc, setAdhoc] = useState<SubShellPage | null>(null)
  const [slotNode, setSlotNode] = useState<HTMLDivElement | null>(null)
  const backRef = useRef<HTMLButtonElement | null>(null)
  const touchStart = useRef<{ x: number; y: number } | null>(null)

  const back = useCallback(() => {
    setOpenId(null)
    setAdhoc(null)
  }, [])
  const open = useCallback((next: SubShellPage) => {
    setOpenId(null)
    setAdhoc(next)
  }, [])
  const isOpen = openId !== null || adhoc !== null

  // A page owns the keyboard the moment it slides in: a screen reader lands
  // on the back button rather than deep in the rows, and Escape returns.
  const onBack = page?.onBack
  useEffect(() => {
    if (onBack) backRef.current?.focus()
  }, [onBack])

  const value: SubShellLevelValue = {
    open,
    back,
    isOpen,
    slot: slotNode,
    openId,
    setOpenId: (id) => {
      setAdhoc(null)
      setOpenId(id)
    },
  }

  return (
    <SubShellLevel.Provider value={value}>
      <div data-slot={slotName} className={cn(`flex min-w-0 flex-col`, className)}>
        {/* This level at rest. Kept MOUNTED while one of its rows is open
            (so the open page keeps seeing live props) but hidden from
            layout and from the a11y tree. */}
        <div
          hidden={isOpen || undefined}
          data-slot="sub-shell-body"
          className={cn(
            `flex min-w-0 flex-col gap-6`,
            isOpen && `hidden`,
            page &&
              `animate-in duration-fast ease-standard slide-in-from-right-4 fade-in motion-reduce:animate-none`
          )}
          onKeyDown={
            onBack
              ? (event) => {
                  if (event.key !== `Escape`) return
                  // The innermost open page wins: the event reaches it
                  // first on its way out through its ancestors' slots.
                  event.stopPropagation()
                  onBack()
                }
              : undefined
          }
          onTouchStart={
            onBack
              ? (event) => {
                  const touch = event.touches[0]
                  touchStart.current = touch ? { x: touch.clientX, y: touch.clientY } : null
                }
              : undefined
          }
          onTouchEnd={
            onBack
              ? (event) => {
                  const start = touchStart.current
                  const touch = event.changedTouches[0]
                  touchStart.current = null
                  if (!start || !touch) return
                  const dx = touch.clientX - start.x
                  const dy = Math.abs(touch.clientY - start.y)
                  if (dx > SWIPE_BACK_PX && dx > dy * 2) onBack()
                }
              : undefined
          }
        >
          {page ? (
            <div className="flex min-w-0 items-center gap-2">
              <button
                ref={backRef}
                type="button"
                data-slot="sub-shell-back"
                aria-label="Back"
                onClick={page.onBack}
                className="-ml-1 flex items-center rounded-md p-1 text-muted-foreground transition-colors outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50"
              >
                <BackGlyph className="size-4 shrink-0" />
              </button>
              <span className="min-w-0 truncate text-sm font-medium">{page.title}</span>
            </div>
          ) : null}
          {children}
        </div>
        <div ref={setSlotNode} className="contents" />
        {adhoc && slotNode
          ? createPortal(
              <ShellLevel
                slotName="sub-shell-page"
                page={{ title: adhoc.title, onBack: back }}
              >
                {adhoc.content}
              </ShellLevel>,
              slotNode
            )
          : null}
      </div>
    </SubShellLevel.Provider>
  )
}

/**
 * The card boundary a sub-shell page replaces: renders its card at rest and
 * the open row's page instead of it.
 */
export function SubShellHost({ children, className }: SubShellHostProps) {
  return (
    <ShellLevel slotName="sub-shell-host" className={className}>
      {children}
    </ShellLevel>
  )
}

/**
 * A row entry that slides its child page in place of the whole card.
 */
export function SubShell({
  label,
  description,
  icon: Icon,
  value,
  title,
  open: controlledOpen,
  onOpenChange,
  children,
  disabled = false,
  className,
  "data-testid": testId,
}: SubShellProps) {
  const id = useId()
  const level = useContext(SubShellLevel)
  if (!level) throw new Error(`SubShell needs a SubShellHost above it`)

  const isOpen = level.openId === id
  const { setOpenId } = level

  // Controlled: the prop is the truth and the level follows it.
  useEffect(() => {
    if (controlledOpen === undefined) return
    setOpenId(controlledOpen ? id : null)
    // `setOpenId` is rebuilt every render; only the flag drives this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [controlledOpen, id])

  const setOpen = (next: boolean) => {
    if (disabled) return
    onOpenChange?.(next)
    if (controlledOpen === undefined) setOpenId(next ? id : null)
  }

  const pageTitle = title ?? (typeof label === `string` ? label : ``)

  return (
    <>
      <GlassRow
        interactive={!disabled}
        data-slot="sub-shell"
        data-testid={testId}
        aria-disabled={disabled || undefined}
        aria-expanded={isOpen}
        onClick={() => setOpen(true)}
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
      {isOpen && level.slot
        ? createPortal(
            <ShellLevel
              slotName="sub-shell-page"
              page={{ title: pageTitle, onBack: () => setOpen(false) }}
            >
              {children}
            </ShellLevel>,
            level.slot
          )
        : null}
    </>
  )
}
