import { useState, type ReactNode } from "react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet"
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
          className="flex flex-col gap-0 p-0 pb-[env(safe-area-inset-bottom)]"
        >
          <SheetHeader className="pb-2">
            <SheetTitle className={mobileTitle ? undefined : `sr-only`}>
              {mobileTitle ?? `Options`}
            </SheetTitle>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto py-1">
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
                      <Icon
                    className={`h-4 w-4 shrink-0 ${option.color}`}
                    style={
                      option.colorHex ? { color: option.colorHex } : undefined
                    }
                  />
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
              <Icon
                className={`mr-2 h-4 w-4 ${option.color}`}
                style={option.colorHex ? { color: option.colorHex } : undefined}
              />
              {option.label}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
