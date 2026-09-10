import { useState, type ReactNode } from "react"
import type { TeamAction } from "@/components/action-editor-dialog"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import { conceptIcon } from "@/lib/icons.generated"
import { getActionIcon } from "@/lib/board-icons"
import { cn } from "@/lib/utils"

// EXP-825: the composer's action picker — the launch dialog's Actions tab
// (EXP-257/EXP-768) as a popover off the card's ▶ tool. Single-select: a row
// becomes the subject (and the popover closes). Both listed builtins ride
// the list now — "Fix merge conflicts" pinned first, then "Create action"
// (its dedicated dialog is gone: the request is the composer text) — and the
// hidden Chat builtin never does (no subject IS the chat).

// EXP-721: the mobile row idiom — a LEADING selection glyph, then the
// action's own icon, then name over an optional one-line description.
const SelectedIcon = conceptIcon(`ui-selected`)
const UnselectedIcon = conceptIcon(`ui-unselected`)

export function ActionPickerList({
  actions,
  selectedActionId,
  onSelect,
}: {
  /** Builtin-first sorted list; null while the shape is loading. */
  actions: TeamAction[] | null
  selectedActionId: string | null
  onSelect: (actionId: string) => void
}) {
  return (
    <Command>
      <CommandInput placeholder="Search actions" />
      <CommandList data-testid="agent-composer-actions-picker">
        {actions === null ? (
          <div className="px-3 py-3 text-sm text-foreground/70">Loading…</div>
        ) : (
          <>
            <CommandEmpty>No actions match.</CommandEmpty>
            <CommandGroup>
              {actions.map((action) => {
                const selected = action.id === selectedActionId
                const RowIcon = getActionIcon(action)
                return (
                  <CommandItem
                    key={action.id}
                    value={action.id}
                    keywords={[action.name, action.description ?? ``]}
                    onSelect={() => onSelect(action.id)}
                    aria-pressed={selected}
                    className={cn(
                      `flex items-center gap-2.5`,
                      selected && `bg-glass-active`
                    )}
                  >
                    {selected ? (
                      <SelectedIcon className="size-4 shrink-0 text-foreground" />
                    ) : (
                      <UnselectedIcon className="size-4 shrink-0 text-muted-foreground" />
                    )}
                    <RowIcon className="size-4 shrink-0 text-muted-foreground" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-sm">{action.name}</span>
                      {action.description && (
                        <span className="truncate text-xs text-muted-foreground">
                          {action.description}
                        </span>
                      )}
                    </span>
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </>
        )}
      </CommandList>
    </Command>
  )
}

/** The ▶ tool's popover (a bottom sheet on phones); closes on a pick. */
export function ActionPicker({
  actions,
  selectedActionId,
  onSelect,
  disabled,
  children,
}: {
  actions: TeamAction[] | null
  selectedActionId: string | null
  onSelect: (actionId: string) => void
  disabled?: boolean
  /** The trigger (a `ComposerTool`). */
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      <MobilePopoverTrigger asChild disabled={disabled}>
        {children}
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[20rem] p-0"
        align="start"
        mobileTitle="Actions"
      >
        <ActionPickerList
          actions={actions}
          selectedActionId={selectedActionId}
          onSelect={(actionId) => {
            setOpen(false)
            onSelect(actionId)
          }}
        />
      </MobilePopoverContent>
    </MobilePopover>
  )
}
