import * as React from "react"

import { cn } from "./cn"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./dialog"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"

const LoadingIcon = conceptIcon(`ui-loading`)

// EXP-1215 — THE confirm/choice prompt. Every "Delete X?", every merge
// confirm and every multi-answer choice on the web is a `Prompt`, the same
// card on every width (lifted from the EXP-1212 draft prompts): a centred card
// over the dimmed scrim, no ✕, ONE question as the title, an optional body
// line that states a fact the title cannot carry, an optional content slot
// (a graph, an input) and ONE row of the 32px `md` Pill capsules — the same
// capsules iOS and Android draw. The wording of every prompt the clients
// share lives in `@exp/domain-contract/fixtures/prompts.json` (web mirror:
// `apps/web/src/lib/prompts.ts`), whose `_comment` also fixes the rules below.
//
// Roles (the fixture's):
//   cancel           — the plain pill that only dismisses (= scrim, Esc);
//   primary          — the accent pill, the safe/expected answer, trailing;
//   default          — a plain pill that does something;
//   destructive      — a plain pill with a destructive label + tinted border:
//                      the answer of a plain "Delete X?" next to Cancel;
//   quietDestructive — destructive TEXT set apart on the leading edge, for a
//                      destructive answer beside a safe primary.
//
// Initial focus: a content-slot field marked `data-prompt-autofocus`, else
// the action marked `autoFocus`, else the primary, else the first plain
// one; never a destructive answer (so Enter never deletes).
// Scrim tap / Esc = the cancel path (`onDismiss`, then `onOpenChange(false)`).
// An `onSelect` that returns a promise keeps the card open with the row
// disabled until it settles; the handler closes the prompt itself (an action
// with no `onSelect` simply dismisses, i.e. Cancel). A busy answer (its
// promise in flight, or `busy` on the action) keeps its label and swaps its
// leading glyph for the spinner; the whole row and the dismissal lock.
//
// Layout: ONE row, a leading quiet destructive answer apart on the leading
// edge, the rest packed trailing in display order. When the row cannot fit
// it STACKS: one natural-width pill per line, trailing-aligned, in REVERSE
// display order (default/primary first, Cancel last, a quiet destructive
// answer last of all). Never a two-row hybrid, never full-width blocks.

export type PromptActionRole =
  | `cancel`
  | `primary`
  | `default`
  | `destructive`
  | `quietDestructive`

export interface PromptAction {
  label: React.ReactNode
  /** A glyph before the label (Merge stack's merge mark). */
  leading?: React.ReactNode
  /** Defaults to `default` (a plain pill). */
  role?: PromptActionRole
  disabled?: boolean
  /** This answer's work is in flight: the spinner replaces `leading`, the
   *  label stays (an outside mutation; an async `onSelect` sets it itself). */
  busy?: boolean
  /** May be async: the row stays disabled while it runs. Omitted = dismiss. */
  onSelect?: () => unknown
  /** Takes initial focus (never honoured on a destructive role). */
  autoFocus?: boolean
  testId?: string
}

export interface PromptProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: React.ReactNode
  /** A fact the title cannot carry; never a restatement of the buttons. */
  body?: React.ReactNode
  /** The content slot between the text and the row (a graph, an input). */
  children?: React.ReactNode
  /** In order, leading → trailing. */
  actions: readonly PromptAction[]
  /** Scrim tap, Esc and an action without `onSelect`. */
  onDismiss?: () => void
  /** Disables the row and the dismissal (an outside mutation in flight). */
  busy?: boolean
  /** Card overrides (`sm:max-w-*` for a wide content slot). */
  className?: string
  "data-testid"?: string
}

