import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Button } from "@/components/ui/button"
import {
  CircleDashed,
  Circle,
  Timer,
  CircleCheck,
  CircleX,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"

export const statuses = [
  { value: `backlog`, label: `Backlog`, icon: CircleDashed, color: `text-muted-foreground` },
  { value: `todo`, label: `Todo`, icon: Circle, color: `text-foreground` },
  { value: `in_progress`, label: `In Progress`, icon: Timer, color: `text-yellow-500` },
  { value: `done`, label: `Done`, icon: CircleCheck, color: `text-green-500` },
  { value: `cancelled`, label: `Cancelled`, icon: CircleX, color: `text-muted-foreground` },
] as const

export type IssueStatus = (typeof statuses)[number][`value`]

export function getStatusConfig(status: string) {
  return statuses.find((s) => s.value === status) ?? statuses[0]
}

export function StatusIcon({ status, className }: { status: string; className?: string }) {
  const config = getStatusConfig(status)
  const Icon = config.icon
  return <Icon className={`h-4 w-4 ${config.color} ${className ?? ``}`} />
}

export function StatusDropdown({
  issueId,
  status,
}: {
  issueId: string
  status: string
}) {
  const config = getStatusConfig(status)
  const Icon = config.icon

  const handleSelect = async (newStatus: string) => {
    await trpc.issues.update.mutate({ id: issueId, status: newStatus as IssueStatus })
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" className="h-5 w-5 p-0">
          <Icon className={`h-3.5 w-3.5 ${config.color}`} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {statuses.map((s) => {
          const SIcon = s.icon
          return (
            <DropdownMenuItem
              key={s.value}
              onClick={() => handleSelect(s.value)}
            >
              <SIcon className={`mr-2 h-4 w-4 ${s.color}`} />
              {s.label}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
