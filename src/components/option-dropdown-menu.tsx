import type { ReactNode } from "react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { IssueOption } from "@/lib/domain"

interface OptionDropdownMenuProps<TValue extends string> {
  align?: React.ComponentProps<typeof DropdownMenuContent>[`align`]
  disabled?: boolean
  onSelect: (value: TValue) => void | Promise<void>
  options: readonly IssueOption<TValue>[]
  renderTrigger: (selected: IssueOption<TValue>) => ReactNode
  value: TValue
}

export function OptionDropdownMenu<TValue extends string>({
  align = `start`,
  disabled,
  onSelect,
  options,
  renderTrigger,
  value,
}: OptionDropdownMenuProps<TValue>) {
  const selected = options.find((option) => option.value === value) ?? options[0]

  if (disabled) {
    return <>{renderTrigger(selected)}</>
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{renderTrigger(selected)}</DropdownMenuTrigger>
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