/** The card: 20px padding, `max-w-md` from `sm` up (the alert arm below). */
const PROMPT_CARD_CLASS = `p-5 sm:max-w-md sm:p-5`
// `flex-nowrap`: the row never wraps into a two-row hybrid; a row that
// cannot fit switches to the stack below as a whole (`useRowFits`).
const PROMPT_ROW = `flex-row flex-nowrap items-center justify-end`
// The fallback when the row cannot fit (iOS `GlassAlertLayout.stacked`): the
// pills keep their own width, right-aligned, the default answer (last in
// reading order) on TOP and a leading quiet one at the bottom.
const PROMPT_STACK = `flex-col-reverse items-end gap-2 sm:flex-col-reverse sm:justify-start`
const ROLE_CLASS: Record<PromptActionRole, string> = {
  cancel: ``,
  primary: ``,
  default: ``,
  destructive: `border-destructive/40 text-destructive hover:text-destructive`,
  quietDestructive: `border-transparent bg-transparent text-destructive hover:bg-transparent hover:text-destructive`,
}
// In the ROW only: the quiet answer sits apart on the leading edge, `-ml-3`
// (the pill's `px-3`) lining the word up with the question. The stack keeps
// it trailing-aligned like every other pill.
const QUIET_IN_ROW = `-ml-3 mr-auto`

const isDestructive = (role: PromptActionRole) =>
  role === `destructive` || role === `quietDestructive`

/** The index of the action that takes initial focus, or -1 (the card). */
export function promptFocusIndex(actions: readonly PromptAction[]): number {
  const usable = (action: PromptAction) =>
    !action.disabled && !isDestructive(action.role ?? `default`)
  const marked = actions.findIndex((a) => a.autoFocus && usable(a))
  if (marked >= 0) return marked
  const primary = actions.findIndex(
    (a) => (a.role ?? `default`) === `primary` && usable(a)
  )
  if (primary >= 0) return primary
  return actions.findIndex(usable)
}

function Prompt({
  open,
  onOpenChange,
  title,
  body,
  children,
  actions,
  onDismiss,
  busy = false,
  className,
  "data-testid": testId,
}: PromptProps) {
  // The index of the answer whose async `onSelect` is in flight, or null.
  const [running, setRunning] = React.useState<number | null>(null)
  const contentRef = React.useRef<HTMLDivElement | null>(null)
  const buttonRefs = React.useRef<(HTMLButtonElement | null)[]>([])
  const mounted = React.useRef(true)
  React.useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  // A prompt reopened after an async answer starts with a live row.
  React.useEffect(() => {
    if (!open) setRunning(null)
  }, [open])

  const locked =
    busy || running !== null || actions.some((action) => action.busy)

  const dismiss = () => {
    onDismiss?.()
    onOpenChange(false)
  }

  const select = (action: PromptAction, index: number) => {
    if (locked) return
    if (!action.onSelect) {
      dismiss()
      return
    }
    const result = action.onSelect()
    if (result && typeof (result as Promise<unknown>).then === `function`) {
      setRunning(index)
      void (result as Promise<unknown>)
        .catch(() => undefined)
        .finally(() => {
          if (mounted.current) setRunning(null)
        })
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) {
          onOpenChange(true)
        } else if (!locked) {
          dismiss()
        }
      }}
    >
      <DialogContent
        ref={contentRef}
        mobile="alert"
        role="alertdialog"
        showCloseButton={false}
        className={cn(PROMPT_CARD_CLASS, className)}
        data-testid={testId}
        data-prompt=""
        {...(body ? {} : { "aria-describedby": undefined })}
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          // A field in the content slot marked `data-prompt-autofocus` (a
          // type-the-name confirm) takes focus first.
          const field = contentRef.current?.querySelector<HTMLElement>(
            `[data-prompt-autofocus]`
          )
          const index = promptFocusIndex(actions)
          const target = field ?? (index >= 0 ? buttonRefs.current[index] : null)
          ;(target ?? contentRef.current)?.focus()
        }}
      >
        <PromptLayout
          title={title}
          body={body}
          actions={actions}
          disabled={locked}
          runningIndex={running}
          onSelect={select}
          buttonRef={(index, node) => {
            buttonRefs.current[index] = node
          }}
        >
          {children}
        </PromptLayout>
      </DialogContent>
    </Dialog>
  )
}

