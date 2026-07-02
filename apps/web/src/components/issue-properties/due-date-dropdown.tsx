import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Calendar } from "@/components/ui/calendar"
import { Button } from "@/components/ui/button"
import { CalendarDays } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { formatDate, parseLocalDate } from "@/lib/utils"
import { formatDateForMutation } from "@/lib/domain"

interface DueDateDropdownProps {
  issueId: string
  dueDate: string | null
  disabled?: boolean
}

export function DueDateDropdown({
  issueId,
  dueDate,
  disabled,
}: DueDateDropdownProps) {
  const dateValue = dueDate ? parseLocalDate(dueDate) : undefined

  const handleSelect = async (date: Date | undefined) => {
    await trpc.issues.update.mutate({
      id: issueId,
      dueDate: formatDateForMutation(date),
    })
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          className={
            dueDate ? `h-5 gap-1 px-1 py-0 has-[>svg]:px-1` : `h-5 w-5 p-0`
          }
          disabled={disabled}
        >
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
