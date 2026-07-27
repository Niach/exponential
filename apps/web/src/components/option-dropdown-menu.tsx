import { useState, type ReactNode } from "react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet"
import { useIsMobile } from "@/hooks/use-mobile"
import { Check } from "lucide-react"
import type { IssueOption } from "@/lib/domain"

interface OptionDropdownMenuProps<TValue extends string> {
  align?: React.ComponentProps<typeof DropdownMenuContent>[`align`]
  disabled?: boolean
  // Which option the trigger shows when `value` is not in `options` (an
  // unknown/forward-compat wire value, or a value the caller filtered out of
  // the menu). Callers pass the lifecycle-start fallback of their vocabulary
  // (`ISSUE_STATUS_FALLBACK` / `ISSUE_PRIORITY_FALLBACK`) — the option tables
  // are DISPLAY-ordered (REV2-85), so falling back to `options[0]` would show
  // "In Progress" / "Urgent" for an unknown value.
  fallbackValue?: TValue
  onSelect: (value: TValue) => void | Promise<void>
  options: readonly IssueOption<TValue>[]
  renderTrigger: (selected: IssueOption<TValue>) => ReactNode
  value: TValue
  mobileTitle?: string
}

export function OptionDropdownMenu<TValue extends string>({
  align = `start`,
  disabled,
  fallbackValue,
  onSelect,
  options,
  renderTrigger,
  value,
  mobileTitle,
}: OptionDropdownMenuProps<TValue>) {
  const isMobile = useIsMobile()
  const [open, setOpen] = useState(false)
  const selected =
    options.find((option) => option.value === value) ??
    options.find((option) => option.value === fallbackValue) ??
    options[0]

  if (disabled) {
    return <>{renderTrigger(selected)}</>
  }

  if (isMobile) {
    return (
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetTrigger asChild>{renderTrigger(selected)}</SheetTrigger>
        <SheetContent
          side="bottom"
          showCloseButton={false}
          className="flex flex-col gap-0 max-h-[85dvh] p-0 pb-[env(safe-area-inset-bottom)] rounded-t-xl"
        >
          {mobileTitle && (
            <div className="px-4 pt-3 pb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {mobileTitle}
            </div>
          )}
          <div className="overflow-y-auto py-1">
            {options.map((option) => {
              const Icon = option.icon
              const isSelected = option.value === value
              return (
                <button
                  key={option.value}
                  type="button"
                  className="flex w-full items-center gap-3 px-4 py-3 text-left text-sm hover:bg-accent active:bg-accent/70"
                  onClick={() => {
                    void onSelect(option.value)
                    setOpen(false)
                  }}
                >
                  <Icon className={`h-4 w-4 shrink-0 ${option.color}`} />
                  <span className="flex-1 truncate">{option.label}</span>
                  {isSelected && (
                    <Check className="h-4 w-4 shrink-0 text-muted-foreground" />
                  )}
                </button>
              )
            })}
          </div>
        </SheetContent>
      </Sheet>
    )
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        {renderTrigger(selected)}
      </DropdownMenuTrigger>
      <DropdownMenuContent align={align}>
        {options.map((option) => {
          const Icon = option.icon

          return (
            <DropdownMenuItem
              key={option.value}
              onClick={() => {
                void onSelect(option.value)
              }}
            >
              <Icon className={`mr-2 h-4 w-4 ${option.color}`} />
              {option.label}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