/**
 * Whether the row's pills fit side by side: their own widths plus the gaps
 * against the row's width, re-measured on resize. jsdom (no layout) and the
 * first paint count as fitting.
 */
function useRowFits(rowRef: React.RefObject<HTMLDivElement | null>): boolean {
  const [fits, setFits] = React.useState(true)
  React.useLayoutEffect(() => {
    const row = rowRef.current
    if (!row || typeof ResizeObserver === `undefined`) return
    const measure = () => {
      const pills = [...row.children] as HTMLElement[]
      const gap = parseFloat(getComputedStyle(row).columnGap) || 0
      const needed =
        pills.reduce((sum, pill) => sum + pill.offsetWidth, 0) +
        gap * Math.max(0, pills.length - 1)
      const width = row.clientWidth
      setFits(width === 0 || needed <= width)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(row)
    for (const pill of row.children) observer.observe(pill)
    return () => observer.disconnect()
  }, [rowRef])
  return fits
}

/**
 * The card's inside: the question, the body, the content slot and the row.
 * `Prompt` renders it in its portalled `DialogContent`; the styleguide draws
 * it statically inside a `Dialog` root (a portal renders nothing to static
 * markup), in a frame carrying `PROMPT_CARD_CLASS`.
 */
function PromptLayout({
  title,
  body,
  children,
  actions,
  disabled = false,
  runningIndex = null,
  onSelect,
  buttonRef,
  stacked,
}: {
  title: React.ReactNode
  body?: React.ReactNode
  children?: React.ReactNode
  actions: readonly PromptAction[]
  disabled?: boolean
  /** The answer whose async `onSelect` is in flight (its pill spins). */
  runningIndex?: number | null
  onSelect?: (action: PromptAction, index: number) => void
  buttonRef?: (index: number, node: HTMLButtonElement | null) => void
  /** Force the stacked fallback (the styleguide's static specimen). */
  stacked?: boolean
}) {
  const rowRef = React.useRef<HTMLDivElement | null>(null)
  const fits = useRowFits(rowRef)
  const isStacked = stacked ?? !fits
  return (
    <>
      <DialogHeader className="gap-2">
        <DialogTitle className="leading-snug">{title}</DialogTitle>
        {body ? (
          <DialogDescription asChild>
            <div>{body}</div>
          </DialogDescription>
        ) : null}
      </DialogHeader>
      {children ? <DialogBody>{children}</DialogBody> : null}
      <DialogFooter
        ref={rowRef}
        className={isStacked ? PROMPT_STACK : PROMPT_ROW}
        data-prompt-actions=""
        data-stacked={isStacked ? `` : undefined}
      >
        {actions.map((action, index) => {
          const role = action.role ?? `default`
          const spinning = action.busy || runningIndex === index
          return (
            <Pill
              key={index}
              ref={(node) => buttonRef?.(index, node)}
              size="md"
              mode="action"
              primary={role === `primary`}
              data-role={role}
              data-busy={spinning ? `` : undefined}
              aria-busy={spinning || undefined}
              data-testid={action.testId}
              className={cn(
                ROLE_CLASS[role],
                role === `quietDestructive` && !isStacked && QUIET_IN_ROW
              )}
              disabled={disabled || action.disabled || action.busy}
              leading={
                spinning ? (
                  <LoadingIcon className="animate-spin" data-prompt-spinner="" />
                ) : (
                  action.leading
                )
              }
              onClick={() => onSelect?.(action, index)}
            >
              {action.label}
            </Pill>
          )
        })}
      </DialogFooter>
    </>
  )
}

export { Prompt, PromptLayout, PROMPT_CARD_CLASS }
