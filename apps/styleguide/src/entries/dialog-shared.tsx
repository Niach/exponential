import type { ComponentProps, ReactNode } from "react"
import prompts from "@exp/domain-contract/fixtures/prompts.json"
import {
  cn,
  conceptIcon,
  Dialog,
  PromptLayout,
  type PromptAction,
  type PromptActionRole,
} from "@exp/ui"

/**
 * What the dialog entries share (`blocked-start-dialog`,
 * `stack-merge-choice-dialog`, `draft-leave-dialog`).
 *
 * Not an entry itself (nothing in `sections.json` names it): just the frame.
 * `DialogContent` is a Radix portal, and a portal
 * renders nothing to static markup, so an entry draws the dialog's REAL parts
 * (`DialogHeader`, `DialogTitle`, `DialogFooter`, `Button`, ...) inside this
 * panel instead. The classes are the centred `sm` panel of
 * `packages/ui/src/dialog.tsx` (its `max-w-lg` width, its padding, its ✕),
 * minus its positioning and its animation. The page's demo canvas is
 * phone-width by default, so `styles.ts` widens it for each dialog entry to
 * the panel's real width.
 */

const CloseIcon = conceptIcon(`ui-close`)

export function DialogSpecimen({
  caption,
  showClose = false,
  className,
  children,
}: {
  /** One line above the panel naming the state it shows. */
  caption?: string
  /** `DialogContent` draws a ✕ top right; an alert dialog has none. */
  showClose?: boolean
  className?: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-2">
      {caption !== undefined && (
        <p className="text-xs text-muted-foreground">{caption}</p>
      )}
      <div
        className={cn(
          `relative flex w-full max-w-lg flex-col gap-4 overflow-hidden rounded-2xl border border-glass-stroke-card bg-card/85 p-6 shadow-2xl shadow-black/40 backdrop-blur-2xl`,
          className
        )}
      >
        {children}
        {showClose && (
          <span className="absolute top-4 right-4 opacity-70">
            <CloseIcon className="size-4" />
          </span>
        )}
      </div>
    </div>
  )
}

/**
 * EXP-1215: one `Prompt` at rest. The app's `Prompt` portals its card, so a
 * specimen draws the SAME inside (`PromptLayout`: the question, the body, the
 * content slot, the Pill row) in this frame with the prompt card's own
 * padding and width (`PROMPT_CARD_CLASS`: 20px, `max-w-md`), under a Radix
 * root that gives the title and the body their context.
 */
export function PromptSpecimen({
  caption,
  className,
  ...layout
}: {
  caption?: string
  className?: string
} & ComponentProps<typeof PromptLayout>) {
  return (
    <Dialog open>
      <DialogSpecimen caption={caption} className={cn(`max-w-md p-5`, className)}>
        <PromptLayout {...layout} />
      </DialogSpecimen>
    </Dialog>
  )
}

type PromptEntry = {
  title?: string
  titleOne?: string
  body?: string
  bodyOne?: string
  actions: { id: string; label: string; role: string }[]
}

/**
 * EXP-1215: a contract prompt's text for a specimen, straight off
 * `prompts.json` (the app reads it through `apps/web/src/lib/prompts.ts`):
 * the plain `title`/`body` (or their `One` variants) with `{params}` filled,
 * and the actions in display order. `busy`/`disabled` name an action id.
 */
export function promptSpecimenCopy(
  id: keyof typeof prompts.prompts,
  params: Record<string, string | number> = {},
  { busy, disabled }: { busy?: string; disabled?: string } = {}
): { title: string; body?: string; actions: PromptAction[] } {
  const entry = prompts.prompts[id] as PromptEntry
  const fill = (text: string | undefined) =>
    text?.replace(/\{(\w+)\}/g, (match, name: string) =>
      name in params ? String(params[name]) : match
    )
  const body = fill(entry.body ?? entry.bodyOne)
  return {
    title: fill(entry.title ?? entry.titleOne) ?? ``,
    ...(body ? { body } : {}),
    actions: entry.actions.map((action) => ({
      label: action.label,
      role: action.role as PromptActionRole,
      busy: action.id === busy,
      disabled: action.id === disabled,
    })),
  }
}
