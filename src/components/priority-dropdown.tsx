import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Button } from "@/components/ui/button"
import {
  Minus,
  AlertTriangle,
  SignalHigh,
  SignalMedium,
  SignalLow,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"

export const priorities = [
  { value: `none`, label: `No priority`, icon: Minus, color: `text-muted-foreground` },
  { value: `urgent`, label: `Urgent`, icon: AlertTriangle, color: `text-red-500` },
  { value: `high`, label: `High`, icon: SignalHigh, color: `text-orange-500` },
  { value: `medium`, label: `Medium`, icon: SignalMedium, color: `text-yellow-500` },
  { value: `low`, label: `Low`, icon: SignalLow, color: `text-blue-500` },
] as const

export type IssuePriority = (typeof priorities)[number][`value`]

export function getPriorityConfig(priority: string) {
  return priorities.find((p) => p.value === priority) ?? priorities[0]
}

export function PriorityIcon({ priority, className }: { priority: string; className?: string }) {
  const config = getPriorityConfig(priority)
  const Icon = config.icon
  return <Icon className={`h-4 w-4 ${config.color} ${className ?? ``}`} />
}

export function PriorityDropdown({
  issueId,
  priority,
}: {
  issueId: string
  priority: string
}) {
  const config = getPriorityConfig(priority)
  const Icon = config.icon

  const handleSelect = async (newPriority: string) => {
    await trpc.issues.update.mutate({ id: issueId, priority: newPriority as IssuePriority })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" className="h-5 w-5 p-0">
          <Icon className={`h-3.5 w-3.5 ${config.color}`} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {priorities.map((p) => {
          const PIcon = p.icon
          return (
            <DropdownMenuItem
              key={p.value}
              onClick={() => handleSelect(p.value)}
            >
              <PIcon className={`mr-2 h-4 w-4 ${p.color}`} />
              {p.label}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
