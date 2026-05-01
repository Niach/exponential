import { CalendarDays, X } from "lucide-react"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Calendar } from "@/components/ui/calendar"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { RecurrenceUnit } from "@/lib/domain"
import { recurrenceUnitValues } from "@/lib/domain"
import { formatDate } from "@/lib/utils"

export interface RecurrenceValue {
  firstDue: Date | undefined
  interval: number
  unit: RecurrenceUnit
}

interface RecurrenceEditorProps {
  disabled?: boolean
  onChange: (value: RecurrenceValue | null) => void
  value: RecurrenceValue
}

const intervalOptions = [1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 21, 30]

export function RecurrenceEditor({
  disabled,
  onChange,
  value,
}: RecurrenceEditorProps) {
  return (
    <div className="flex items-center gap-2 text-sm">
      <span className="text-muted-foreground">First due</span>

      <Popover>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="outline"
            size="xs"
            disabled={disabled}
            className="gap-1.5"
          >
            <CalendarDays className="size-3" />
            {value.firstDue ? formatDate(value.firstDue) : `Pick date`}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="single"
            selected={value.firstDue}
            onSelect={(date) => onChange({ ...value, firstDue: date })}
          />
        </PopoverContent>
      </Popover>

      <span className="text-muted-foreground">repeats every</span>

      <Select
        value={String(value.interval)}
        disabled={disabled}
        onValueChange={(next) => onChange({ ...value, interval: Number(next) })}
      >
        <SelectTrigger size="sm" className="h-7 px-2 py-0">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {intervalOptions.map((n) => (
            <SelectItem key={n} value={String(n)}>
              {n}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Select
        value={value.unit}
        disabled={disabled}
        onValueChange={(next) =>
          onChange({ ...value, unit: next as RecurrenceUnit })
        }
      >
        <SelectTrigger size="sm" className="h-7 px-2 py-0">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {recurrenceUnitValues.map((unit) => (
            <SelectItem key={unit} value={unit}>
              {value.interval === 1 ? unit : `${unit}s`}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label="Stop recurrence"
        disabled={disabled}
        onClick={() => onChange(null)}
        className="text-muted-foreground"
      >
        <X className="size-3" />
      </Button>
    </div>
  )
}
