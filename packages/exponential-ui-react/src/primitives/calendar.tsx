import * as React from "react"
import {
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
} from "lucide-react"
import {
  DayPicker,
  getDefaultClassNames,
  type DayButton,
} from "react-day-picker"
import { CALENDAR_CLASSES, buttonVariants } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"
import { Button } from "./button"

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  captionLayout = `label`,
  buttonVariant = `ghost`,
  formatters,
  components,
  ...props
}: React.ComponentProps<typeof DayPicker> & {
  buttonVariant?: React.ComponentProps<typeof Button>[`variant`]
}) {
  const defaultClassNames = getDefaultClassNames()
  const parts = CALENDAR_CLASSES.classNames

  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      className={cn(CALENDAR_CLASSES.root, className)}
      captionLayout={captionLayout}
      formatters={{
        formatMonthDropdown: (date) =>
          date.toLocaleString(`default`, { month: `short` }),
        ...formatters,
      }}
      classNames={{
        root: cn(parts.root, defaultClassNames.root),
        months: cn(parts.months, defaultClassNames.months),
        month: cn(parts.month, defaultClassNames.month),
        nav: cn(parts.nav, defaultClassNames.nav),
        button_previous: cn(
          buttonVariants({ variant: buttonVariant }),
          parts.button_previous,
          defaultClassNames.button_previous
        ),
        button_next: cn(
          buttonVariants({ variant: buttonVariant }),
          parts.button_next,
          defaultClassNames.button_next
        ),
        month_caption: cn(parts.month_caption, defaultClassNames.month_caption),
        dropdowns: cn(parts.dropdowns, defaultClassNames.dropdowns),
        dropdown_root: cn(parts.dropdown_root, defaultClassNames.dropdown_root),
        dropdown: cn(parts.dropdown, defaultClassNames.dropdown),
        caption_label: cn(
          parts.caption_label,
          captionLayout === `label`
            ? CALENDAR_CLASSES.captionLabel.label
            : CALENDAR_CLASSES.captionLabel.dropdown,
          defaultClassNames.caption_label
        ),
        table: parts.table,
        weekdays: cn(parts.weekdays, defaultClassNames.weekdays),
        weekday: cn(parts.weekday, defaultClassNames.weekday),
        week: cn(parts.week, defaultClassNames.week),
        week_number_header: cn(parts.week_number_header, defaultClassNames.week_number_header),
        week_number: cn(parts.week_number, defaultClassNames.week_number),
        day: cn(
          parts.day,
          props.showWeekNumber
            ? CALENDAR_CLASSES.dayFirst.weekNumbers
            : CALENDAR_CLASSES.dayFirst.plain,
          defaultClassNames.day
        ),
        range_start: cn(parts.range_start, defaultClassNames.range_start),
        range_middle: cn(parts.range_middle, defaultClassNames.range_middle),
        range_end: cn(parts.range_end, defaultClassNames.range_end),
        today: cn(parts.today, defaultClassNames.today),
        outside: cn(parts.outside, defaultClassNames.outside),
        disabled: cn(parts.disabled, defaultClassNames.disabled),
        hidden: cn(parts.hidden, defaultClassNames.hidden),
        ...classNames,
      }}
      components={{
        Root: ({ className, rootRef, ...props }) => {
          return (
            <div
              data-slot="calendar"
              ref={rootRef}
              className={cn(className)}
              {...props}
            />
          )
        },
        Chevron: ({ className, orientation, ...props }) => {
          if (orientation === `left`) {
            return (
              <ChevronLeftIcon className={cn(CALENDAR_CLASSES.chevron, className)} {...props} />
            )
          }

          if (orientation === `right`) {
            return (
              <ChevronRightIcon
                className={cn(CALENDAR_CLASSES.chevron, className)}
                {...props}
              />
            )
          }

          return (
            <ChevronDownIcon className={cn(CALENDAR_CLASSES.chevron, className)} {...props} />
          )
        },
        DayButton: CalendarDayButton,
        WeekNumber: ({ children, ...props }) => {
          return (
            <td {...props}>
              <div className={CALENDAR_CLASSES.weekNumber}>
                {children}
              </div>
            </td>
          )
        },
        ...components,
      }}
      {...props}
    />
  )
}

function CalendarDayButton({
  className,
  day,
  modifiers,
  ...props
}: React.ComponentProps<typeof DayButton>) {
  const defaultClassNames = getDefaultClassNames()

  const ref = React.useRef<HTMLButtonElement>(null)
  React.useEffect(() => {
    if (modifiers.focused) ref.current?.focus()
  }, [modifiers.focused])

  return (
    <Button
      ref={ref}
      variant="ghost"
      size="icon"
      data-day={day.date.toLocaleDateString()}
      data-selected-single={
        modifiers.selected &&
        !modifiers.range_start &&
        !modifiers.range_end &&
        !modifiers.range_middle
      }
      data-range-start={modifiers.range_start}
      data-range-end={modifiers.range_end}
      data-range-middle={modifiers.range_middle}
      className={cn(
        CALENDAR_CLASSES.dayButton,
        defaultClassNames.day,
        className
      )}
      {...props}
    />
  )
}

export { Calendar, CalendarDayButton }
