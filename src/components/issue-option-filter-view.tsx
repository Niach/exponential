import { ArrowLeft } from "lucide-react"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Command,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import type { IssueOption } from "@/lib/domain"

interface IssueOptionFilterViewProps<TValue extends string> {
  onBack: () => void
  onToggle: (value: TValue) => void
  options: readonly IssueOption<TValue>[]
  selected: TValue[]
  title: string
}

export function IssueOptionFilterView<TValue extends string>({
  onBack,
  onToggle,
  options,
  selected,
  title,
}: IssueOptionFilterViewProps<TValue>) {
  return (
    <Command>
      <CommandList>
        <CommandGroup>
          <CommandItem onSelect={onBack} className="flex items-center gap-2">
            <ArrowLeft className="size-3.5" />
            <span className="font-medium">{title}</span>
          </CommandItem>
          {options.map((option) => {
            const Icon = option.icon

            return (
              <CommandItem
                key={option.value}
                onSelect={() => onToggle(option.value)}
                className="flex items-center gap-2"
              >
                <Checkbox
                  checked={selected.includes(option.value)}
                  className="pointer-events-none"
                />
                <Icon className={`!h-3.5 !w-3.5 ${option.color}`} />
                <span className="text-sm">{option.label}</span>
              </CommandItem>
            )
          })}
        </CommandGroup>
      </CommandList>
    </Command>
  )
}
