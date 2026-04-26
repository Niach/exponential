import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Calendar } from "@/components/ui/calendar"
import { Button } from "@/components/ui/button"
import { CalendarDays } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { formatDate } from "@/lib/utils"
import { formatDateForMutation } from "@/lib/domain"

interface DueDateDropdownProps {
  issueId: string
  dueDate: string | null
}

export function DueDateDropdown({ issueId, dueDate }: DueDateDropdownProps) {
  const dateValue = dueDate ? new Date(dueDate + `T00:00:00`) : undefined

  const handleSelect = async (date: Date | undefined) => {
    await trpc.issues.update.mutate({
      id: issueId,
      dueDate: formatDateForMutation(date),
    })
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" className="h-5 w-full p-0 justify-end gap-1">
          <CalendarDays
            className={`size-3 shrink-0 ${dueDate ? `text-muted-foreground` : `text-muted-foreground/30`}`}
          />
          {dueDate && (
            <span className="text-xs text-muted-foreground whitespace-nowrap">
              {formatDate(dueDate)}
            </span>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="end">
        <Calendar mode="single" selected={dateValue} onSelect={handleSelect} />
      </PopoverContent>
    </Popover>
  )
}
