// shadcn's Calendar on react-day-picker: `classNames` is DayPicker's part map
// (its keys are DayPicker's), the rest are the root, the chevrons, the week
// number cell and the day button.
export const CALENDAR_CLASSES = {
  root: [
    `group/calendar bg-background p-3 [--cell-size:--spacing(8)] [[data-slot=card-content]_&]:bg-transparent [[data-slot=popover-content]_&]:bg-transparent`,
    String.raw`rtl:**:[.rdp-button\_next>svg]:rotate-180`,
    String.raw`rtl:**:[.rdp-button\_previous>svg]:rotate-180`,
  ].join(` `),
  classNames: {
    root: `w-fit`,
    months: `relative flex flex-col gap-4 md:flex-row`,
    month: `flex w-full flex-col gap-4`,
    nav: `absolute inset-x-0 top-0 flex w-full items-center justify-between gap-1`,
    button_previous: `size-(--cell-size) p-0 select-none aria-disabled:opacity-50`,
    button_next: `size-(--cell-size) p-0 select-none aria-disabled:opacity-50`,
    month_caption: `flex h-(--cell-size) w-full items-center justify-center px-(--cell-size)`,
    dropdowns: `flex h-(--cell-size) w-full items-center justify-center gap-1.5 text-sm font-medium`,
    dropdown_root: `relative rounded-md border border-input shadow-xs has-focus:border-ring has-focus:ring-[3px] has-focus:ring-ring/50`,
    dropdown: `absolute inset-0 bg-popover opacity-0`,
    caption_label: `font-medium select-none`,
    table: `w-full border-collapse`,
    weekdays: `flex`,
    weekday: `flex-1 rounded-md text-[0.8rem] font-normal text-muted-foreground select-none`,
    week: `mt-2 flex w-full`,
    week_number_header: `w-(--cell-size) select-none`,
    week_number: `text-[0.8rem] text-muted-foreground select-none`,
    day: `group/day relative aspect-square h-full w-full p-0 text-center select-none [&:last-child[data-selected=true]_button]:rounded-r-md`,
    range_start: `rounded-l-md bg-accent`,
    range_middle: `rounded-none`,
    range_end: `rounded-r-md bg-accent`,
    today: `rounded-md bg-accent text-accent-foreground data-[selected=true]:rounded-none`,
    outside: `text-muted-foreground aria-selected:text-muted-foreground`,
    disabled: `text-muted-foreground opacity-50`,
    hidden: `invisible`,
  },
  /** `caption_label` per `captionLayout`: `label`, else a dropdown. */
  captionLabel: {
    label: `text-sm`,
    dropdown: `flex h-8 items-center gap-1 rounded-md pr-1 pl-2 text-sm [&>svg]:size-3.5 [&>svg]:text-muted-foreground`,
  },
  /** The `day` cell's first-selected corner, with or without week numbers. */
  dayFirst: {
    weekNumbers: `[&:nth-child(2)[data-selected=true]_button]:rounded-l-md`,
    plain: `[&:first-child[data-selected=true]_button]:rounded-l-md`,
  },
  chevron: `size-4`,
  weekNumber: `flex size-(--cell-size) items-center justify-center text-center`,
  dayButton: `flex aspect-square size-auto w-full min-w-(--cell-size) flex-col gap-1 leading-none font-normal group-data-[focused=true]/day:relative group-data-[focused=true]/day:z-10 group-data-[focused=true]/day:border-ring group-data-[focused=true]/day:ring-[3px] group-data-[focused=true]/day:ring-ring/50 data-[range-end=true]:rounded-md data-[range-end=true]:rounded-r-md data-[range-end=true]:bg-primary data-[range-end=true]:text-primary-foreground data-[range-middle=true]:rounded-none data-[range-middle=true]:bg-accent data-[range-middle=true]:text-accent-foreground data-[range-start=true]:rounded-md data-[range-start=true]:rounded-l-md data-[range-start=true]:bg-primary data-[range-start=true]:text-primary-foreground data-[selected-single=true]:bg-primary data-[selected-single=true]:text-primary-foreground dark:hover:text-accent-foreground [&>span]:text-xs [&>span]:opacity-70`,
} as const
